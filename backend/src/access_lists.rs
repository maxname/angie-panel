//! Access-list CRUD (v2): basic-auth users + IP allow/deny rules, attachable
//! to proxy hosts. Passwords are bcrypt-hashed here (never stored plaintext,
//! never returned to the client); on update a user whose password is omitted
//! keeps its existing hash.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::model::{self, AccessList, AccessListInput};
use crate::repo::{self, AclUserHash};
use crate::state::AppState;

const BCRYPT_COST: u32 = 10;

fn acl_json(l: &AccessList) -> Value {
    serde_json::to_value(l).unwrap_or(Value::Null)
}

pub async fn list(_u: AuthUser, State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let lists = repo::list_access_lists(&state.db).await?;
    let arr: Vec<Value> = lists.iter().map(acl_json).collect();
    Ok(Json(json!({ "access_lists": arr })))
}

pub async fn get_one(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let list = repo::get_access_list(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no access list #{id}")))?;
    Ok(Json(acl_json(&list)))
}

/// Resolve the final (username, hash) set: hash new passwords, preserve
/// existing ones. `existing` maps username → current hash (empty on create).
fn resolve_user_hashes(
    input: &AccessListInput,
    existing: &HashMap<String, String>,
) -> ApiResult<Vec<AclUserHash>> {
    let mut out = Vec::with_capacity(input.users.len());
    for u in &input.users {
        let hash = match &u.password {
            Some(pw) => bcrypt::hash(pw, BCRYPT_COST)
                .map_err(|e| ApiError::internal(format!("bcrypt: {e}")))?,
            None => existing.get(&u.username).cloned().ok_or_else(|| {
                ApiError::bad_request(
                    "password_required",
                    format!("user '{}' is new and needs a password", u.username),
                )
            })?,
        };
        out.push(AclUserHash {
            username: u.username.clone(),
            password_hash: hash,
        });
    }
    Ok(out)
}

pub async fn create(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(raw): Json<AccessListInput>,
) -> ApiResult<Json<Value>> {
    let input = model::validate_acl_input(raw)?;
    let hashes = resolve_user_hashes(&input, &HashMap::new())?;
    let id = repo::upsert_access_list(&state.db, None, &input, &hashes).await?;
    let list = repo::get_access_list(&state.db, id)
        .await?
        .expect("just inserted");
    Ok(Json(acl_json(&list)))
}

pub async fn update(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(raw): Json<AccessListInput>,
) -> ApiResult<Json<Value>> {
    if repo::get_access_list(&state.db, id).await?.is_none() {
        return Err(ApiError::not_found(format!("no access list #{id}")));
    }
    let input = model::validate_acl_input(raw)?;
    let existing: HashMap<String, String> = repo::acl_user_hashes(&state.db, id)
        .await?
        .into_iter()
        .map(|u| (u.username, u.password_hash))
        .collect();
    let hashes = resolve_user_hashes(&input, &existing)?;
    repo::upsert_access_list(&state.db, Some(id), &input, &hashes).await?;
    let list = repo::get_access_list(&state.db, id)
        .await?
        .expect("just updated");
    Ok(Json(acl_json(&list)))
}

pub async fn delete(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    // Refuse to delete a list a host still references (would silently drop the
    // host's access control on the next apply).
    let referencing = repo::hosts_using_access_list(&state.db, id).await?;
    if !referencing.is_empty() {
        let list = referencing
            .iter()
            .map(|(hid, dom)| format!("#{hid} ({dom})"))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(ApiError::new(
            axum::http::StatusCode::CONFLICT,
            "access_list_in_use",
            format!("access list is used by host(s) {list}; detach them first"),
        ));
    }
    if !repo::delete_access_list(&state.db, id).await? {
        return Err(ApiError::not_found(format!("no access list #{id}")));
    }
    Ok(Json(json!({ "ok": true })))
}
