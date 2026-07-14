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
//! (Host allowlist applies). It does NOTHING without a valid token. The type
//! registry + credential-profile CRUD handlers are ordinary admin-gated
//! endpoints. Credentials live only in the hook child's environment.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use hickory_resolver::config::{
    LookupIpStrategy, NameServerConfig, NameServerConfigGroup, Protocol, ResolverConfig,
    ResolverOpts,
};
use hickory_resolver::proto::rr::{RData, RecordType};
use hickory_resolver::TokioAsyncResolver;
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

/// Gather a credential profile's stored values as (env var, value) pairs, keyed
/// by the profile id. `ptype` is the provider TYPE (its fields define which env
/// vars). Returns None if any field is unset (⇒ not configured).
async fn profile_creds(
    state: &AppState,
    profile_id: i64,
    ptype: &ProviderDef,
) -> Option<Vec<(String, String)>> {
    let key = profile_id.to_string();
    let mut out = Vec::with_capacity(ptype.fields.len());
    for field in ptype.fields {
        let v = setting(state, &dns_providers::cred_key(&key, field.env)).await;
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

    // 2. Which credential PROFILE? (baked into the hook URL by the generator as
    //    the profile id.) Resolve it → its provider type → the acme.sh plugin.
    let profile_id: i64 = match params.get("provider").and_then(|s| s.parse().ok()) {
        Some(id) => id,
        None => {
            tracing::error!(provider = ?params.get("provider"), "ACME hook: bad profile id");
            return (StatusCode::INTERNAL_SERVER_ERROR, "bad profile").into_response();
        }
    };
    let profile = match crate::repo::get_dns_credential(&state.db, profile_id).await {
        Ok(Some(p)) => p,
        _ => {
            tracing::error!(profile_id, "ACME hook: credential profile not found");
            return (StatusCode::INTERNAL_SERVER_ERROR, "unknown profile").into_response();
        }
    };
    let ptype = match dns_providers::get(&profile.provider) {
        Some(t) => t,
        None => {
            tracing::error!(
                provider = profile.provider,
                "ACME hook: unknown provider type"
            );
            return (StatusCode::INTERNAL_SERVER_ERROR, "unknown provider").into_response();
        }
    };
    let creds = match profile_creds(&state, profile_id, ptype).await {
        Some(c) => c,
        None => {
            tracing::error!(profile_id, "ACME hook: credentials not configured");
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

    if let Err(e) = run_dnsapi(&state, ptype, fn_action, domain, keyauth, &creds).await {
        tracing::error!(error = %e, profile_id, domain, action, "ACME dnsapi hook failed");
        return (StatusCode::BAD_GATEWAY, "hook failed").into_response();
    }

    // For an `add`, block until the TXT is actually live on the zone's
    // authoritative nameservers before returning. Angie validates the DNS-01
    // challenge immediately after the hook returns and has no propagation wait
    // of its own; every renewal retry uses a fresh keyauth, so a slow DNS
    // provider (e.g. reg.ru, minutes to publish) would otherwise never issue.
    // `rm` needs no wait. The hook's proxy_read_timeout (set by the generator)
    // is sized above PROPAGATION_TIMEOUT so Angie waits for us.
    if fn_action == "add" {
        let fqdn = challenge_fqdn(domain);
        let base = domain.strip_prefix("*.").unwrap_or(domain);
        let p = wait_for_txt_propagation(&fqdn, keyauth, base).await;
        if p.confirmed {
            tracing::info!(
                domain,
                profile_id,
                waited_secs = p.waited.as_secs(),
                "ACME hook: TXT published + visible on authoritative NS"
            );
        } else {
            tracing::warn!(
                domain,
                profile_id,
                waited_secs = p.waited.as_secs(),
                "ACME hook: TXT set but not visible on authoritative NS before timeout — \
                 the CA check may fail; DNS provider may be slow to propagate"
            );
        }
    } else {
        tracing::info!(domain, profile_id, "ACME hook: TXT removed");
    }
    StatusCode::OK.into_response()
}

/// The `_acme-challenge` FQDN for a (possibly wildcard) domain.
fn challenge_fqdn(domain: &str) -> String {
    format!(
        "_acme-challenge.{}",
        domain.strip_prefix("*.").unwrap_or(domain)
    )
}

/// How long the challenge TXT took to become visible on the authoritative
/// nameservers (or that it did not, before the timeout).
struct Propagation {
    confirmed: bool,
    waited: Duration,
}

/// Block until `value` is served for `fqdn`'s TXT by the zone's authoritative
/// nameservers — the source of truth the CA will query — or a timeout. Polling
/// the authoritative servers directly (not a recursive resolver) avoids
/// negative-caching: a recursive resolver can pin "no record" for the zone's
/// negative-TTL long after the record is live.
async fn wait_for_txt_propagation(fqdn: &str, value: &str, base_domain: &str) -> Propagation {
    const TIMEOUT: Duration = Duration::from_secs(170);
    const INTERVAL: Duration = Duration::from_secs(4);
    let start = Instant::now();

    // v4-only public meta-resolver to discover the zone's authoritative NS and
    // their addresses. Many self-hosted boxes have no working IPv6, and trying
    // AAAA there just wastes time.
    let mut opts = ResolverOpts::default();
    opts.ip_strategy = LookupIpStrategy::Ipv4Only;
    opts.cache_size = 0;
    let meta = TokioAsyncResolver::tokio(ResolverConfig::cloudflare(), opts.clone());

    // A resolver aimed straight at the authoritative NS; if we can't discover
    // them, fall back to the recursive meta-resolver (better than not waiting).
    let auth = authoritative_resolver(&meta, base_domain, &opts)
        .await
        .unwrap_or(meta);

    loop {
        if txt_contains(&auth, fqdn, value).await {
            return Propagation {
                confirmed: true,
                waited: start.elapsed(),
            };
        }
        if start.elapsed() >= TIMEOUT {
            return Propagation {
                confirmed: false,
                waited: start.elapsed(),
            };
        }
        tokio::time::sleep(INTERVAL).await;
    }
}

/// Build a resolver that queries the zone's authoritative nameservers directly
/// (no recursion, caching disabled). Returns None if the NS can't be found.
async fn authoritative_resolver(
    meta: &TokioAsyncResolver,
    base_domain: &str,
    opts: &ResolverOpts,
) -> Option<TokioAsyncResolver> {
    let ns = meta
        .lookup(format!("{base_domain}."), RecordType::NS)
        .await
        .ok()?;
    let mut ips: Vec<IpAddr> = Vec::new();
    for rdata in ns.iter() {
        if let RData::NS(name) = rdata {
            if let Ok(addrs) = meta.lookup_ip(name.0.to_utf8()).await {
                ips.extend(addrs.iter());
            }
        }
    }
    if ips.is_empty() {
        return None;
    }
    let mut group = NameServerConfigGroup::new();
    for ip in ips {
        group.push(NameServerConfig::new(
            SocketAddr::new(ip, 53),
            Protocol::Udp,
        ));
    }
    let cfg = ResolverConfig::from_parts(None, vec![], group);
    Some(TokioAsyncResolver::tokio(cfg, opts.clone()))
}

/// True if the fqdn's TXT RRset contains exactly `value`.
async fn txt_contains(resolver: &TokioAsyncResolver, fqdn: &str, value: &str) -> bool {
    let name = format!("{fqdn}.");
    match resolver.lookup(name, RecordType::TXT).await {
        Ok(lookup) => lookup.iter().any(|rdata| {
            if let RData::TXT(txt) = rdata {
                // A TXT record is one or more character-strings; concatenate.
                let joined: Vec<u8> = txt.iter().flatten().copied().collect();
                joined == value.as_bytes()
            } else {
                false
            }
        }),
        Err(_) => false,
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

    let fqdn = challenge_fqdn(domain);

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

// -------------------------------------------- provider types + profiles API

/// GET /api/dns-providers — the static registry of provider TYPES (id, label,
/// credential fields). Used by the UI to build the "add profile" form.
pub async fn list_providers(_u: AuthUser) -> ApiResult<Json<Value>> {
    let providers: Vec<Value> = dns_providers::PROVIDERS
        .iter()
        .map(|p| {
            json!({
                "id": p.id,
                "label": p.label,
                "fields": p.fields.iter().map(|f| json!({"env": f.env, "label": f.label})).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(Json(json!({ "providers": providers })))
}

/// Serialise a profile with its provider label + configured flag.
async fn profile_json(state: &AppState, c: &crate::model::DnsCredential) -> Value {
    let ptype = dns_providers::get(&c.provider);
    let configured = match ptype {
        Some(t) => profile_creds(state, c.id, t).await.is_some(),
        None => false,
    };
    json!({
        "id": c.id,
        "provider": c.provider,
        "provider_label": ptype.map(|t| t.label).unwrap_or(&c.provider),
        "name": c.name,
        "configured": configured,
    })
}

/// GET /api/dns-credentials — the operator's credential profiles.
pub async fn list_credentials(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<Value>> {
    let rows = crate::repo::list_dns_credentials(&state.db).await?;
    let mut profiles = Vec::with_capacity(rows.len());
    for c in &rows {
        profiles.push(profile_json(&state, c).await);
    }
    Ok(Json(json!({ "credentials": profiles })))
}

#[derive(serde::Deserialize)]
pub struct CreateCredentialBody {
    provider: String,
    name: String,
    #[serde(default)]
    credentials: HashMap<String, String>,
}

/// POST /api/dns-credentials — create a profile (provider type + name) and store
/// its credentials. Only the type's own credential fields are accepted.
pub async fn create_credential(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateCredentialBody>,
) -> ApiResult<Json<Value>> {
    let input = crate::model::validate_dns_credential_input(crate::model::DnsCredentialInput {
        provider: body.provider,
        name: body.name,
    })?;
    let ptype = dns_providers::get(&input.provider).expect("validated");
    reject_unknown_fields(ptype, &body.credentials)?;
    let id = crate::repo::insert_dns_credential(&state.db, &input).await?;
    store_creds(&state, id, &body.credentials).await?;
    let profile = crate::repo::get_dns_credential(&state.db, id)
        .await?
        .expect("just inserted");
    Ok(Json(profile_json(&state, &profile).await))
}

#[derive(serde::Deserialize)]
pub struct UpdateCredentialBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    credentials: Option<HashMap<String, String>>,
}

/// PUT /api/dns-credentials/{id} — rename and/or update a profile's credentials.
pub async fn update_credential(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateCredentialBody>,
) -> ApiResult<Json<Value>> {
    let profile = crate::repo::get_dns_credential(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no DNS credential profile #{id}")))?;
    let ptype = dns_providers::get(&profile.provider)
        .ok_or_else(|| ApiError::internal("profile has an unknown provider type"))?;

    if let Some(name) = body.name {
        // Reuse the input validator for the name (provider is unchanged).
        let checked =
            crate::model::validate_dns_credential_input(crate::model::DnsCredentialInput {
                provider: profile.provider.clone(),
                name,
            })?;
        crate::repo::update_dns_credential_name(&state.db, id, &checked.name).await?;
    }
    if let Some(creds) = &body.credentials {
        reject_unknown_fields(ptype, creds)?;
        store_creds(&state, id, creds).await?;
    }
    let updated = crate::repo::get_dns_credential(&state.db, id)
        .await?
        .expect("exists");
    Ok(Json(profile_json(&state, &updated).await))
}

/// DELETE /api/dns-credentials/{id} — remove a profile (and its stored creds).
/// Blocked while a certificate references it.
pub async fn delete_credential(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    if crate::repo::dns_credential_in_use(&state.db, id).await? {
        return Err(ApiError::new(
            axum::http::StatusCode::CONFLICT,
            "in_use",
            "a certificate uses this DNS provider profile; detach it first",
        ));
    }
    if !crate::repo::delete_dns_credential(&state.db, id).await? {
        return Err(ApiError::not_found(format!(
            "no DNS credential profile #{id}"
        )));
    }
    Ok(Json(json!({ "ok": true })))
}

/// Reject any credential key that is not one of the provider type's fields.
fn reject_unknown_fields(ptype: &ProviderDef, creds: &HashMap<String, String>) -> ApiResult<()> {
    for env in creds.keys() {
        if !ptype.fields.iter().any(|f| f.env == env) {
            return Err(ApiError::bad_request(
                "unknown_field",
                format!("'{env}' is not a credential field of {}", ptype.id),
            ));
        }
    }
    Ok(())
}

/// Store a profile's credentials (write-only) under `dns_cred:<id>:<env>`.
async fn store_creds(state: &AppState, id: i64, creds: &HashMap<String, String>) -> ApiResult<()> {
    let key = id.to_string();
    for (env, value) in creds {
        crate::repo::set_setting(&state.db, &dns_providers::cred_key(&key, env), value.trim())
            .await
            .map_err(ApiError::internal)?;
    }
    Ok(())
}
