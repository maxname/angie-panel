//! Config export / import (M4) — the top NPM feature request (backup/export
//! from the UI, open since 2018). Export dumps the FULL config — every host
//! type (proxy / redirect / 404 / stream), certificates, access lists, and
//! settings — as one JSON document; import replaces it transactionally.
//!
//! SECURITY: imported JSON is UNTRUSTED (it is a config-injection vector — the
//! security review flagged this explicitly). Every entry is run through the
//! SAME allowlist validation as the create/update API before any DB write,
//! including bcrypt-hash shape checks on imported basic-auth users (they land
//! in an htpasswd file). The whole import is atomic (all-or-nothing).
//!
//! The document DOES contain sensitive material — advanced-snippet contents and
//! access-list password hashes — so it is a secret at rest (documented as such).

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::model::{
    self, AccessListClientInput, AccessListInput, AccessListUserInput, CertificateInput,
    DeadHostInput, Directive, ProxyHostInput, RedirectHostInput, Satisfy, StreamInput,
    UpstreamPolicy,
};
use crate::repo::{self, AclImportRow, AclUserHash};
use crate::{auth::AuthUser, error::ApiError, error::ApiResult, state::AppState};

/// Bumped from 1 → 2 when redirect/404/stream hosts + access lists joined the
/// document. v1 backups (proxy hosts + certs + settings only) are not accepted.
const EXPORT_VERSION: u32 = 2;

#[derive(Serialize)]
struct Export {
    version: u32,
    exported_at: i64,
    certificates: Vec<Value>,
    access_lists: Vec<Value>,
    hosts: Vec<Value>,
    redirect_hosts: Vec<Value>,
    dead_hosts: Vec<Value>,
    streams: Vec<Value>,
    sni_routers: Vec<Value>,
    dns_credentials: Vec<Value>,
    bans: Vec<Value>,
    settings: HashMap<String, String>,
}

fn to_values<T: Serialize>(items: &[T]) -> Vec<Value> {
    items
        .iter()
        .map(|it| serde_json::to_value(it).unwrap_or(Value::Null))
        .collect()
}

/// GET /api/export → the full config as a JSON document (downloaded by the UI).
/// NOTE: contains secrets (advanced snippets, access-list password hashes) —
/// treated as sensitive in the docs.
pub async fn export(_u: AuthUser, State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let hosts = repo::list_hosts(&state.db).await?;
    let certs = repo::list_certs(&state.db).await?;
    let redirects = repo::list_redirects(&state.db).await?;
    let deads = repo::list_dead(&state.db).await?;
    let streams = repo::list_streams(&state.db).await?;
    let sni_routers = repo::list_sni_routers(&state.db).await?;
    let dns_credentials = repo::list_dns_credentials(&state.db).await?;
    let bans = repo::list_bans(&state.db).await?;
    let mut settings = repo::all_settings(&state.db).await?;

    // Never export internal bookkeeping keys or secrets (DNS provider creds, the
    // hook token). The user re-enters credentials after a restore.
    settings.remove(crate::settings::KEY_LAST_APPLIED_REVISION);
    settings.retain(|k, _| !crate::apply_api::is_secret_setting(k));

    // Access lists carry their user password HASHES so a restore is faithful
    // (the normal list API hides them; the full export is sensitive by design).
    let mut access_lists = Vec::new();
    for l in repo::list_access_lists(&state.db).await? {
        let hashes = repo::acl_user_hashes(&state.db, l.id).await?;
        access_lists.push(json!({
            "id": l.id,
            "name": l.name,
            "satisfy": l.satisfy.as_str(),
            "pass_auth": l.pass_auth,
            "users": hashes
                .iter()
                .map(|u| json!({ "username": u.username, "password_hash": u.password_hash }))
                .collect::<Vec<_>>(),
            "clients": l
                .clients
                .iter()
                .map(|c| json!({ "directive": c.directive.as_str(), "address": c.address }))
                .collect::<Vec<_>>(),
        }));
    }

    let doc = Export {
        version: EXPORT_VERSION,
        exported_at: crate::db::now_epoch(),
        certificates: to_values(&certs),
        access_lists,
        hosts: to_values(&hosts),
        redirect_hosts: to_values(&redirects),
        dead_hosts: to_values(&deads),
        streams: to_values(&streams),
        sni_routers: to_values(&sni_routers),
        dns_credentials: to_values(&dns_credentials),
        bans: to_values(&bans),
        settings,
    };
    Ok(Json(serde_json::to_value(&doc).unwrap_or(Value::Null)))
}

#[derive(Deserialize)]
pub struct ImportDoc {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    certificates: Vec<Value>,
    #[serde(default)]
    access_lists: Vec<Value>,
    #[serde(default)]
    hosts: Vec<Value>,
    #[serde(default)]
    redirect_hosts: Vec<Value>,
    #[serde(default)]
    dead_hosts: Vec<Value>,
    #[serde(default)]
    streams: Vec<Value>,
    #[serde(default)]
    sni_routers: Vec<Value>,
    #[serde(default)]
    dns_credentials: Vec<Value>,
    #[serde(default)]
    bans: Vec<Value>,
    #[serde(default)]
    settings: HashMap<String, String>,
}

const ALLOWED_SETTING_KEYS: &[&str] = &[
    crate::settings::KEY_DEFAULT_SITE,
    crate::settings::KEY_DEFAULT_SITE_REDIRECT,
    crate::settings::KEY_IPV6_ENABLED,
    crate::settings::KEY_RESOLVER_OVERRIDE,
    crate::settings::KEY_ACME_EMAIL,
];

/// One access list as it appears in an import document — like `AccessListInput`
/// but users carry a bcrypt `password_hash` (restore is faithful) instead of a
/// plaintext `password`, and there is an explicit `id`.
#[derive(Deserialize)]
struct AclImportDoc {
    name: String,
    #[serde(default = "default_satisfy")]
    satisfy: Satisfy,
    #[serde(default)]
    pass_auth: bool,
    #[serde(default)]
    users: Vec<AclUserImport>,
    #[serde(default)]
    clients: Vec<AclClientImport>,
}

#[derive(Deserialize)]
struct AclUserImport {
    username: String,
    password_hash: String,
}

#[derive(Deserialize)]
struct AclClientImport {
    directive: Directive,
    address: String,
}

fn default_satisfy() -> Satisfy {
    Satisfy::All
}

/// POST /api/import — validate every entry (same allowlist rules as the CRUD
/// API), then transactionally replace ALL config. Changes materialize only on
/// the next Apply.
pub async fn import(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(doc): Json<ImportDoc>,
) -> ApiResult<Json<Value>> {
    if doc.version != EXPORT_VERSION {
        return Err(ApiError::bad_request(
            "unsupported_version",
            format!(
                "unsupported export version {} (expected {EXPORT_VERSION})",
                doc.version
            ),
        ));
    }

    let policy = UpstreamPolicy {
        allow_loopback: state.cfg.allow_loopback_upstreams,
    };

    // --- certificates ---
    let mut certs: Vec<(i64, CertificateInput)> = Vec::new();
    let mut cert_ids = std::collections::HashSet::new();
    let mut cert_names = std::collections::HashSet::new();
    for (i, v) in doc.certificates.iter().enumerate() {
        let id = extract_id(v).ok_or_else(|| bad_entry("certificate", i, "missing numeric id"))?;
        let input: CertificateInput = serde_json::from_value(v.clone())
            .map_err(|e| bad_entry("certificate", i, &e.to_string()))?;
        let input = model::validate_cert_input(input)?;
        if !cert_ids.insert(id) {
            return Err(bad_entry("certificate", i, "duplicate id"));
        }
        if !cert_names.insert(input.name.clone()) {
            return Err(bad_entry("certificate", i, "duplicate name"));
        }
        certs.push((id, input));
    }

    // --- access lists (structure via validate_acl_input; hashes shape-checked
    //     because they land verbatim in an htpasswd file) ---
    let mut acls: Vec<AclImportRow> = Vec::new();
    let mut acl_ids = std::collections::HashSet::new();
    let mut acl_names = std::collections::HashSet::new();
    for (i, v) in doc.access_lists.iter().enumerate() {
        let id = extract_id(v).ok_or_else(|| bad_entry("access list", i, "missing numeric id"))?;
        let parsed: AclImportDoc = serde_json::from_value(v.clone())
            .map_err(|e| bad_entry("access list", i, &e.to_string()))?;
        // Reuse the CRUD validator for name / usernames / addresses / counts /
        // non-empty (passwords are None here — we carry hashes, not plaintext).
        let structural = AccessListInput {
            name: parsed.name,
            satisfy: parsed.satisfy,
            pass_auth: parsed.pass_auth,
            users: parsed
                .users
                .iter()
                .map(|u| AccessListUserInput {
                    username: u.username.clone(),
                    password: None,
                })
                .collect(),
            clients: parsed
                .clients
                .iter()
                .map(|c| AccessListClientInput {
                    directive: c.directive,
                    address: c.address.clone(),
                })
                .collect(),
        };
        let structural = model::validate_acl_input(structural)?;
        // Pair each validated username with its shape-checked bcrypt hash.
        let mut users = Vec::with_capacity(parsed.users.len());
        for (j, u) in parsed.users.iter().enumerate() {
            let hash = model::validate_bcrypt_hash(&u.password_hash)?;
            users.push(AclUserHash {
                username: structural.users[j].username.clone(),
                password_hash: hash,
            });
        }
        if !acl_ids.insert(id) {
            return Err(bad_entry("access list", i, "duplicate id"));
        }
        if !acl_names.insert(structural.name.clone()) {
            return Err(bad_entry("access list", i, "duplicate name"));
        }
        acls.push(AclImportRow {
            id,
            name: structural.name,
            satisfy: structural.satisfy,
            pass_auth: structural.pass_auth,
            users,
            clients: structural
                .clients
                .iter()
                .map(|c| (c.directive, c.address.clone()))
                .collect(),
        });
    }

    // Domain uniqueness is CROSS-TYPE among enabled hosts (proxy/redirect/dead).
    let mut enabled_domains = std::collections::HashSet::new();

    // --- proxy hosts ---
    let mut hosts: Vec<(i64, ProxyHostInput)> = Vec::new();
    let mut host_ids = std::collections::HashSet::new();
    for (i, v) in doc.hosts.iter().enumerate() {
        let id = extract_id(v).ok_or_else(|| bad_entry("host", i, "missing numeric id"))?;
        let input: ProxyHostInput =
            serde_json::from_value(v.clone()).map_err(|e| bad_entry("host", i, &e.to_string()))?;
        let input = model::validate_host_input(input, state.cfg.allow_advanced_snippets, &policy)?;
        if !host_ids.insert(id) {
            return Err(bad_entry("host", i, "duplicate id"));
        }
        check_cert_ref("host", i, input.certificate_id, &cert_ids)?;
        // access_list_id must reference an access list in THIS import.
        if let Some(aid) = input.access_list_id {
            if !acl_ids.contains(&aid) {
                return Err(bad_entry(
                    "host",
                    i,
                    "access_list_id references an access list not present in the import",
                ));
            }
        }
        claim_domains(&mut enabled_domains, input.enabled, &input.domains)?;
        hosts.push((id, input));
    }

    // --- redirect hosts ---
    let mut redirects: Vec<(i64, RedirectHostInput)> = Vec::new();
    let mut redirect_ids = std::collections::HashSet::new();
    for (i, v) in doc.redirect_hosts.iter().enumerate() {
        let id =
            extract_id(v).ok_or_else(|| bad_entry("redirect host", i, "missing numeric id"))?;
        let input: RedirectHostInput = serde_json::from_value(v.clone())
            .map_err(|e| bad_entry("redirect host", i, &e.to_string()))?;
        let input = model::validate_redirect_input(input, state.cfg.allow_advanced_snippets)?;
        if !redirect_ids.insert(id) {
            return Err(bad_entry("redirect host", i, "duplicate id"));
        }
        check_cert_ref("redirect host", i, input.certificate_id, &cert_ids)?;
        claim_domains(&mut enabled_domains, input.enabled, &input.domains)?;
        redirects.push((id, input));
    }

    // --- 404 (dead) hosts ---
    let mut deads: Vec<(i64, DeadHostInput)> = Vec::new();
    let mut dead_ids = std::collections::HashSet::new();
    for (i, v) in doc.dead_hosts.iter().enumerate() {
        let id = extract_id(v).ok_or_else(|| bad_entry("404 host", i, "missing numeric id"))?;
        let input: DeadHostInput = serde_json::from_value(v.clone())
            .map_err(|e| bad_entry("404 host", i, &e.to_string()))?;
        let input = model::validate_dead_input(input, state.cfg.allow_advanced_snippets)?;
        if !dead_ids.insert(id) {
            return Err(bad_entry("404 host", i, "duplicate id"));
        }
        check_cert_ref("404 host", i, input.certificate_id, &cert_ids)?;
        claim_domains(&mut enabled_domains, input.enabled, &input.domains)?;
        deads.push((id, input));
    }

    // --- streams (incoming-port conflicts among enabled, like the CRUD API) ---
    let mut streams: Vec<(i64, StreamInput)> = Vec::new();
    let mut stream_ids = std::collections::HashSet::new();
    // port -> (tcp_taken, udp_taken) for enabled streams
    let mut port_use: HashMap<u16, (bool, bool)> = HashMap::new();
    for (i, v) in doc.streams.iter().enumerate() {
        let id = extract_id(v).ok_or_else(|| bad_entry("stream", i, "missing numeric id"))?;
        let input: StreamInput = serde_json::from_value(v.clone())
            .map_err(|e| bad_entry("stream", i, &e.to_string()))?;
        let input = model::validate_stream_input(input, &policy)?;
        if input.tls == model::StreamTls::Terminate {
            check_cert_ref("stream", i, input.certificate_id, &cert_ids)?;
        }
        if !stream_ids.insert(id) {
            return Err(bad_entry("stream", i, "duplicate id"));
        }
        if input.enabled {
            let entry = port_use
                .entry(input.incoming_port)
                .or_insert((false, false));
            if (input.tcp && entry.0) || (input.udp && entry.1) {
                return Err(ApiError::new(
                    axum::http::StatusCode::CONFLICT,
                    "port_conflict",
                    format!(
                        "incoming port {} is forwarded by more than one enabled stream in the import",
                        input.incoming_port
                    ),
                ));
            }
            entry.0 |= input.tcp;
            entry.1 |= input.udp;
        }
        streams.push((id, input));
    }

    // --- SNI routers (listen TCP in the stream context, so they share the port
    // space with streams: reuse `port_use` to reject collisions either way) ---
    let mut sni_routers: Vec<(i64, model::SniRouterInput)> = Vec::new();
    let mut sni_router_ids = std::collections::HashSet::new();
    for (i, v) in doc.sni_routers.iter().enumerate() {
        let id = extract_id(v).ok_or_else(|| bad_entry("sni_router", i, "missing numeric id"))?;
        let input: model::SniRouterInput = serde_json::from_value(v.clone())
            .map_err(|e| bad_entry("sni_router", i, &e.to_string()))?;
        let input = model::validate_sni_router_input(input, &policy)?;
        if !sni_router_ids.insert(id) {
            return Err(bad_entry("sni_router", i, "duplicate id"));
        }
        if input.enabled {
            let entry = port_use
                .entry(input.incoming_port)
                .or_insert((false, false));
            if entry.0 {
                return Err(ApiError::new(
                    axum::http::StatusCode::CONFLICT,
                    "port_conflict",
                    format!(
                        "port {} is used by more than one enabled stream / SNI router in the import",
                        input.incoming_port
                    ),
                ));
            }
            entry.0 = true;
        }
        sni_routers.push((id, input));
    }

    // --- DNS credential profiles (validated; ids preserved so certs keep their
    // dns_provider reference). Secrets are not in the export — restored profiles
    // are unconfigured until the operator re-enters credentials. ---
    let mut dns_credentials: Vec<(i64, model::DnsCredentialInput)> = Vec::new();
    let mut dns_cred_ids = std::collections::HashSet::new();
    for (i, v) in doc.dns_credentials.iter().enumerate() {
        let id =
            extract_id(v).ok_or_else(|| bad_entry("dns_credential", i, "missing numeric id"))?;
        let input: model::DnsCredentialInput = serde_json::from_value(v.clone())
            .map_err(|e| bad_entry("dns_credential", i, &e.to_string()))?;
        let input = model::validate_dns_credential_input(input)?;
        if !dns_cred_ids.insert(id) {
            return Err(bad_entry("dns_credential", i, "duplicate id"));
        }
        dns_credentials.push((id, input));
    }
    // A cert's dns_provider (when set) must reference an imported profile.
    for (i, (_, cert)) in certs.iter().enumerate() {
        if let Some(pid) = &cert.dns_provider {
            let ok = pid
                .parse::<i64>()
                .map(|n| dns_cred_ids.contains(&n))
                .unwrap_or(false);
            if !ok {
                return Err(bad_entry(
                    "certificate",
                    i,
                    "dns_provider references a DNS credential profile not in the import",
                ));
            }
        }
    }

    // --- bans (validated like the CRUD API; dedup by address) ---
    let mut bans: Vec<(i64, model::BanInput)> = Vec::new();
    let mut ban_ids = std::collections::HashSet::new();
    let mut ban_addrs = std::collections::HashSet::new();
    for (i, v) in doc.bans.iter().enumerate() {
        let id = extract_id(v).ok_or_else(|| bad_entry("ban", i, "missing numeric id"))?;
        let input: model::BanInput =
            serde_json::from_value(v.clone()).map_err(|e| bad_entry("ban", i, &e.to_string()))?;
        let input = model::validate_ban(input)?;
        if !ban_ids.insert(id) {
            return Err(bad_entry("ban", i, "duplicate id"));
        }
        if !ban_addrs.insert(input.address.clone()) {
            return Err(bad_entry("ban", i, "duplicate address"));
        }
        bans.push((id, input));
    }

    // --- settings keys ---
    let mut settings = HashMap::new();
    for (k, val) in &doc.settings {
        if !ALLOWED_SETTING_KEYS.contains(&k.as_str()) {
            return Err(ApiError::bad_request(
                "unknown_setting",
                format!("unknown setting key in import: {k}"),
            ));
        }
        settings.insert(k.clone(), val.clone());
    }

    repo::import_replace(
        &state.db,
        &certs,
        &acls,
        &hosts,
        &redirects,
        &deads,
        &streams,
        &sni_routers,
        &dns_credentials,
        &bans,
        &settings,
    )
    .await?;

    Ok(Json(json!({
        "ok": true,
        "imported": {
            "certificates": certs.len(),
            "access_lists": acls.len(),
            "hosts": hosts.len(),
            "redirect_hosts": redirects.len(),
            "dead_hosts": deads.len(),
            "streams": streams.len(),
            "sni_routers": sni_routers.len(),
            "dns_credentials": dns_credentials.len(),
            "bans": bans.len(),
            "settings": settings.len(),
        },
    })))
}

/// A cert reference on any host type must point at a certificate in the import.
fn check_cert_ref(
    kind: &str,
    i: usize,
    certificate_id: Option<i64>,
    cert_ids: &std::collections::HashSet<i64>,
) -> ApiResult<()> {
    if let Some(cid) = certificate_id {
        if !cert_ids.contains(&cid) {
            return Err(bad_entry(
                kind,
                i,
                "certificate_id references a certificate not present in the import",
            ));
        }
    }
    Ok(())
}

/// Claim an enabled host's domains in the cross-type set, rejecting duplicates.
fn claim_domains(
    seen: &mut std::collections::HashSet<String>,
    enabled: bool,
    domains: &[String],
) -> ApiResult<()> {
    if !enabled {
        return Ok(());
    }
    for d in domains {
        if !seen.insert(d.clone()) {
            return Err(ApiError::new(
                axum::http::StatusCode::CONFLICT,
                "domain_conflict",
                format!("domain {d} is claimed by more than one enabled host in the import"),
            ));
        }
    }
    Ok(())
}

fn extract_id(v: &Value) -> Option<i64> {
    v.get("id").and_then(Value::as_i64).filter(|id| *id > 0)
}

fn bad_entry(kind: &str, index: usize, detail: &str) -> ApiError {
    ApiError::bad_request(
        "invalid_import",
        format!("{kind} #{index} in the import is invalid: {detail}"),
    )
}
