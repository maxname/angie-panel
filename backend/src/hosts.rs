//! Proxy-host CRUD handlers. Every mutation validates through
//! `model::validate_host_input` (allowlist-and-reject) and enforces the
//! domain-uniqueness rule before touching the DB. Nothing here writes Angie
//! config — changes materialize only on the next apply.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::certs;
use crate::error::{ApiError, ApiResult};
use crate::model::{
    self, CertificateInput, Challenge, KeyType, ProxyHost, ProxyHostInput, UpstreamPolicy,
};
use crate::repo::{self, HostKind};
use crate::settings;
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

/// Reject a create/update that references a certificate or access list which
/// doesn't exist. Without this the generator would emit a dangling
/// `$acme_cert_<name>` reference, or (for a bad access_list_id) the host would
/// silently lose its intended IP/basic-auth restriction — a fail-open. Streams
/// already do the cert check; hosts must too.
async fn check_refs(state: &AppState, input: &ProxyHostInput) -> ApiResult<()> {
    if let Some(cid) = input.certificate_id {
        if repo::get_cert(&state.db, cid).await?.is_none() {
            return Err(ApiError::not_found(format!("no certificate #{cid}")));
        }
    }
    if let Some(aid) = input.access_list_id {
        if repo::get_access_list(&state.db, aid).await?.is_none() {
            return Err(ApiError::not_found(format!("no access list #{aid}")));
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

/// Host create/update body: the host itself, plus an optional request to issue
/// a new certificate for the host's domains in the same call.
#[derive(Deserialize)]
pub struct HostBody {
    #[serde(flatten)]
    host: ProxyHostInput,
    #[serde(default)]
    new_certificate: Option<NewCertificate>,
}

/// How to issue the certificate a host asks for. The domains are the host's;
/// everything omitted falls back to the global ACME settings (default CA,
/// contact email) or the certificate defaults.
#[derive(Deserialize)]
pub struct NewCertificate {
    #[serde(default)]
    challenge: Option<Challenge>,
    #[serde(default)]
    dns_provider: Option<String>,
    #[serde(default)]
    ca: Option<String>,
    #[serde(default)]
    key_type: Option<KeyType>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    staging: bool,
}

/// Validate the host, then — when asked — create its certificate and bind it.
/// The certificate is created after every host check has passed, so a host
/// the panel would reject never leaves an orphan certificate behind.
async fn prepare(
    state: &AppState,
    body: HostBody,
    exclude_id: Option<i64>,
) -> ApiResult<(ProxyHostInput, Option<i64>)> {
    let mut input = model::validate_host_input(
        body.host,
        state.cfg.allow_advanced_snippets,
        &upstream_policy(state),
    )?;
    check_domain_uniqueness(state, &input, exclude_id).await?;
    check_refs(state, &input).await?;
    let Some(req) = body.new_certificate else {
        return Ok((input, None));
    };
    if input.certificate_id.is_some() {
        return Err(ApiError::bad_request(
            "conflicting_certificate",
            "pick an existing certificate or request a new one, not both",
        ));
    }
    let map = repo::all_settings(&state.db).await?;
    let cert = certs::create_cert(
        state,
        CertificateInput {
            name: String::new(),
            domains: input.domains.clone(),
            challenge: req.challenge.unwrap_or(Challenge::Http),
            key_type: req.key_type.unwrap_or(KeyType::Ecdsa),
            email: req
                .email
                .or_else(|| map.get(settings::KEY_ACME_EMAIL).cloned()),
            staging: req.staging,
            dns_provider: req.dns_provider,
            ca: req
                .ca
                .or_else(|| map.get(settings::KEY_ACME_DEFAULT_CA).cloned())
                .unwrap_or_else(model::default_ca),
        },
    )
    .await?;
    input.certificate_id = Some(cert.id);
    Ok((input, Some(cert.id)))
}

/// Undo a certificate created for a host whose own write then failed.
async fn discard_cert(state: &AppState, created: Option<i64>) {
    if let Some(cid) = created {
        if let Err(e) = repo::delete_cert(&state.db, cid).await {
            tracing::warn!(cert = cid, error = %e, "could not remove certificate of a failed host write");
        }
    }
}

pub async fn create(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<HostBody>,
) -> ApiResult<Json<Value>> {
    let (input, created) = prepare(&state, body, None).await?;
    let id = match repo::insert_host(&state.db, &input).await {
        Ok(id) => id,
        Err(e) => {
            discard_cert(&state, created).await;
            return Err(e.into());
        }
    };
    let host = repo::get_host(&state.db, id).await?.expect("just inserted");
    Ok(Json(host_json(&host)))
}

pub async fn update(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(body): Json<HostBody>,
) -> ApiResult<Json<Value>> {
    if repo::get_host(&state.db, id).await?.is_none() {
        return Err(ApiError::not_found(format!("no host #{id}")));
    }
    let (input, created) = prepare(&state, body, Some(id)).await?;
    match repo::update_host(&state.db, id, &input).await {
        Ok(true) => {}
        Ok(false) => {
            discard_cert(&state, created).await;
            return Err(ApiError::not_found(format!("no host #{id}")));
        }
        Err(e) => {
            discard_cert(&state, created).await;
            return Err(e.into());
        }
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
        health_checks: h.health_checks.clone(),
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
        error_pages: h.error_pages.clone(),
        proxy_tuning: h.proxy_tuning.clone(),
        enabled,
    }
}

/// GET /api/hosts/{id}/health — the recent beats for the uptime bars.
///
/// One flat array covering every kind the host checks; the UI groups by kind.
/// Capped so a chatty host with a short interval cannot ask for its whole 30-day
/// history in one call — the bar only ever shows the tail.
pub async fn health(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    // Enough to fill the bar (Kuma-style ~50) for each of two kinds, with slack.
    const PER_KIND: i64 = 60;
    let mut beats: Vec<Value> = Vec::new();
    for kind in ["tcp", "http"] {
        for (ts, ok, latency_ms, error) in repo::recent_beats(&state.db, id, kind, PER_KIND).await?
        {
            beats.push(json!({
                "kind": kind,
                "ts": ts,
                "ok": ok,
                "latency_ms": latency_ms,
                "error": error,
            }));
        }
    }
    Ok(Json(json!({ "beats": beats })))
}
