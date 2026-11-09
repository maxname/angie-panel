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

use std::net::SocketAddr;
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
    // Which hosts Angie is actually serving over TLS right now. Asked only when
    // an HTTP probe is due, because it costs a call to Angie's status API — and
    // asked at all because probing :443 on a host whose certificate has not
    // issued yet would report a site that is up on :80 as down. The readiness
    // comes from the same place the generator reads it, so the probe always
    // aims at the port the config actually opened.
    let https_hosts = if due.iter().any(|(_, c)| c.kind == HealthCheckKind::Http) {
        https_host_ids(state).await
    } else {
        std::collections::HashSet::new()
    };

    for chunk in due.chunks(MAX_CONCURRENT) {
        let mut set = tokio::task::JoinSet::new();
        for (host, check) in chunk.iter().cloned() {
            let timeout = Duration::from_secs(u64::from(
                check.timeout_secs.unwrap_or(defaults.timeout_secs),
            ));
            let https = https_hosts.contains(&host.id);
            set.spawn(async move {
                let beat = probe(&host, &check, timeout, https).await;
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

async fn probe(host: &ProxyHost, check: &HealthCheck, timeout: Duration, https: bool) -> Beat {
    let started = Instant::now();
    let result = match check.kind {
        HealthCheckKind::Tcp => probe_tcp(host, check, timeout).await,
        HealthCheckKind::Http => probe_http(host, check, timeout, https, angie_port(https)).await,
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

/// Which port Angie listens on for this host. Passed in rather than assumed so
/// the probe can be driven against an ephemeral port in tests — binding 443
/// would need root, and the one assumption worth pinning here is not the port
/// number.
fn angie_port(https: bool) -> u16 {
    if https {
        443
    } else {
        80
    }
}

/// Ask Angie for the site, over loopback, with the host's domain as SNI.
///
/// The URL carries the domain so the certificate is checked against the name it
/// was issued for, but resolution is pinned to 127.0.0.1 — the public address is
/// deliberately not used. On the deploy target the domain resolves to a machine
/// that is not this one, and probing it returned 200 with a stranger's
/// certificate: a green bar for a server we do not run.
///
/// Certificates are verified unless the check opts out. That verification is the
/// cheapest half of the value here — it catches an expired or mis-issued
/// certificate, which is exactly what nobody notices until a browser does.
async fn probe_http(
    host: &ProxyHost,
    check: &HealthCheck,
    timeout: Duration,
    https: bool,
    port: u16,
) -> anyhow::Result<()> {
    let domain = host
        .domains
        .first()
        .ok_or_else(|| anyhow::anyhow!("host has no domain to ask for"))?;

    // resolve() takes the address and ignores the port in it — reqwest's own
    // words: "any port in the overridden addr will be ignored and traffic sent
    // to the conventional port for the given scheme". So the port has to travel
    // in the URL, and the address here is only ever the loopback.
    let pin = SocketAddr::from(([127, 0, 0, 1], 0));
    let client = reqwest::Client::builder()
        .resolve(domain, pin)
        .timeout(timeout)
        .danger_accept_invalid_certs(check.insecure)
        .build()?;

    let scheme = if https { "https" } else { "http" };
    let path = if check.path.is_empty() {
        "/"
    } else {
        &check.path
    };
    // Port stated explicitly, including the conventional one: Angie matches
    // server_name on the host without it, and it keeps this testable.
    let res = client
        .get(format!("{scheme}://{domain}:{port}{path}"))
        .send()
        .await?;

    let status = res.status();
    let accepted = if check.expected_status.is_empty() {
        status.is_success()
    } else {
        check.expected_status.contains(&status.as_u16())
    };
    if !accepted {
        anyhow::bail!("status {}", status.as_u16());
    }

    if let Some(kw) = check.keyword.as_deref().filter(|k| !k.is_empty()) {
        let body = res.text().await?;
        match (body.contains(kw), check.keyword_absent) {
            (true, true) => anyhow::bail!("keyword {kw:?} present"),
            (false, false) => anyhow::bail!("keyword {kw:?} missing"),
            _ => {}
        }
    }
    Ok(())
}

/// Hosts Angie is serving on :443 right now — those with a certificate that has
/// actually issued. Same source the generator gates the :443 server on, so the
/// probe and the config can never disagree about which port exists.
async fn https_host_ids(state: &AppState) -> std::collections::HashSet<i64> {
    let ready = crate::settings::acme_ready_map(state).await;
    let certs = repo::list_certs(&state.db).await.unwrap_or_default();
    let ready_ids: std::collections::HashSet<i64> = certs
        .iter()
        .filter(|c| ready.get(&c.name).copied().unwrap_or(false))
        .map(|c| c.id)
        .collect();
    repo::list_hosts(&state.db)
        .await
        .unwrap_or_default()
        .iter()
        .filter(|h| h.certificate_id.is_some_and(|id| ready_ids.contains(&id)))
        .map(|h| h.id)
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ErrorPages, ForwardAuth, Gzip, HealthCheckKind, Maintenance, Mtls, ProxyTuning, RateLimit,
        Scheme, Upstream,
    };

    fn host(port: u16) -> ProxyHost {
        ProxyHost {
            id: 1,
            domains: vec!["probe.example.com".into()],
            forward_scheme: Scheme::Http,
            forward_host: "127.0.0.1".into(),
            forward_port: port,
            websockets_upgrade: false,
            block_exploits: false,
            cache_assets: false,
            http2: false,
            http3: false,
            force_ssl: false,
            hsts: false,
            hsts_subdomains: false,
            trust_forwarded_proto: false,
            certificate_id: None,
            access_list_id: None,
            locations: vec![],
            advanced_snippet: None,
            rate_limit: RateLimit::default(),
            health_checks: vec![],
            upstream: Upstream::default(),
            mtls: Mtls::default(),
            forward_auth: ForwardAuth::default(),
            custom_headers: vec![],
            maintenance: Maintenance::default(),
            gzip: Gzip::default(),
            error_pages: ErrorPages::default(),
            proxy_tuning: ProxyTuning::default(),
            enabled: true,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn check(kind: HealthCheckKind) -> HealthCheck {
        HealthCheck {
            kind,
            enabled: true,
            interval_secs: None,
            timeout_secs: None,
            path: String::new(),
            expected_status: vec![],
            keyword: None,
            keyword_absent: false,
            insecure: false,
            port: None,
        }
    }

    #[tokio::test]
    async fn tcp_probe_sees_a_listener_and_its_absence() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let beat = probe(
            &host(port),
            &check(HealthCheckKind::Tcp),
            Duration::from_secs(2),
            false,
        )
        .await;
        assert!(beat.ok, "a bound port must read as up: {:?}", beat.error);
        assert!(beat.latency_ms.is_some());

        // Same host, port now closed.
        drop(listener);
        let beat = probe(
            &host(port),
            &check(HealthCheckKind::Tcp),
            Duration::from_secs(2),
            false,
        )
        .await;
        assert!(!beat.ok, "a closed port must read as down");
        // Failures carry no latency — timing an error is not timing the service.
        assert_eq!(beat.latency_ms, None);
        assert!(beat.error.is_some());
    }

    /// The whole HTTP probe rests on reqwest's resolve() overriding the address
    /// while the URL keeps the domain — that is what gets the right SNI and a
    /// certificate checked against the right name. If resolve() also pinned the
    /// port, every probe would hit the wrong one. Nail it against a real server.
    #[tokio::test]
    async fn http_probe_pins_the_address_but_keeps_the_domain() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = std::sync::Arc::new(tokio::sync::Mutex::new(String::new()));
        let seen2 = seen.clone();

        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let n = tokio::io::AsyncReadExt::read(&mut sock, &mut buf)
                .await
                .unwrap();
            *seen2.lock().await = String::from_utf8_lossy(&buf[..n]).to_string();
            tokio::io::AsyncWriteExt::write_all(
                &mut sock,
                b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello",
            )
            .await
            .unwrap();
        });

        // A domain that resolves nowhere real: if the probe used DNS rather than
        // the pin, this could not connect at all.
        let mut h = host(0);
        h.domains = vec!["probe.invalid".into()];
        let err = probe_http(
            &h,
            &check(HealthCheckKind::Http),
            Duration::from_secs(5),
            false,
            port,
        )
        .await;

        assert!(
            err.is_ok(),
            "probe should have reached the local listener: {err:?}"
        );
        // Lowercased: hyper writes header names as HeaderMap stores them, which
        // is lowercase on the wire.
        let req = seen.lock().await.to_lowercase();
        assert!(
            req.contains(&format!("host: probe.invalid:{port}")),
            "the request must carry the domain, not the IP — that is what gets \
             the right SNI and a certificate checked against the right name:\n{req}"
        );
    }
}
