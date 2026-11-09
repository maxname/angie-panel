//! SNI passthrough routers (Angie `stream {}` context). One router is a single
//! stream listener that inspects the TLS ClientHello (`ssl_preread`) and forwards
//! the raw connection — WITHOUT terminating TLS — to a backend chosen by the SNI
//! hostname. It lets several TLS services share one public port, each keeping its
//! own certificate end-to-end (unlike a proxy host, which terminates TLS).
//!
//! Like streams, a router is keyed by its **incoming port**: it listens TCP in
//! the stream context, so it cannot share a port with another enabled router or
//! an enabled TCP stream. We reject the conflict up front rather than letting
//! `angie -t` / the bind fail at apply time. Routers ride the same
//! enable-streams gate as streams (they emit `stream.d/` files).

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::model::{self, SniRouterInput, UpstreamPolicy};
use crate::repo;
use crate::state::AppState;

fn upstream_policy(state: &AppState) -> UpstreamPolicy {
    UpstreamPolicy {
        allow_loopback: state.cfg.allow_loopback_upstreams,
    }
}

fn port_conflict(port: u16, what: String) -> ApiError {
    ApiError::new(
        axum::http::StatusCode::CONFLICT,
        "port_conflict",
        format!("port {port} is already used by {what}"),
    )
}

/// A router listens TCP on its incoming port. Reject a collision with another
/// enabled router, or with an enabled stream's TCP listener. `exclude` skips the
/// router being updated (0 = create).
async fn check_port_free(state: &AppState, input: &SniRouterInput, exclude: i64) -> ApiResult<()> {
    if !input.enabled {
        return Ok(());
    }
    for r in repo::list_sni_routers(&state.db).await? {
        if r.id != exclude && r.enabled && r.incoming_port == input.incoming_port {
            return Err(port_conflict(
                input.incoming_port,
                format!("SNI router #{}", r.id),
            ));
        }
    }
    for s in repo::list_streams(&state.db).await? {
        if s.enabled && s.tcp && s.incoming_port == input.incoming_port {
            return Err(port_conflict(
                input.incoming_port,
                format!("stream #{}", s.id),
            ));
        }
    }
    Ok(())
}

pub async fn list(_u: AuthUser, State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let routers = repo::list_sni_routers(&state.db).await?;
    let arr: Vec<Value> = routers
        .iter()
        .map(|r| serde_json::to_value(r).unwrap_or(Value::Null))
        .collect();
    Ok(Json(json!({ "sni_routers": arr })))
}

pub async fn get_one(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let r = repo::get_sni_router(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no SNI router #{id}")))?;
    Ok(Json(serde_json::to_value(&r).unwrap_or(Value::Null)))
}

pub async fn create(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(raw): Json<SniRouterInput>,
) -> ApiResult<Json<Value>> {
    let input = model::validate_sni_router_input(raw, &upstream_policy(&state))?;
    check_port_free(&state, &input, 0).await?;
    let id = repo::insert_sni_router(&state.db, &input).await?;
    let r = repo::get_sni_router(&state.db, id)
        .await?
        .expect("just inserted");
    Ok(Json(serde_json::to_value(&r).unwrap_or(Value::Null)))
}

pub async fn update(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(raw): Json<SniRouterInput>,
) -> ApiResult<Json<Value>> {
    let input = model::validate_sni_router_input(raw, &upstream_policy(&state))?;
    check_port_free(&state, &input, id).await?;
    if !repo::update_sni_router(&state.db, id, &input).await? {
        return Err(ApiError::not_found(format!("no SNI router #{id}")));
    }
    let r = repo::get_sni_router(&state.db, id)
        .await?
        .expect("just updated");
    Ok(Json(serde_json::to_value(&r).unwrap_or(Value::Null)))
}

pub async fn delete(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    if !repo::delete_sni_router(&state.db, id).await? {
        return Err(ApiError::not_found(format!("no SNI router #{id}")));
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn enable(
    u: AuthUser,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> ApiResult<Json<Value>> {
    set_enabled(u, state, id, true).await
}

pub async fn disable(
    u: AuthUser,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> ApiResult<Json<Value>> {
    set_enabled(u, state, id, false).await
}

async fn set_enabled(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    enabled: bool,
) -> ApiResult<Json<Value>> {
    // Re-enabling can resurrect a port conflict — check the intended (enabled)
    // shape before flipping the flag.
    if enabled {
        if let Some(r) = repo::get_sni_router(&state.db, id).await? {
            let intended = SniRouterInput {
                name: r.name,
                incoming_port: r.incoming_port,
                routes: r.routes,
                default_host: r.default_host,
                default_port: r.default_port,
                enabled: true,
            };
            check_port_free(&state, &intended, id).await?;
        }
    }
    if !repo::set_sni_router_enabled(&state.db, id, enabled).await? {
        return Err(ApiError::not_found(format!("no SNI router #{id}")));
    }
    Ok(Json(json!({ "ok": true, "enabled": enabled })))
}
