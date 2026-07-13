//! Effective-settings resolution and FileSet assembly. Bridges the DB/config
//! to the generator: reads the settings table, discovers resolvers from
//! /etc/resolv.conf (override-able), and builds the full GeneratorInput.

use std::sync::Arc;

use crate::error::{ApiError, ApiResult};
use crate::generator::{self, DefaultSite, EffectiveSettings, FileSet, GeneratorInput};
use crate::model::{GeoMode, GeoPolicy};
use crate::repo;
use crate::state::AppState;

// Settings keys (settings table).
pub const KEY_DEFAULT_SITE: &str = "default_site"; // notfound|drop444|redirect|html
pub const KEY_DEFAULT_SITE_REDIRECT: &str = "default_site_redirect_url";
pub const KEY_IPV6_ENABLED: &str = "ipv6_enabled"; // "1"/"0"
pub const KEY_RESOLVER_OVERRIDE: &str = "resolver_override"; // space/comma list
pub const KEY_ACME_EMAIL: &str = "acme_email";
pub const KEY_GEO_MODE: &str = "geo_mode"; // off|deny|allow
pub const KEY_GEO_COUNTRIES: &str = "geo_countries"; // JSON array of ISO codes

/// reg.ru API credentials for DNS-01-via-hook wildcard issuance. Secrets: never
/// returned by the settings GET (only a "configured" flag is), never exported.
pub const KEY_REGRU_USERNAME: &str = "regru_username";
pub const KEY_REGRU_PASSWORD: &str = "regru_password";
/// Random shared secret the ACME hook proxy_pass carries; the panel's hook
/// endpoint rejects requests without it. Auto-generated, not user-editable.
pub const KEY_ACME_HOOK_TOKEN: &str = "acme_hook_token";
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

/// Loopback `host:port` Angie uses to reach the panel's ACME hook. Angie and
/// the panel share a host; a wildcard/loopback bind is reached via 127.0.0.1.
fn hook_target(state: &AppState) -> String {
    let bind = state.cfg.bind_addr.as_str();
    let host = if matches!(bind, "0.0.0.0" | "::" | "127.0.0.1" | "localhost") {
        "127.0.0.1"
    } else {
        bind
    };
    format!("{host}:{}", state.cfg.port)
}

/// Get-or-create the ACME hook shared secret. Generated once (32 random bytes,
/// hex) and stored; stable across restarts so regenerating the config doesn't
/// churn the token on every apply.
async fn ensure_acme_hook_token(state: &AppState) -> ApiResult<String> {
    if let Some(tok) = repo::get_setting(&state.db, KEY_ACME_HOOK_TOKEN)
        .await
        .map_err(ApiError::internal)?
    {
        if !tok.is_empty() {
            return Ok(tok);
        }
    }
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).map_err(ApiError::internal)?;
    let tok = hex::encode(buf);
    repo::set_setting(&state.db, KEY_ACME_HOOK_TOKEN, &tok)
        .await
        .map_err(ApiError::internal)?;
    Ok(tok)
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

/// Read the global geo policy from the settings table.
pub async fn geo_policy(state: &AppState) -> ApiResult<GeoPolicy> {
    let map = repo::all_settings(&state.db).await?;
    let mode = GeoMode::from_stored(map.get(KEY_GEO_MODE).map(String::as_str).unwrap_or("off"));
    let countries = map
        .get(KEY_GEO_COUNTRIES)
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default();
    Ok(GeoPolicy { mode, countries })
}

/// Persist the global geo policy (already validated) to the settings table.
pub async fn set_geo_policy(state: &AppState, policy: &GeoPolicy) -> ApiResult<()> {
    repo::set_setting(&state.db, KEY_GEO_MODE, policy.mode.as_str())
        .await
        .map_err(ApiError::internal)?;
    let countries = serde_json::to_string(&policy.countries).unwrap_or_else(|_| "[]".into());
    repo::set_setting(&state.db, KEY_GEO_COUNTRIES, &countries)
        .await
        .map_err(ApiError::internal)?;
    Ok(())
}

/// Resolve the policy's country codes to CIDR ranges using the bundled dataset
/// (CSV `country_code,cidr`). Returns the CIDRs of the selected countries, in
/// file order. A missing dataset (or no matches) yields an empty list, which the
/// generator treats as "geo inactive" — it never locks a host out on bad data.
pub fn load_geo_cidrs(path: &std::path::Path, policy: &GeoPolicy) -> Vec<String> {
    if !policy.active() {
        return Vec::new();
    }
    let wanted: std::collections::HashSet<&str> =
        policy.countries.iter().map(String::as_str).collect();
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(?path, %e, "geo dataset unavailable; country filtering disabled");
            return Vec::new();
        }
    };
    let mut cidrs = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((code, cidr)) = line.split_once(',') {
            if wanted.contains(code.trim()) {
                cidrs.push(cidr.trim().to_string());
            }
        }
    }
    cidrs
}

/// Assemble the generator input from the current DB state.
pub async fn build_generator_input(state: &AppState) -> ApiResult<GeneratorInput> {
    let hosts = repo::list_hosts(&state.db).await?;
    let settings = effective_settings(state).await?;
    let db_certs = repo::list_certs(&state.db).await?;
    let ready = acme_ready_map(state).await;
    let geo = geo_policy(state).await?;
    let geo_cidrs = load_geo_cidrs(&state.cfg.angie.geoip_data, &geo);

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
            dns_provider: c.dns_provider.map(|p| p.as_str().to_string()),
            // Pause/enabled toggle is a follow-up UI action; default on.
            enabled: true,
        })
        .collect();

    // Access lists, with the bcrypt hashes needed to write the htpasswd files.
    let db_lists = repo::list_access_lists(&state.db).await?;
    let mut access_lists = Vec::with_capacity(db_lists.len());
    for l in &db_lists {
        let hashes = repo::acl_user_hashes(&state.db, l.id).await?;
        access_lists.push(generator::AccessList {
            id: l.id,
            satisfy: l.satisfy.as_str().to_string(),
            pass_auth: l.pass_auth,
            users: hashes
                .into_iter()
                .map(|u| (u.username, u.password_hash))
                .collect(),
            clients: l
                .clients
                .iter()
                .map(|c| (c.directive.as_str().to_string(), c.address.clone()))
                .collect(),
        });
    }

    Ok(GeneratorInput {
        hosts,
        settings,
        snippets_dir: state.cfg.angie.snippets_dir.clone(),
        status_port: status_port(state),
        public_dir: state.cfg.public_dir(),
        certificates,
        acme_socket_dir: state.cfg.angie.acme_socket_dir.clone(),
        acme_hook_target: hook_target(state),
        acme_hook_token: ensure_acme_hook_token(state).await?,
        access_lists,
        http_d_dir: state.cfg.angie.http_d_dir.clone(),
        redirect_hosts: repo::list_redirects(&state.db).await?,
        dead_hosts: repo::list_dead(&state.db).await?,
        streams: repo::list_streams(&state.db).await?,
        sni_routers: repo::list_sni_routers(&state.db).await?,
        bans: repo::list_bans(&state.db).await?,
        geo_policy: geo,
        geo_cidrs,
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
