//! Multi-user management (v2). Admins create/manage operators; viewers are
//! read-only. Authorization is enforced centrally in `security::security_layer`
//! (every mutation requires an admin, except the self-service allowlist), so
//! these handlers focus on the domain rules — email/password validation,
//! and the two safety invariants:
//!   * never delete or demote the **last admin** (would lock everyone out);
//!   * never let a user delete **their own** account from under themselves.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{self, AuthUser, Role, MIN_PASSWORD_LEN};
use crate::error::{ApiError, ApiResult};
use crate::repo::{self, UserRow};
use crate::state::AppState;

fn user_json(u: &UserRow) -> Value {
    json!({ "id": u.id, "email": u.email, "role": u.role, "created_at": u.created_at })
}

fn parse_role(raw: &str) -> ApiResult<Role> {
    match raw {
        "admin" => Ok(Role::Admin),
        "viewer" => Ok(Role::Viewer),
        _ => Err(ApiError::bad_request(
            "invalid_role",
            "role must be admin or viewer",
        )),
    }
}

fn check_password(password: &str) -> ApiResult<()> {
    if password.len() < MIN_PASSWORD_LEN {
        return Err(ApiError::bad_request(
            "weak_password",
            format!("password must be at least {MIN_PASSWORD_LEN} characters"),
        ));
    }
    Ok(())
}

/// GET /api/users — admin only (the method-based middleware doesn't gate GETs).
pub async fn list(user: AuthUser, State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let users = repo::list_users(&state.db).await?;
    let arr: Vec<Value> = users.iter().map(user_json).collect();
    Ok(Json(json!({ "users": arr })))
}

#[derive(Deserialize)]
pub struct CreateUser {
    email: String,
    password: String,
    #[serde(default)]
    role: Option<String>,
}

/// POST /api/users — create an operator (admin only, via the middleware gate).
pub async fn create(
    _user: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateUser>,
) -> ApiResult<Json<Value>> {
    let email = auth::normalize_email(&req.email)?;
    check_password(&req.password)?;
    let role = parse_role(req.role.as_deref().unwrap_or("viewer"))?;
    if repo::user_email_exists(&state.db, &email).await? {
        return Err(ApiError::new(
            axum::http::StatusCode::CONFLICT,
            "email_taken",
            "a user with that email already exists",
        ));
    }
    let hash = auth::hash_password(&state, req.password).await?;
    let id = repo::insert_user(&state.db, &email, &hash, role.as_str()).await?;
    let created = repo::get_user(&state.db, id).await?.expect("just inserted");
    Ok(Json(user_json(&created)))
}

#[derive(Deserialize)]
pub struct RoleUpdate {
    role: String,
}

/// PUT /api/users/{id}/role — change a user's role (admin only). Refuses to
/// demote the last remaining admin.
pub async fn update_role(
    _user: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(req): Json<RoleUpdate>,
) -> ApiResult<Json<Value>> {
    let new_role = parse_role(&req.role)?;
    let target = repo::get_user(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no user #{id}")))?;
    // Demoting the only admin would lock everyone out of write access.
    if Role::from_str(&target.role) == Role::Admin
        && new_role != Role::Admin
        && repo::count_admins(&state.db).await? <= 1
    {
        return Err(last_admin_error());
    }
    repo::set_user_role(&state.db, id, new_role.as_str()).await?;
    let updated = repo::get_user(&state.db, id).await?.expect("just updated");
    Ok(Json(user_json(&updated)))
}

/// DELETE /api/users/{id} — remove an operator (admin only). Refuses to delete
/// yourself or the last admin.
pub async fn delete(
    user: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    // Also blocks deleting the account an API token belongs to, since the
    // token's user_id is the owner's.
    if user.user_id == Some(id) {
        return Err(ApiError::bad_request(
            "cannot_delete_self",
            "you cannot delete your own account",
        ));
    }
    let target = repo::get_user(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no user #{id}")))?;
    if Role::from_str(&target.role) == Role::Admin && repo::count_admins(&state.db).await? <= 1 {
        return Err(last_admin_error());
    }
    repo::delete_user(&state.db, id).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct PasswordChange {
    current_password: String,
    new_password: String,
}

/// POST /api/users/me/password — change your OWN password (any authenticated
/// user, incl. viewers). Requires the current password.
pub async fn change_own_password(
    user: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(req): Json<PasswordChange>,
) -> ApiResult<Json<Value>> {
    // Password changes are a browser-session flow: an API token proves nothing
    // about knowing the password, and the local token has no account at all.
    user.require_session()?;
    let user_id = user.require_user_id()?;
    check_password(&req.new_password)?;
    let hash = repo::user_password_hash(&state.db, user_id)
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    if !auth::verify_password(&state, req.current_password, hash).await? {
        return Err(ApiError::forbidden(
            "wrong_password",
            "the current password is incorrect",
        ));
    }
    let new_hash = auth::hash_password(&state, req.new_password).await?;
    repo::set_user_password(&state.db, user_id, &new_hash).await?;
    Ok(Json(json!({ "ok": true })))
}

fn last_admin_error() -> ApiError {
    ApiError::bad_request(
        "last_admin",
        "cannot remove the last administrator — promote another user to admin first",
    )
}
