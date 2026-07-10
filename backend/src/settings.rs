//! Effective-settings resolution and FileSet assembly. Bridges the DB/config
//! to the generator: reads the settings table, discovers resolvers from
//! /etc/resolv.conf (override-able), and builds the full GeneratorInput.

use std::sync::Arc;

use crate::error::{ApiError, ApiResult};
use crate::generator::{self, DefaultSite, EffectiveSettings, FileSet, GeneratorInput};
use crate::repo;
use crate::state::AppState;

// Settings keys (settings table).
pub const KEY_DEFAULT_SITE: &str = "default_site"; // notfound|drop444|redirect|html
pub const KEY_DEFAULT_SITE_REDIRECT: &str = "default_site_redirect_url";
pub const KEY_IPV6_ENABLED: &str = "ipv6_enabled"; // "1"/"0"
pub const KEY_RESOLVER_OVERRIDE: &str = "resolver_override"; // space/comma list
pub const KEY_ACME_EMAIL: &str = "acme_email";
/// hosts_revision that is currently live (set after each successful apply).
/// Lets the reconciler distinguish external cert-readiness changes from
/// pending user edits. Not user-editable.
pub const KEY_LAST_APPLIED_REVISION: &str = "last_applied_revision";

/// Parse nameserver lines out of resolv.conf. systemd-resolved's stub
/// (127.0.0.53) is a valid resolver and kept as-is.
fn resolvers_from_resolv_conf() -> Vec<String> {
    let text = std::fs::read_to_string("/etc/resolv.conf").unwrap_or_default();
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(ns) = line.strip_prefix("nameserver ") {
            let ns = ns.trim();
            if !ns.is_empty() && !out.contains(&ns.to_string()) {
                out.push(ns.to_string());
            }
        }
    }
    out
}

/// Detect a global (non-loopback, non-link-local) IPv6 address on any
/// interface — used as the default for ipv6_enabled at first run.
#[cfg(target_os = "linux")]
fn host_has_global_ipv6() -> bool {
    // Best-effort: parse `ip -6 addr` is heavy; instead check for a global
    // scope address via the /proc interface list. Fall back to false.
    std::fs::read_to_string("/proc/net/if_inet6")
        .map(|s| {
            s.lines().any(|l| {
                // fields: address ifindex prefixlen scope flags name
                // scope 0x00 == global
                l.split_whitespace().nth(3) == Some("00")
            })
        })
        .unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
fn host_has_global_ipv6() -> bool {
    false
}

pub async fn effective_settings(state: &AppState) -> ApiResult<EffectiveSettings> {
    let map = repo::all_settings(&state.db).await?;

    let default_site = match map.get(KEY_DEFAULT_SITE).map(String::as_str) {
        Some("drop444") => DefaultSite::Drop444,
        Some("html") => DefaultSite::Html,
        Some("redirect") => {
            let url = map
                .get(KEY_DEFAULT_SITE_REDIRECT)
                .cloned()
                .unwrap_or_default();
            DefaultSite::Redirect(url)
        }
        _ => DefaultSite::NotFound,
    };

    let ipv6_enabled = match map.get(KEY_IPV6_ENABLED).map(String::as_str) {
        Some("1") => true,
        Some("0") => false,
        _ => host_has_global_ipv6(),
    };

    let resolvers = match map.get(KEY_RESOLVER_OVERRIDE) {
        Some(o) if !o.trim().is_empty() => o
            .split([' ', ','])
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect(),
        _ => resolvers_from_resolv_conf(),
    };

    Ok(EffectiveSettings {
        default_site,
        ipv6_enabled,
        resolvers,
    })
}

/// Parse the loopback port the status API listens on, from status_api_url.
fn status_port(state: &AppState) -> u16 {
    state
        .cfg
        .angie
        .status_api_url
        .rsplit(':')
        .next()
        .and_then(|tail| tail.split('/').next())
        .and_then(|p| p.parse().ok())
        .unwrap_or(8100)
}

/// Query `/status/http/acme_clients/` and return name → issued?. On any error
/// (status API down, off-device) every cert is treated as not-yet-issued, so
/// hosts stay HTTP-only — the safe default that never serves broken TLS.
pub async fn acme_ready_map(state: &AppState) -> std::collections::HashMap<String, bool> {
    let url = format!(
        "{}/http/acme_clients/",
        state.cfg.angie.status_api_url.trim_end_matches('/')
    );
    let mut map = std::collections::HashMap::new();
    if let Ok(resp) = state.http_client.get(&url).send().await {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            if let Some(obj) = json.as_object() {
                for (name, v) in obj {
                    // Ready when Angie reports the certificate as valid.
                    let ready = v.get("certificate").and_then(|c| c.as_str()) == Some("valid");
                    map.insert(name.clone(), ready);
                }
            }
        }
    }
    map
}

/// Assemble the generator input from the current DB state.
pub async fn build_generator_input(state: &AppState) -> ApiResult<GeneratorInput> {
    let hosts = repo::list_hosts(&state.db).await?;
    let settings = effective_settings(state).await?;
    let db_certs = repo::list_certs(&state.db).await?;
    let ready = acme_ready_map(state).await;

    let certificates = db_certs
        .into_iter()
        .map(|c| generator::Certificate {
            id: c.id,
            ready: ready.get(&c.name).copied().unwrap_or(false),
            name: c.name,
            domains: c.domains,
            challenge: c.challenge.as_str().to_string(),
            key_type: c.key_type.as_str().to_string(),
            email: c.email,
            staging: c.staging,
            // Pause/enabled toggle is a follow-up UI action; default on.
            enabled: true,
        })
        .collect();

    Ok(GeneratorInput {
        hosts,
        settings,
        snippets_dir: state.cfg.angie.snippets_dir.clone(),
        status_port: status_port(state),
        public_dir: state.cfg.public_dir(),
        certificates,
        acme_socket_dir: state.cfg.angie.acme_socket_dir.clone(),
    })
}

/// Generate + header-wrap + lint the full fileset. This is the panel-side
/// prep shared by preview and apply. Lint failure here is a 400 (the panel
/// should never produce a config its own linter rejects — but we surface it
/// rather than shipping it to the helper).
pub async fn build_fileset(state: &Arc<AppState>) -> ApiResult<FileSet> {
    let input = build_generator_input(state).await?;
    let raw = generator::generate(&input).map_err(ApiError::internal)?;

    // Wrap each file with the MANAGED-BY header.
    let mut wrapped: FileSet = FileSet::new();
    for (name, body) in &raw {
        wrapped.insert(name.clone(), generator::with_header(body));
    }

    // Defense in depth: lint the generated output (the real trust boundary is
    // re-run root-side, but failing early gives a clean error to the UI).
    let policy = crate::apply::lint_policy(&state.cfg);
    let violations = generator::lint::check_fileset(&wrapped, &policy);
    if !violations.is_empty() {
        let detail = violations
            .iter()
            .map(|v| format!("{}: {}", v.file, v.message))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(ApiError::new(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "lint_failed",
            format!("generated config rejected by the directive allowlist: {detail}"),
        ));
    }
    Ok(wrapped)
}
