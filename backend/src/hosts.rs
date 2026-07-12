//! Proxy-host CRUD handlers. Every mutation validates through
//! `model::validate_host_input` (allowlist-and-reject) and enforces the
//! domain-uniqueness rule before touching the DB. Nothing here writes Angie
//! config — changes materialize only on the next apply.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::model::{self, ProxyHost, ProxyHostInput, UpstreamPolicy};
use crate::repo::{self, HostKind};
use crate::state::AppState;

fn upstream_policy(state: &AppState) -> UpstreamPolicy {
    UpstreamPolicy {
        allow_loopback: state.cfg.allow_loopback_upstreams,
    }
}

/// Enforce: a domain (after normalization) may belong to at most one ENABLED
/// host of ANY type (proxy / redirect / 404). `exclude_id` skips the proxy
/// host being updated.
async fn check_domain_uniqueness(
    state: &AppState,
    input: &ProxyHostInput,
    exclude_id: Option<i64>,
) -> ApiResult<()> {
    if !input.enabled {
        return Ok(());
    }
    let skip = exclude_id.map(|id| (HostKind::Proxy, id));
    let taken = repo::all_enabled_domains(&state.db, skip).await?;
    for d in &input.domains {
        if let Some((kind, id)) = taken.get(d) {
            return Err(ApiError::new(
                axum::http::StatusCode::CONFLICT,
                "domain_conflict",
                format!("domain {d} already belongs to {} #{id}", kind.label()),
            ));
        }
    }
    Ok(())
}

fn host_json(h: &ProxyHost) -> Value {
    serde_json::to_value(h).unwrap_or(Value::Null)
}

pub async fn list(_u: AuthUser, State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let hosts = repo::list_hosts(&state.db).await?;
    let arr: Vec<Value> = hosts.iter().map(host_json).collect();
    Ok(Json(json!({ "hosts": arr })))
}

pub async fn get_one(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let host = repo::get_host(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no host #{id}")))?;
    Ok(Json(host_json(&host)))
}

pub async fn create(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(raw): Json<ProxyHostInput>,
) -> ApiResult<Json<Value>> {
    let input = model::validate_host_input(
        raw,
        state.cfg.allow_advanced_snippets,
        &upstream_policy(&state),
    )?;
    check_domain_uniqueness(&state, &input, None).await?;
    let id = repo::insert_host(&state.db, &input).await?;
    let host = repo::get_host(&state.db, id).await?.expect("just inserted");
    Ok(Json(host_json(&host)))
}

pub async fn update(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(raw): Json<ProxyHostInput>,
) -> ApiResult<Json<Value>> {
    let input = model::validate_host_input(
        raw,
        state.cfg.allow_advanced_snippets,
        &upstream_policy(&state),
    )?;
    check_domain_uniqueness(&state, &input, Some(id)).await?;
    if !repo::update_host(&state.db, id, &input).await? {
        return Err(ApiError::not_found(format!("no host #{id}")));
    }
    let host = repo::get_host(&state.db, id).await?.expect("just updated");
    Ok(Json(host_json(&host)))
}

pub async fn delete(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    if !repo::delete_host(&state.db, id).await? {
        return Err(ApiError::not_found(format!("no host #{id}")));
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
    // Re-check uniqueness when enabling: a disabled host may hold a domain that
    // another enabled host has since claimed.
    if enabled {
        if let Some(host) = repo::get_host(&state.db, id).await? {
            let as_input = host_to_input(&host, true);
            check_domain_uniqueness(&state, &as_input, Some(id)).await?;
        }
    }
    if !repo::set_enabled(&state.db, id, enabled).await? {
        return Err(ApiError::not_found(format!("no host #{id}")));
    }
    Ok(Json(json!({ "ok": true, "enabled": enabled })))
}

/// Minimal ProxyHost → ProxyHostInput projection for the uniqueness re-check.
fn host_to_input(h: &ProxyHost, enabled: bool) -> ProxyHostInput {
    ProxyHostInput {
        domains: h.domains.clone(),
        forward_scheme: h.forward_scheme,
        forward_host: h.forward_host.clone(),
        forward_port: h.forward_port,
        websockets_upgrade: h.websockets_upgrade,
        block_exploits: h.block_exploits,
        cache_assets: h.cache_assets,
        http2: h.http2,
        http3: h.http3,
        force_ssl: h.force_ssl,
        hsts: h.hsts,
        hsts_subdomains: h.hsts_subdomains,
        trust_forwarded_proto: h.trust_forwarded_proto,
        certificate_id: h.certificate_id,
        access_list_id: h.access_list_id,
        locations: h.locations.clone(),
        advanced_snippet: h.advanced_snippet.clone(),
        rate_limit: h.rate_limit.clone(),
        upstream: h.upstream.clone(),
        mtls: h.mtls.clone(),
        forward_auth: h.forward_auth.clone(),
        custom_headers: h.custom_headers.clone(),
        maintenance: h.maintenance.clone(),
        gzip: h.gzip.clone(),
        enabled,
    }
}
