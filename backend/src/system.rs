use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::auth::AuthUser;
use crate::db::now_epoch;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use crate::systemd;

pub const CONFIGTEST_UNIT: &str = "angie-panel-configtest.service";
pub const REPORT_FILE: &str = "configtest-report.json";

// ------------------------------------------------------------------ report

/// Written by the root helper, read by the panel. The schema is shared with
/// the frontend — see the API contract in frontend/src/lib/api.ts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigtestReport {
    pub timestamp: i64,
    pub ok: bool,
    pub exit_code: Option<i32>,
    pub output: String,
    pub ran_via: String,
}

pub fn read_report(data_dir: &Path) -> Option<ConfigtestReport> {
    let raw = std::fs::read_to_string(data_dir.join(REPORT_FILE)).ok()?;
    serde_json::from_str(&raw).ok()
}

// ------------------------------------------------------------------ status

#[derive(Serialize)]
pub struct SystemStatus {
    panel: PanelStatus,
    angie: AngieStatus,
    dbus: DbusStatus,
    status_api: StatusApi,
}

#[derive(Serialize)]
struct PanelStatus {
    version: &'static str,
    data_dir_writable: bool,
}

#[derive(Serialize)]
struct AngieStatus {
    installed: bool,
    version: Option<String>,
    acme_module: Option<bool>,
    unit_active: Option<bool>,
}

#[derive(Serialize)]
struct DbusStatus {
    available: bool,
    polkit_ok: Option<bool>,
}

#[derive(Serialize)]
struct StatusApi {
    reachable: bool,
    generation: Option<u64>,
}

async fn probe_angie(state: &AppState) -> AngieStatus {
    let out = Command::new(&state.cfg.angie.bin).arg("-V").output().await;
    match out {
        Ok(out) => {
            // Angie prints version and configure arguments to stderr,
            // e.g. "Angie version: Angie/1.11.8" + "configure arguments: ...".
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stderr),
                String::from_utf8_lossy(&out.stdout)
            );
            let version = text
                .lines()
                .find_map(|l| l.split("Angie/").nth(1))
                .map(|v| v.split_whitespace().next().unwrap_or(v).to_string());
            let acme = text.contains("http_acme");
            AngieStatus {
                installed: true,
                version,
                acme_module: Some(acme),
                unit_active: systemd::unit_active("angie.service").await,
            }
        }
        Err(_) => AngieStatus {
            installed: false,
            version: None,
            acme_module: None,
            unit_active: None,
        },
    }
}

async fn probe_status_api(state: &AppState) -> StatusApi {
    let url = format!(
        "{}/angie",
        state.cfg.angie.status_api_url.trim_end_matches('/')
    );
    match state.http_client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let generation = resp
                .json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|v| v.get("generation").and_then(|g| g.as_u64()));
            StatusApi {
                reachable: true,
                generation,
            }
        }
        _ => StatusApi {
            reachable: false,
            generation: None,
        },
    }
}

fn data_dir_writable(dir: &Path) -> bool {
    let probe = dir.join(".write-probe");
    let ok = std::fs::write(&probe, b"x").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

pub async fn get_status(
    _user: AuthUser,
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<SystemStatus>> {
    let (angie, status_api, dbus_available) = tokio::join!(
        probe_angie(&state),
        probe_status_api(&state),
        systemd::dbus_available(),
    );
    Ok(Json(SystemStatus {
        panel: PanelStatus {
            version: env!("CARGO_PKG_VERSION"),
            data_dir_writable: data_dir_writable(&state.cfg.data_dir),
        },
        angie,
        dbus: DbusStatus {
            available: dbus_available,
            polkit_ok: *state.polkit_ok.lock().unwrap(),
        },
        status_api,
    }))
}

// -------------------------------------------------------------- configtest

pub async fn last_configtest(
    _user: AuthUser,
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<ConfigtestReport>> {
    read_report(&state.cfg.data_dir)
        .map(Json)
        .ok_or_else(|| ApiError::not_found("config validation has not run yet"))
}

/// Trigger a config validation through the root helper. Primary path: start
/// the oneshot systemd unit (polkit-gated). Fallback for dev environments:
/// run the helper directly as the panel user.
pub async fn run_configtest(
    _user: AuthUser,
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<ConfigtestReport>> {
    let started = now_epoch();

    match systemd::start_unit(CONFIGTEST_UNIT).await {
        Ok(()) => {
            *state.polkit_ok.lock().unwrap() = Some(true);
            let report = wait_for_report(&state, started).await?;
            return Ok(Json(report));
        }
        Err(crate::systemd::SystemdError::Denied(detail)) => {
            *state.polkit_ok.lock().unwrap() = Some(false);
            return Err(ApiError::forbidden(
                "polkit_denied",
                format!(
                    "polkit refused to start {CONFIGTEST_UNIT}: {detail}. \
                     Is 10-angie-panel.rules installed?"
                ),
            ));
        }
        Err(crate::systemd::SystemdError::Unavailable(detail)) => {
            tracing::debug!(%detail, "systemd unavailable, falling back to direct helper run");
        }
    }

    // Dev fallback: spawn ourselves as the helper. On a real install this
    // will typically produce a failing report (no permissions), which is
    // still honest and visible in the UI.
    let exe = std::env::current_exe().map_err(ApiError::internal)?;
    let out = Command::new(exe)
        .args(["helper", "configtest", "--config"])
        .arg(&state.cfg_path)
        .env("ANGIE_PANEL_RAN_VIA", "direct")
        .output()
        .await
        .map_err(ApiError::internal)?;
    if !out.status.success() {
        return Err(ApiError::internal(format!(
            "helper failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let report = read_report(&state.cfg.data_dir)
        .filter(|r| r.timestamp >= started)
        .ok_or_else(|| ApiError::internal("helper finished but produced no report"))?;
    Ok(Json(report))
}

async fn wait_for_report(state: &AppState, started: i64) -> ApiResult<ConfigtestReport> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(r) = read_report(&state.cfg.data_dir) {
            if r.timestamp >= started {
                return Ok(r);
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(ApiError::internal(
                "timed out waiting for the configtest report (check `journalctl -u angie-panel-configtest`)",
            ));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
