//! TCP/UDP port forwarding (Angie `stream {}` context) — the last host type
//! for full nginx-proxy-manager parity (v2). A stream is a plain L4 forward,
//! or (TCP only) terminates TLS with a panel-managed certificate — Angie
//! decrypts on the incoming port via `$acme_cert_<name>` (issued by the same
//! http-context ACME collector the proxy hosts use) and forwards plaintext.
//!
//! Unlike HTTP hosts, streams are keyed by their **incoming port**, not a
//! domain — two enabled streams cannot listen on the same port for the same
//! protocol, or Angie would refuse to bind. We reject the conflict up front
//! with a clear message rather than letting `angie -t` fail at apply time.
//!
//! Activating the stream context in the live `angie.conf` is a separate,
//! explicit one-time step (see [`enable_context`]) — Angie ships that context
//! commented out, and apply refuses to proceed until it is on.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::db::now_epoch;
use crate::error::{ApiError, ApiResult};
use crate::model::{self, StreamInput, StreamTls, UpstreamPolicy};
use crate::repo;
use crate::state::AppState;
use crate::systemd;

fn upstream_policy(state: &AppState) -> UpstreamPolicy {
    UpstreamPolicy {
        allow_loopback: state.cfg.allow_loopback_upstreams,
    }
}

/// A TLS-terminating stream must point at a certificate that actually exists,
/// or the generator would emit a dangling `$acme_cert_<name>` reference.
async fn check_cert_exists(state: &AppState, input: &StreamInput) -> ApiResult<()> {
    if input.tls != StreamTls::Terminate {
        return Ok(());
    }
    if let Some(cid) = input.certificate_id {
        if repo::get_cert(&state.db, cid).await?.is_none() {
            return Err(ApiError::not_found(format!("no certificate #{cid}")));
        }
    }
    Ok(())
}

/// Reject an incoming port already taken by another enabled stream on an
/// overlapping protocol. `exclude` skips the stream being updated (0 = create).
async fn check_port_free(state: &AppState, input: &StreamInput, exclude: i64) -> ApiResult<()> {
    if !input.enabled {
        return Ok(());
    }
    for s in repo::list_streams(&state.db).await? {
        if s.id == exclude || !s.enabled || s.incoming_port != input.incoming_port {
            continue;
        }
        let tcp_clash = s.tcp && input.tcp;
        let udp_clash = s.udp && input.udp;
        if tcp_clash || udp_clash {
            let proto = if tcp_clash && udp_clash {
                "TCP/UDP"
            } else if tcp_clash {
                "TCP"
            } else {
                "UDP"
            };
            return Err(ApiError::new(
                axum::http::StatusCode::CONFLICT,
                "port_conflict",
                format!(
                    "port {} ({proto}) is already forwarded by stream #{}",
                    input.incoming_port, s.id
                ),
            ));
        }
    }
    Ok(())
}

pub async fn list(_u: AuthUser, State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let streams = repo::list_streams(&state.db).await?;
    let arr: Vec<Value> = streams
        .iter()
        .map(|s| serde_json::to_value(s).unwrap_or(Value::Null))
        .collect();
    Ok(Json(json!({ "streams": arr })))
}

pub async fn get_one(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let s = repo::get_stream(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no stream #{id}")))?;
    Ok(Json(serde_json::to_value(&s).unwrap_or(Value::Null)))
}

pub async fn create(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(raw): Json<StreamInput>,
) -> ApiResult<Json<Value>> {
    let input = model::validate_stream_input(raw, &upstream_policy(&state))?;
    check_cert_exists(&state, &input).await?;
    check_port_free(&state, &input, 0).await?;
    let id = repo::insert_stream(&state.db, &input).await?;
    let s = repo::get_stream(&state.db, id)
        .await?
        .expect("just inserted");
    Ok(Json(serde_json::to_value(&s).unwrap_or(Value::Null)))
}

pub async fn update(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(raw): Json<StreamInput>,
) -> ApiResult<Json<Value>> {
    let input = model::validate_stream_input(raw, &upstream_policy(&state))?;
    check_cert_exists(&state, &input).await?;
    check_port_free(&state, &input, id).await?;
    if !repo::update_stream(&state.db, id, &input).await? {
        return Err(ApiError::not_found(format!("no stream #{id}")));
    }
    let s = repo::get_stream(&state.db, id)
        .await?
        .expect("just updated");
    Ok(Json(serde_json::to_value(&s).unwrap_or(Value::Null)))
}

pub async fn delete(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    if !repo::delete_stream(&state.db, id).await? {
        return Err(ApiError::not_found(format!("no stream #{id}")));
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
    // Re-enabling can resurrect a port conflict — check against the intended
    // (enabled) shape before flipping the flag.
    if enabled {
        if let Some(s) = repo::get_stream(&state.db, id).await? {
            let intended = StreamInput {
                incoming_port: s.incoming_port,
                forward_host: s.forward_host,
                forward_port: s.forward_port,
                tcp: s.tcp,
                udp: s.udp,
                tls: s.tls,
                certificate_id: s.certificate_id,
                enabled: true,
            };
            check_port_free(&state, &intended, id).await?;
        }
    }
    if !repo::set_stream_enabled(&state.db, id, enabled).await? {
        return Err(ApiError::not_found(format!("no stream #{id}")));
    }
    Ok(Json(json!({ "ok": true, "enabled": enabled })))
}

// ------------------------------------------------------ enable stream context

pub const ENABLE_STREAMS_UNIT: &str = "angie-panel-enable-streams.service";
pub const ENABLE_REPORT_FILE: &str = "enable-streams-result.json";

/// Whether Angie's stream context is active (loads our stream.d). Surfaced in
/// the dashboard so the UI can prompt the one-time enable before first apply.
pub fn context_active(state: &AppState) -> bool {
    crate::apply::pipeline::stream_context_active(&state.cfg.angie.angie_conf)
}

/// One-time privileged action: edit the live `angie.conf` to activate the
/// `stream {}` context so Angie loads `/etc/angie/stream.d/*.conf`. Runs the
/// root helper via its dedicated oneshot unit (polkit-gated), with a dev
/// fallback that spawns the helper directly. The helper validates with
/// `angie -t` and rolls back its edit on failure, so a bad result is reported,
/// never left live.
pub async fn enable_context(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<Value>> {
    if context_active(&state) {
        return Ok(Json(json!({ "ok": true, "already_active": true })));
    }
    let started = now_epoch();

    match systemd::start_unit(ENABLE_STREAMS_UNIT).await {
        Ok(()) => {
            *state.polkit_ok.lock().unwrap() = Some(true);
            let report = wait_for_enable_report(&state, started).await?;
            return finish_enable(report);
        }
        Err(systemd::SystemdError::Denied(detail)) => {
            *state.polkit_ok.lock().unwrap() = Some(false);
            return Err(ApiError::forbidden(
                "polkit_denied",
                format!(
                    "polkit refused to start {ENABLE_STREAMS_UNIT}: {detail}. \
                     Is 10-angie-panel.rules installed?"
                ),
            ));
        }
        Err(systemd::SystemdError::Unavailable(detail)) => {
            tracing::debug!(%detail, "systemd unavailable, running enable-streams helper directly");
        }
    }

    // Dev fallback: spawn ourselves as the helper.
    let exe = std::env::current_exe().map_err(ApiError::internal)?;
    let out = tokio::process::Command::new(exe)
        .args(["helper", "enable-streams", "--config"])
        .arg(&state.cfg_path)
        .output()
        .await
        .map_err(ApiError::internal)?;
    if !out.status.success() {
        return Err(ApiError::internal(format!(
            "enable-streams helper failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let report = read_enable_report(&state)
        .filter(|r| r.timestamp >= started)
        .ok_or_else(|| ApiError::internal("helper finished but produced no report"))?;
    finish_enable(report)
}

fn finish_enable(report: EnableReport) -> ApiResult<Json<Value>> {
    if report.ok {
        Ok(Json(json!({ "ok": true, "message": report.message })))
    } else {
        Err(ApiError::internal(format!(
            "could not enable the stream context: {}",
            report.message
        )))
    }
}

/// Result the enable-streams helper writes for the panel (root's stdout isn't
/// readable across the privilege boundary, so it round-trips via a file).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EnableReport {
    pub timestamp: i64,
    pub ok: bool,
    pub message: String,
}

pub fn read_enable_report(state: &AppState) -> Option<EnableReport> {
    let raw = std::fs::read_to_string(state.cfg.data_dir.join(ENABLE_REPORT_FILE)).ok()?;
    serde_json::from_str(&raw).ok()
}

async fn wait_for_enable_report(state: &AppState, started: i64) -> ApiResult<EnableReport> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(r) = read_enable_report(state) {
            if r.timestamp >= started {
                return Ok(r);
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(ApiError::internal(
                "timed out waiting for the enable-streams report \
                 (check `journalctl -u angie-panel-enable-streams`)",
            ));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
