//! Apply/preview/settings HTTP handlers. The panel builds the fileset from the
//! DB, previews the diff, and (on apply) stages it and triggers the root helper
//! unit over D-Bus — mirroring `system::run_configtest`. The helper does the
//! privileged transaction; the panel reads back `apply-result.json`.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};
use tokio::process::Command;

use crate::auth::AuthUser;
use crate::db::now_epoch;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use crate::{apply, repo, settings, systemd};

pub const APPLY_UNIT: &str = "angie-panel-apply.service";

// ------------------------------------------------------------------ preview

pub async fn preview(_u: AuthUser, State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let fileset = settings::build_fileset(&state).await?;
    let diff = apply::preview(&state.cfg, &fileset).map_err(ApiError::internal)?;
    let revision = repo::hosts_revision(&state.db).await?;
    Ok(Json(json!({
        "db_revision": revision,
        "diff": serde_json::to_value(&diff).unwrap_or(Value::Null),
    })))
}

// -------------------------------------------------------------------- apply

pub async fn apply_now(_u: AuthUser, State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let report = perform_apply(&state, ApplyTrigger::Manual).await?;
    Ok(Json(serde_json::to_value(&report).unwrap_or(Value::Null)))
}

/// Who initiated an apply — recorded in history so the UI can distinguish a
/// user click from the reconciler auto-activating HTTPS after issuance.
#[derive(Clone, Copy)]
pub enum ApplyTrigger {
    Manual,
    /// Auto-apply after a certificate became ready (M3 reconciler).
    AutoCertReady,
}

/// The full apply path shared by the HTTP handler and the background
/// reconciler: acquire the lock, stage, run the helper, record history, and
/// (on success) remember the DB revision that is now live.
pub async fn perform_apply(
    state: &Arc<AppState>,
    trigger: ApplyTrigger,
) -> ApiResult<apply::ApplyReport> {
    // Serialize applies (PLAN.md §2.2): a second concurrent apply is rejected.
    let _guard = state.apply_lock.try_lock().map_err(|_| {
        ApiError::new(
            axum::http::StatusCode::CONFLICT,
            "apply_in_progress",
            "an apply is already running",
        )
    })?;

    let revision = repo::hosts_revision(&state.db).await?;
    let fileset = settings::build_fileset(state).await?;

    apply::write_staging(&fileset, &state.cfg.data_dir, &state.cfg.angie)
        .map_err(ApiError::internal)?;

    let started = now_epoch();
    let report = trigger_helper(state, started).await?;

    let result_label = match trigger {
        ApplyTrigger::Manual => format!("{:?}", report.result),
        ApplyTrigger::AutoCertReady => format!("{:?} (auto: cert ready)", report.result),
    };
    let report_json = serde_json::to_string(&report).unwrap_or_default();
    repo::record_apply(&state.db, revision, &result_label, &report_json).await?;

    // Remember the revision that is now live so the reconciler can tell
    // "cert became ready" (external) from "user has pending edits" (internal).
    if report.result.is_ok() {
        let _ = repo::set_setting(
            &state.db,
            settings::KEY_LAST_APPLIED_REVISION,
            &revision.to_string(),
        )
        .await;
    }

    Ok(report)
}

/// Start the apply unit via D-Bus (polkit-gated); fall back to a direct helper
/// spawn in dev. Returns the apply report the helper wrote.
async fn trigger_helper(state: &Arc<AppState>, started: i64) -> ApiResult<apply::ApplyReport> {
    match systemd::start_unit(APPLY_UNIT).await {
        Ok(()) => {
            *state.polkit_ok.lock().unwrap() = Some(true);
            wait_for_apply_report(state, started).await
        }
        Err(systemd::SystemdError::Denied(detail)) => {
            *state.polkit_ok.lock().unwrap() = Some(false);
            Err(ApiError::forbidden(
                "polkit_denied",
                format!("polkit refused to start {APPLY_UNIT}: {detail}"),
            ))
        }
        Err(systemd::SystemdError::Unavailable(detail)) => {
            tracing::debug!(%detail, "systemd unavailable, running apply helper directly");
            let exe = std::env::current_exe().map_err(ApiError::internal)?;
            let out = Command::new(exe)
                .args(["helper", "apply", "--config"])
                .arg(&state.cfg_path)
                .env("ANGIE_PANEL_RAN_VIA", "direct")
                .output()
                .await
                .map_err(ApiError::internal)?;
            if !out.status.success() {
                return Err(ApiError::internal(format!(
                    "apply helper failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                )));
            }
            apply::read_report(&state.cfg.data_dir)
                .filter(|r| r.timestamp >= started)
                .ok_or_else(|| ApiError::internal("helper produced no apply report"))
        }
    }
}

async fn wait_for_apply_report(
    state: &Arc<AppState>,
    started: i64,
) -> ApiResult<apply::ApplyReport> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(40);
    loop {
        if let Some(r) = apply::read_report(&state.cfg.data_dir) {
            if r.timestamp >= started {
                return Ok(r);
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(ApiError::internal(
                "timed out waiting for the apply report (check `journalctl -u angie-panel-apply`)",
            ));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

// ------------------------------------------------------------------ history

pub async fn history(_u: AuthUser, State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let entries = repo::list_apply_history(&state.db, 50).await?;
    let arr: Vec<Value> = entries
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "timestamp": e.timestamp,
                "result": e.result,
                "report": serde_json::from_str::<Value>(&e.report).unwrap_or(Value::Null),
            })
        })
        .collect();
    Ok(Json(json!({ "history": arr })))
}

// ----------------------------------------------------------------- settings

/// Is this settings key a secret — never returned by the GET, never exported?
/// The ACME hook token and every DNS provider credential (`dns_cred:*`).
pub fn is_secret_setting(key: &str) -> bool {
    key == settings::KEY_ACME_HOOK_TOKEN || crate::dns_providers::is_cred_key(key)
}

pub async fn get_settings(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<Value>> {
    let mut map = repo::all_settings(&state.db).await?;
    // Redact secrets (hook token + DNS provider creds). "Configured" state for
    // providers is surfaced separately via GET /api/dns-providers.
    map.retain(|k, _| !is_secret_setting(k));
    let eff = settings::effective_settings(&state).await?;
    Ok(Json(json!({
        "raw": map,
        "effective": {
            "default_site": format!("{:?}", eff.default_site),
            "ipv6_enabled": eff.ipv6_enabled,
            "resolvers": eff.resolvers,
        }
    })))
}

#[derive(serde::Deserialize)]
pub struct SettingsUpdate {
    #[serde(flatten)]
    values: std::collections::HashMap<String, String>,
}

const ALLOWED_SETTING_KEYS: &[&str] = &[
    settings::KEY_DEFAULT_SITE,
    settings::KEY_DEFAULT_SITE_REDIRECT,
    settings::KEY_IPV6_ENABLED,
    settings::KEY_RESOLVER_OVERRIDE,
    settings::KEY_ACME_EMAIL,
];

pub async fn put_settings(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(update): Json<SettingsUpdate>,
) -> ApiResult<Json<Value>> {
    for (k, v) in &update.values {
        if !ALLOWED_SETTING_KEYS.contains(&k.as_str()) {
            return Err(ApiError::bad_request(
                "unknown_setting",
                format!("unknown setting key: {k}"),
            ));
        }
        // Validate a redirect URL so it cannot break the generated directive.
        if k == settings::KEY_DEFAULT_SITE_REDIRECT && !v.is_empty() {
            validate_redirect_url(v)?;
        }
        repo::set_setting(&state.db, k, v).await?;
    }
    get_settings(_u, State(state)).await
}

fn validate_redirect_url(url: &str) -> ApiResult<()> {
    let ok = (url.starts_with("http://") || url.starts_with("https://"))
        && url.len() <= 2000
        && !url.contains(|c: char| c.is_whitespace() || matches!(c, ';' | '{' | '}' | '"' | '\''));
    if !ok {
        return Err(ApiError::bad_request(
            "invalid_redirect_url",
            "redirect URL must be http(s):// and contain no spaces/quotes/braces",
        ));
    }
    Ok(())
}
