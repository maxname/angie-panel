//! ACME DNS-01 provider hook + provider registry API. Angie's `acme_hook`
//! proxies to [`hook`] on each add/remove step of a provider DNS-01 challenge;
//! we set/delete the `_acme-challenge` TXT via the chosen provider's acme.sh
//! dnsapi plugin and return 2xx so issuance proceeds (non-2xx aborts renewal).
//!
//! The bridge: export the operator's stored credentials as the plugin's env
//! vars, source acme.sh's core helpers + the plugin, and call
//! `dns_<plugin>_add`/`_rm "$fqdn" "$value"`. This is exactly how acme.sh runs
//! them; verified on real Angie + pebble (mock plugin issued a cert) and a real
//! plugin (dns_cf) reaching its provider API standalone.
//!
//! SECURITY: [`hook`] is called by Angie, not a browser — no session. It is
//! exempt from the CSRF/role gate (see `security::is_acme_hook`) and instead
//! authenticated by a high-entropy token in the query string; loopback-only
//! (Host allowlist applies). It does NOTHING without a valid token. The registry
//! API handlers ([`list_providers`], [`set_credentials`]) are ordinary
//! admin-gated endpoints. Credentials live only in the hook child's environment.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::dns_providers::{self, ProviderDef};
use crate::error::{ApiError, ApiResult};
use crate::settings::KEY_ACME_HOOK_TOKEN;
use crate::state::AppState;

/// Constant-time byte comparison so the token check can't be timed.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn header<'a>(h: &'a HeaderMap, name: &str) -> &'a str {
    h.get(name).and_then(|v| v.to_str().ok()).unwrap_or("")
}

async fn setting(state: &AppState, key: &str) -> String {
    crate::repo::get_setting(&state.db, key)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Gather a provider's stored credentials as (env var, value) pairs. Returns
/// None if any required field is unset (⇒ not configured).
async fn provider_creds(state: &AppState, p: &ProviderDef) -> Option<Vec<(String, String)>> {
    let mut out = Vec::with_capacity(p.fields.len());
    for field in p.fields {
        let v = setting(state, &dns_providers::cred_key(p.id, field.env)).await;
        if v.is_empty() {
            return None;
        }
        out.push((field.env.to_string(), v));
    }
    Some(out)
}

pub async fn hook(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // 1. Authenticate by the shared token (constant-time). No token → 403, no
    //    side effects.
    let expected = setting(&state, KEY_ACME_HOOK_TOKEN).await;
    let given = params.get("t").map(String::as_str).unwrap_or("");
    if expected.is_empty() || !ct_eq(expected.as_bytes(), given.as_bytes()) {
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }

    let action = header(&headers, "x-acme-hook"); // add | remove
    let challenge = header(&headers, "x-acme-challenge"); // dns
    let domain = header(&headers, "x-acme-domain");
    let keyauth = header(&headers, "x-acme-keyauth");

    // Only DNS-01 involves a TXT record; anything else is a no-op success.
    if challenge != "dns" {
        return StatusCode::OK.into_response();
    }

    // 2. Which provider? (baked into the hook URL by the generator.)
    let provider = match params.get("provider").and_then(|id| dns_providers::get(id)) {
        Some(p) => p,
        None => {
            tracing::error!(provider = ?params.get("provider"), "ACME hook: unknown provider");
            return (StatusCode::INTERNAL_SERVER_ERROR, "unknown provider").into_response();
        }
    };
    let creds = match provider_creds(&state, provider).await {
        Some(c) => c,
        None => {
            tracing::error!(
                provider = provider.id,
                "ACME hook: credentials not configured"
            );
            return (StatusCode::INTERNAL_SERVER_ERROR, "no credentials").into_response();
        }
    };

    // acme.sh dnsapi function suffix: "add" | "rm".
    let fn_action = match action {
        "add" => "add",
        "remove" => "rm",
        other => {
            tracing::warn!(action = other, "ACME hook: unknown action");
            return StatusCode::OK.into_response();
        }
    };

    match run_dnsapi(&state, provider, fn_action, domain, keyauth, &creds).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => {
            tracing::error!(error = %e, provider = provider.id, domain, "ACME dnsapi hook failed");
            (StatusCode::BAD_GATEWAY, "hook failed").into_response()
        }
    }
}

/// Run the provider's acme.sh dnsapi plugin: source acme.sh core + the plugin,
/// then call `dns_<plugin>_<add|rm> "$fqdn" "$value"`. Credentials + paths go in
/// via the child environment only. `plugin`/`fn_action` come from the validated
/// registry / a fixed map (no injection); fqdn/value are passed as positional
/// args, not interpolated into the script.
async fn run_dnsapi(
    state: &AppState,
    provider: &ProviderDef,
    fn_action: &str,
    domain: &str,
    keyauth: &str,
    creds: &[(String, String)],
) -> anyhow::Result<()> {
    let acme_dir = &state.cfg.angie.acme_sh_dir;
    let acme_sh = acme_dir.join("acme.sh");
    let dnsapi = acme_dir.join("dnsapi");
    // acme.sh reads/writes account.conf under HOME/.acme.sh — give it a writable
    // spot inside the panel's data dir.
    let home = state.cfg.data_dir.join("acme-home");
    tokio::fs::create_dir_all(&home).await.ok();

    let fqdn = format!(
        "_acme-challenge.{}",
        domain.strip_prefix("*.").unwrap_or(domain)
    );

    // Script uses positional args ($1 plugin, $2 action) so nothing is
    // interpolated. acme.sh core (sourced) provides _get/_post/… the plugins
    // need. NO `set -eu`: sourcing acme.sh references unset vars, which `set -u`
    // would abort on (verified). Sourcing output is muted; the exit status is the
    // plugin function's own — that is what tells us success/failure.
    const SCRIPT: &str = r#"plugin="$1"; action="$2"
. "$AP_ACME_SH" >/dev/null 2>&1
. "$AP_DNSAPI/dns_${plugin}.sh" >/dev/null 2>&1
"dns_${plugin}_${action}" "$AP_FQDN" "$AP_VALUE""#;

    let mut cmd = tokio::process::Command::new("bash");
    cmd.arg("-c")
        .arg(SCRIPT)
        .arg("bash") // $0
        .arg(provider.plugin)
        .arg(fn_action)
        .env("HOME", &home)
        .env("LE_WORKING_DIR", &home)
        .env("AP_ACME_SH", &acme_sh)
        .env("AP_DNSAPI", &dnsapi)
        .env("AP_FQDN", &fqdn)
        .env("AP_VALUE", keyauth)
        .kill_on_drop(true);
    for (env, value) in creds {
        cmd.env(env, value);
    }

    let out = cmd.output().await?;
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let tail: String = stderr.lines().rev().take(3).collect::<Vec<_>>().join(" | ");
        anyhow::bail!("dns_{}_{} failed: {tail}", provider.plugin, fn_action);
    }
}

// ------------------------------------------------------ provider registry API

/// GET /api/dns-providers — the registry + whether each provider's creds are set.
pub async fn list_providers(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<Value>> {
    let mut providers = Vec::with_capacity(dns_providers::PROVIDERS.len());
    for p in dns_providers::PROVIDERS {
        let configured = provider_creds(&state, p).await.is_some();
        providers.push(json!({
            "id": p.id,
            "label": p.label,
            "fields": p.fields.iter().map(|f| json!({"env": f.env, "label": f.label})).collect::<Vec<_>>(),
            "configured": configured,
        }));
    }
    Ok(Json(json!({ "providers": providers })))
}

#[derive(serde::Deserialize)]
pub struct CredentialsBody {
    /// env var → value. An empty value clears that field.
    credentials: HashMap<String, String>,
}

/// PUT /api/dns-providers/{id}/credentials — save (or clear) a provider's creds.
/// Write-only: the values are never returned. Only the provider's own fields are
/// accepted.
pub async fn set_credentials(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<CredentialsBody>,
) -> ApiResult<Json<Value>> {
    let provider = dns_providers::get(&id)
        .ok_or_else(|| ApiError::not_found(format!("unknown DNS provider '{id}'")))?;
    for (env, value) in &body.credentials {
        if !provider.fields.iter().any(|f| f.env == env) {
            return Err(ApiError::bad_request(
                "unknown_field",
                format!("'{env}' is not a credential field of {}", provider.id),
            ));
        }
        crate::repo::set_setting(
            &state.db,
            &dns_providers::cred_key(provider.id, env),
            value.trim(),
        )
        .await
        .map_err(ApiError::internal)?;
    }
    let configured = provider_creds(&state, provider).await.is_some();
    Ok(Json(json!({ "ok": true, "configured": configured })))
}
