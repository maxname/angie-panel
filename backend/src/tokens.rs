//! API tokens — non-browser access to the same REST API (`apctl`, CI, Ansible).
//!
//! Three rules shape this module:
//!   * a token **inherits its owner's role**, so a viewer cannot mint an admin
//!     token and demoting a user immediately demotes their tokens;
//!   * you manage **your own** tokens; an admin additionally sees and revokes
//!     everyone's (hence the self-service exemption in `security`);
//!   * a token **cannot mint tokens** — otherwise one leaked secret spawns
//!     successors that outlive revoking the original.
//!
//! The secret is returned exactly once, at creation. Only its SHA-256 and an
//! 8-char prefix are stored (see `0026_api_tokens.sql` for why not argon2).

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{self, AuthUser};
use crate::db::now_epoch;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Row shape shared by list and create responses. Never carries the secret.
type TokenRow = (
    i64,
    String,
    String,
    Option<String>,
    i64,
    i64,
    Option<i64>,
    Option<i64>,
);

fn token_json(r: &TokenRow) -> Value {
    let (id, name, prefix, owner, is_local, created_at, last_used_at, expires_at) = r;
    json!({
        "id": id,
        "name": name,
        // Enough to recognize which secret this is, useless as a credential.
        "prefix": format!("{}{}…", auth::API_TOKEN_PREFIX, prefix),
        "owner": owner,
        "is_local": *is_local == 1,
        "created_at": created_at,
        "last_used_at": last_used_at,
        "expires_at": expires_at,
    })
}

const SELECT_COLUMNS: &str = "t.id, t.name, t.prefix, u.email, t.is_local, t.created_at, \
                              t.last_used_at, t.expires_at";

/// GET /api/tokens — your own tokens; admins see every token on the box.
pub async fn list(user: AuthUser, State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM api_tokens t LEFT JOIN users u ON u.id = t.user_id \
         WHERE ?1 = 1 OR t.user_id = ?2 ORDER BY t.created_at DESC"
    );
    let rows: Vec<TokenRow> = sqlx::query_as(&sql)
        .bind(i32::from(user.is_admin()))
        .bind(user.user_id)
        .fetch_all(&state.db)
        .await?;
    let arr: Vec<Value> = rows.iter().map(token_json).collect();
    Ok(Json(json!({ "tokens": arr })))
}

#[derive(Deserialize)]
pub struct CreateToken {
    name: String,
    /// Optional lifetime. Omitted = never expires.
    #[serde(default)]
    expires_in_days: Option<i64>,
}

/// POST /api/tokens — mint a token for the calling user. Exempt from the admin
/// gate (a viewer's token is a viewer token), but session-only.
pub async fn create(
    user: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateToken>,
) -> ApiResult<Json<Value>> {
    user.require_session()?;
    let user_id = user.require_user_id()?;
    let name = auth::normalize_token_name(&req.name)?;

    let expires_at = match req.expires_in_days {
        None => None,
        Some(d) if (1..=3650).contains(&d) => Some(now_epoch() + d * 86_400),
        Some(_) => {
            return Err(ApiError::bad_request(
                "invalid_expiry",
                "expires_in_days must be between 1 and 3650",
            ))
        }
    };

    let (secret, hash, prefix) = auth::new_api_token()?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO api_tokens (name, token_hash, prefix, user_id, is_local, created_at, expires_at) \
         VALUES (?, ?, ?, ?, 0, ?, ?) RETURNING id",
    )
    .bind(&name)
    .bind(&hash)
    .bind(&prefix)
    .bind(user_id)
    .bind(now_epoch())
    .bind(expires_at)
    .fetch_one(&state.db)
    .await?;

    tracing::info!(%name, owner = %user.email, "API token created");
    Ok(Json(json!({
        "id": id,
        "name": name,
        // The one and only time this value exists outside the caller's machine.
        "secret": secret,
    })))
}

/// DELETE /api/tokens/{id} — revoke. Yours always; anyone's if you are an admin.
pub async fn delete(
    user: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let row: Option<(Option<i64>, i64)> =
        sqlx::query_as("SELECT user_id, is_local FROM api_tokens WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let (owner_id, is_local) = row.ok_or_else(|| ApiError::not_found(format!("no token #{id}")))?;

    if is_local == 1 {
        // Revoking it here would leave the file on disk pointing at nothing and
        // the next restart would just mint another. Rotation is a file op.
        return Err(ApiError::bad_request(
            "local_token",
            "the local apctl token is managed on disk: delete \
             /var/lib/angie-panel/cli-token and restart the service to rotate it",
        ));
    }
    if !user.is_admin() && owner_id != user.user_id {
        return Err(ApiError::forbidden(
            "forbidden",
            "you can only revoke your own tokens",
        ));
    }

    sqlx::query("DELETE FROM api_tokens WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    tracing::info!(token_id = id, by = %user.email, "API token revoked");
    Ok(Json(json!({ "ok": true })))
}
