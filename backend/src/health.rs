//! Availability checks: the panel polls each host and records what it saw.
//!
//! The panel has to do this itself. Angie's active `health_check` is a PRO
//! feature, and the passive `max_fails`/`fail_timeout` we do have only reacts to
//! real traffic — a host nobody visits stays "up" forever, which is worse than
//! no answer because it looks like one.
//!
//! **What this can and cannot see.** Every probe leaves from this box, so it
//! reports "my Angie serves the site" and never "the internet reaches me". That
//! is not a shortcut, it is the ceiling: measured on the deploy target, a host's
//! domain resolves to a machine that is not this one, and probing the public URL
//! answered 200 from a stranger holding a stranger's certificate. A check that
//! confidently reports the wrong server is worse than none, so the HTTP probe
//! goes to loopback with the domain as SNI instead.
//!
//! Scheduling is stateless: a tick asks the database when each check last ran
//! and runs the ones that are due. Nothing to rebuild after a restart, and a
//! host edited mid-flight is simply picked up on the next tick.

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::net::TcpStream;

use crate::model::{HealthCheck, HealthCheckKind, ProxyHost};
use crate::state::AppState;
use crate::{repo, settings};

/// How often the scheduler looks for due checks. Not the check interval — that
/// is per-check and always a multiple of this in practice.
const TICK: Duration = Duration::from_secs(10);

/// Bound on probes in flight. The box is an SBC; a burst of checks must not
/// crowd out the thing being checked.
const MAX_CONCURRENT: usize = 8;

pub const DEFAULT_INTERVAL_SECS: u32 = 60;
pub const DEFAULT_TIMEOUT_SECS: u32 = 10;
pub const DEFAULT_RETENTION_DAYS: u32 = 30;

/// Defaults every check inherits unless it overrides them.
pub struct Defaults {
    pub interval_secs: u32,
    pub timeout_secs: u32,
    pub retention_days: u32,
}

pub fn spawn(state: Arc<AppState>) {
    tokio::spawn(async move {
        // Let startup settle; the first tick is not urgent.
        tokio::time::sleep(Duration::from_secs(15)).await;
        loop {
            if let Err(e) = tick(&state).await {
                tracing::debug!(error = %e, "health tick failed");
            }
            tokio::time::sleep(TICK).await;
        }
    });
}

async fn tick(state: &Arc<AppState>) -> anyhow::Result<()> {
    let defaults = defaults(state).await;
    let hosts = repo::list_hosts(&state.db).await?;
    let now = unix_now();

    let mut due: Vec<(ProxyHost, HealthCheck)> = Vec::new();
    for host in hosts {
        // A disabled host is not down — it is switched off. Probing it would
        // paint the bar red for a state the operator chose.
        if !host.enabled {
            continue;
        }
        for check in host.health_checks.iter().filter(|c| c.enabled) {
            let interval = i64::from(check.interval_secs.unwrap_or(defaults.interval_secs));
            let last = repo::last_beat_ts(&state.db, host.id, check.kind.as_str()).await?;
            if last.is_none_or(|t| now - t >= interval) {
                due.push((host.clone(), check.clone()));
            }
        }
    }

    // JoinSet rather than futures::join_all: tokio is already here, and one
    // more crate to await a vector is not worth the binary.
    for chunk in due.chunks(MAX_CONCURRENT) {
        let mut set = tokio::task::JoinSet::new();
        for (host, check) in chunk.iter().cloned() {
            let timeout = Duration::from_secs(u64::from(
                check.timeout_secs.unwrap_or(defaults.timeout_secs),
            ));
            set.spawn(async move {
                let beat = probe(&host, &check, timeout).await;
                (host.id, check.kind, beat)
            });
        }
        while let Some(joined) = set.join_next().await {
            let (host_id, kind, beat) = joined?;
            repo::insert_beat(&state.db, host_id, kind.as_str(), &beat).await?;
        }
    }

    repo::reap_beats(&state.db, now - i64::from(defaults.retention_days) * 86_400).await?;
    Ok(())
}

/// One probe's outcome.
pub struct Beat {
    pub ts: i64,
    pub ok: bool,
    pub latency_ms: Option<i64>,
    pub error: Option<String>,
}

async fn probe(host: &ProxyHost, check: &HealthCheck, timeout: Duration) -> Beat {
    let started = Instant::now();
    let result = match check.kind {
        HealthCheckKind::Tcp => probe_tcp(host, check, timeout).await,
        // Stage 2 — needs a TLS stack in the binary, which reqwest is currently
        // built without (default-features = false).
        HealthCheckKind::Http => Err(anyhow::anyhow!("http checks not implemented yet")),
    };
    let elapsed = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    match result {
        Ok(()) => Beat {
            ts: unix_now(),
            ok: true,
            latency_ms: Some(elapsed),
            error: None,
        },
        Err(e) => Beat {
            ts: unix_now(),
            ok: false,
            // No latency on failure: the number would time the error, not the
            // service, and a timeout would plot as the slowest healthy request
            // the host ever served.
            latency_ms: None,
            error: Some(truncate(&e.to_string(), 200)),
        },
    }
}

/// Open a socket to the host's backend. Answers "is my upstream listening",
/// nothing more — no bytes are exchanged.
async fn probe_tcp(host: &ProxyHost, check: &HealthCheck, timeout: Duration) -> anyhow::Result<()> {
    let port = check.port.unwrap_or(host.forward_port);
    let addr = format!("{}:{}", host.forward_host, port);
    let stream = tokio::time::timeout(timeout, TcpStream::connect(&addr))
        .await
        .map_err(|_| anyhow::anyhow!("timed out after {}s", timeout.as_secs()))??;
    drop(stream);
    Ok(())
}

async fn defaults(state: &AppState) -> Defaults {
    let all = repo::all_settings(&state.db).await.unwrap_or_default();
    let get = |key: &str, fallback: u32| -> u32 {
        all.get(key)
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(fallback)
    };
    Defaults {
        interval_secs: get(settings::KEY_HEALTH_INTERVAL, DEFAULT_INTERVAL_SECS),
        timeout_secs: get(settings::KEY_HEALTH_TIMEOUT, DEFAULT_TIMEOUT_SECS),
        retention_days: get(settings::KEY_HEALTH_RETENTION_DAYS, DEFAULT_RETENTION_DAYS),
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Keep an error readable in a table cell, and keep a pathological upstream from
/// writing a novel into every row.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect::<String>() + "…"
}
