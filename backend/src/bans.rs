//! IP blocklist ("banned IPs") — the panel-native enforcement point that
//! blocks abusive addresses (the outcome fail2ban / CrowdSec provide). Bans are
//! generated into a global http-scope `deny` list (03-bans.conf) and take
//! effect on the next Apply, like every other config change.
//!
//! Mutations are admin-only (enforced centrally in `security::security_layer`).

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::model::{self, BanInput};
use crate::repo;
use crate::state::AppState;

pub async fn list(_u: AuthUser, State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let bans = repo::list_bans(&state.db).await?;
    let arr: Vec<Value> = bans
        .iter()
        .map(|b| serde_json::to_value(b).unwrap_or(Value::Null))
        .collect();
    Ok(Json(json!({ "bans": arr })))
}

pub async fn create(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(raw): Json<BanInput>,
) -> ApiResult<Json<Value>> {
    let input = model::validate_ban(raw)?;
    if repo::ban_address_exists(&state.db, &input.address).await? {
        return Err(ApiError::new(
            axum::http::StatusCode::CONFLICT,
            "already_banned",
            format!("{} is already on the blocklist", input.address),
        ));
    }
    let id = repo::insert_ban(&state.db, &input).await?;
    let ban = repo::get_ban(&state.db, id).await?.expect("just inserted");
    Ok(Json(serde_json::to_value(&ban).unwrap_or(Value::Null)))
}

pub async fn delete(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    if !repo::delete_ban(&state.db, id).await? {
        return Err(ApiError::not_found(format!("no ban #{id}")));
    }
    Ok(Json(json!({ "ok": true })))
}
