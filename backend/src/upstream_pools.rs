//! Shared upstream pools: a named set of backend servers (with a balancing
//! method and passive health) defined once and referenced by any number of
//! proxy hosts through `upstream_pool_id` — instead of retyping the same
//! servers into every host, and editing each of them when a backend moves.
//!
//! Servers go through the SAME SSRF-guarded validation as a host's own
//! upstream. Nothing here writes Angie config: a pool only reaches the
//! generated files through the hosts that reference it, on the next apply.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::model::{self, UpstreamPolicy, UpstreamPool, UpstreamPoolInput};
use crate::repo;
use crate::state::AppState;

fn upstream_policy(state: &AppState) -> UpstreamPolicy {
    UpstreamPolicy {
        allow_loopback: state.cfg.allow_loopback_upstreams,
    }
}

fn pool_json(p: &UpstreamPool) -> Value {
    serde_json::to_value(p).unwrap_or(Value::Null)
}

fn not_found(id: i64) -> ApiError {
    ApiError::not_found(format!("no upstream pool #{id}"))
}

/// Pools are picked by name in the host editor, so two with the same name
/// would be indistinguishable there.
async fn ensure_name_free(state: &AppState, name: &str, except: Option<i64>) -> ApiResult<()> {
    if repo::upstream_pool_name_taken(&state.db, name, except).await? {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "upstream_pool_name_taken",
            format!("an upstream pool named '{name}' already exists"),
        ));
    }
    Ok(())
}

pub async fn list(_u: AuthUser, State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let pools = repo::list_upstream_pools(&state.db).await?;
    let arr: Vec<Value> = pools.iter().map(pool_json).collect();
    Ok(Json(json!({ "upstream_pools": arr })))
}

pub async fn get_one(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let pool = repo::get_upstream_pool(&state.db, id)
        .await?
        .ok_or_else(|| not_found(id))?;
    Ok(Json(pool_json(&pool)))
}

pub async fn create(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(raw): Json<UpstreamPoolInput>,
) -> ApiResult<Json<Value>> {
    let input = model::validate_upstream_pool_input(raw, &upstream_policy(&state))?;
    ensure_name_free(&state, &input.name, None).await?;
    let id = repo::insert_upstream_pool(&state.db, &input).await?;
    let pool = repo::get_upstream_pool(&state.db, id)
        .await?
        .expect("just inserted");
    Ok(Json(pool_json(&pool)))
}

pub async fn update(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(raw): Json<UpstreamPoolInput>,
) -> ApiResult<Json<Value>> {
    if repo::get_upstream_pool(&state.db, id).await?.is_none() {
        return Err(not_found(id));
    }
    let input = model::validate_upstream_pool_input(raw, &upstream_policy(&state))?;
    ensure_name_free(&state, &input.name, Some(id)).await?;
    if !repo::update_upstream_pool(&state.db, id, &input).await? {
        return Err(not_found(id));
    }
    let pool = repo::get_upstream_pool(&state.db, id)
        .await?
        .expect("just updated");
    Ok(Json(pool_json(&pool)))
}

pub async fn delete(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    // Refuse to delete a pool a host still proxies to: that host would be left
    // with no backend at all. (The FK is ON DELETE RESTRICT as a backstop; this
    // check is what turns it into a message that names the hosts.)
    let referencing = repo::hosts_using_upstream_pool(&state.db, id).await?;
    if !referencing.is_empty() {
        let list = referencing
            .iter()
            .map(|(hid, dom)| format!("#{hid} ({dom})"))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "upstream_pool_in_use",
            format!("upstream pool is used by host(s) {list}; point them elsewhere first"),
        ));
    }
    if !repo::delete_upstream_pool(&state.db, id).await? {
        return Err(not_found(id));
    }
    Ok(Json(json!({ "ok": true })))
}
