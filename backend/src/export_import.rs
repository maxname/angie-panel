//! Config export / import (M4) — the top NPM feature request (backup/export
//! from the UI, open since 2018). Export dumps the full host + certificate +
//! settings state as JSON; import replaces it transactionally.
//!
//! SECURITY: imported JSON is UNTRUSTED (it is a config-injection vector — the
//! security review flagged this explicitly). Every host and certificate is run
//! through the SAME allowlist validation as the create/update API before any
//! DB write, and the whole import is atomic (all-or-nothing).

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::model::{self, CertificateInput, ProxyHostInput, UpstreamPolicy};
use crate::repo;
use crate::state::AppState;

const EXPORT_VERSION: u32 = 1;

#[derive(Serialize)]
struct Export {
    version: u32,
    exported_at: i64,
    hosts: Vec<Value>,
    certificates: Vec<Value>,
    settings: HashMap<String, String>,
}

/// GET /api/export → the full config as a JSON document (downloaded by the UI).
/// NOTE: may contain secrets embedded in advanced snippets — treated as
/// sensitive in the docs.
pub async fn export(_u: AuthUser, State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let hosts = repo::list_hosts(&state.db).await?;
    let certs = repo::list_certs(&state.db).await?;
    let settings = repo::all_settings(&state.db).await?;

    // Never export internal bookkeeping keys.
    let mut settings: HashMap<String, String> = settings;
    settings.remove(crate::settings::KEY_LAST_APPLIED_REVISION);

    let doc = Export {
        version: EXPORT_VERSION,
        exported_at: crate::db::now_epoch(),
        hosts: hosts
            .iter()
            .map(|h| serde_json::to_value(h).unwrap_or(Value::Null))
            .collect(),
        certificates: certs
            .iter()
            .map(|c| serde_json::to_value(c).unwrap_or(Value::Null))
            .collect(),
        settings,
    };
    Ok(Json(serde_json::to_value(&doc).unwrap_or(Value::Null)))
}

#[derive(Deserialize)]
pub struct ImportDoc {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    hosts: Vec<Value>,
    #[serde(default)]
    certificates: Vec<Value>,
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

/// POST /api/import — validate every entry, then transactionally replace the
/// current hosts + certificates. Changes materialize only on the next Apply.
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

    // --- validate certificates ---
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

    // Access lists are not part of the export document (managed via their own
    // CRUD); import leaves them intact. A host may only reference one that
    // currently exists, so fetch the live ids to validate against.
    let existing_acls: std::collections::HashSet<i64> = repo::list_access_lists(&state.db)
        .await?
        .into_iter()
        .map(|l| l.id)
        .collect();

    // --- validate hosts (and referential integrity to certs / access lists) ---
    let mut hosts: Vec<(i64, ProxyHostInput)> = Vec::new();
    let mut host_ids = std::collections::HashSet::new();
    let mut enabled_domains = std::collections::HashSet::new();
    for (i, v) in doc.hosts.iter().enumerate() {
        let id = extract_id(v).ok_or_else(|| bad_entry("host", i, "missing numeric id"))?;
        let input: ProxyHostInput =
            serde_json::from_value(v.clone()).map_err(|e| bad_entry("host", i, &e.to_string()))?;
        let input = model::validate_host_input(input, state.cfg.allow_advanced_snippets, &policy)?;
        if !host_ids.insert(id) {
            return Err(bad_entry("host", i, "duplicate id"));
        }
        // certificate_id must reference an imported certificate.
        if let Some(cid) = input.certificate_id {
            if !cert_ids.contains(&cid) {
                return Err(bad_entry(
                    "host",
                    i,
                    "certificate_id references an unknown certificate",
                ));
            }
        }
        // access_list_id must reference an existing access list.
        if let Some(aid) = input.access_list_id {
            if !existing_acls.contains(&aid) {
                return Err(bad_entry(
                    "host",
                    i,
                    "access_list_id references an access list that does not exist \
                     (create the access list before importing, or remove the reference)",
                ));
            }
        }
        // Enforce the same domain-uniqueness rule as the API among enabled hosts.
        if input.enabled {
            for d in &input.domains {
                if !enabled_domains.insert(d.clone()) {
                    return Err(ApiError::new(
                        axum::http::StatusCode::CONFLICT,
                        "domain_conflict",
                        format!(
                            "domain {d} is claimed by more than one enabled host in the import"
                        ),
                    ));
                }
            }
        }
        hosts.push((id, input));
    }

    // --- validate settings keys ---
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

    repo::import_replace(&state.db, &certs, &hosts, &settings).await?;

    Ok(Json(json!({
        "ok": true,
        "imported": { "hosts": hosts.len(), "certificates": certs.len(), "settings": settings.len() },
    })))
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
