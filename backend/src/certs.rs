//! Certificate CRUD + issuance status. Certificates drive Angie's built-in
//! ACME (no certbot): a certificate row becomes an `acme_client` + collector
//! block at generation time (see generator::gen_acme). Issuance status is read
//! live from `/status/http/acme_clients/<name>`.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::model::{self, Certificate};
use crate::{repo, settings};

fn cert_json(c: &Certificate) -> Value {
    serde_json::to_value(c).unwrap_or(Value::Null)
}

/// List certificates, each annotated with live issuance status from the
/// Angie status API (state/certificate/details/next_run) when reachable.
pub async fn list(_u: AuthUser, State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let certs = repo::list_certs(&state.db).await?;
    let status = acme_status_map(&state).await;
    let arr: Vec<Value> = certs
        .iter()
        .map(|c| {
            let mut v = cert_json(c);
            if let Some(obj) = v.as_object_mut() {
                obj.insert(
                    "status".into(),
                    status.get(&c.name).cloned().unwrap_or(Value::Null),
                );
            }
            v
        })
        .collect();
    Ok(Json(json!({ "certificates": arr })))
}

pub async fn get_one(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let cert = repo::get_cert(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no certificate #{id}")))?;
    let status = acme_status_map(&state).await;
    let mut v = cert_json(&cert);
    if let Some(obj) = v.as_object_mut() {
        obj.insert(
            "status".into(),
            status.get(&cert.name).cloned().unwrap_or(Value::Null),
        );
    }
    Ok(Json(v))
}

pub async fn create(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(raw): Json<model::CertificateInput>,
) -> ApiResult<Json<Value>> {
    let input = model::validate_cert_input(raw)?;
    ensure_dns_provider_exists(&state, &input).await?;
    // Name is the acme_client identifier and the $acme_cert_<name> variable —
    // globally unique.
    if repo::cert_name_exists(&state.db, &input.name).await? {
        return Err(name_taken(&input.name));
    }
    let id = repo::insert_cert(&state.db, &input).await?;
    let cert = repo::get_cert(&state.db, id).await?.expect("just inserted");
    Ok(Json(cert_json(&cert)))
}

/// Replace a certificate's definition in place. Editing keeps the same row id,
/// so every host that references this cert stays bound (hosts reference by
/// `certificate_id`) — even an in-use cert can be edited without detaching it.
/// Changing name/domains/challenge/key makes Angie re-issue on the next apply;
/// there is no separate issuance state in the DB to reset (status is read live
/// from Angie). It behaves like a recreate that preserves the binding.
pub async fn update(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(raw): Json<model::CertificateInput>,
) -> ApiResult<Json<Value>> {
    if repo::get_cert(&state.db, id).await?.is_none() {
        return Err(ApiError::not_found(format!("no certificate #{id}")));
    }
    let input = model::validate_cert_input(raw)?;
    ensure_dns_provider_exists(&state, &input).await?;
    if repo::cert_name_exists_except(&state.db, &input.name, id).await? {
        return Err(name_taken(&input.name));
    }
    repo::update_cert(&state.db, id, &input).await?;
    let cert = repo::get_cert(&state.db, id).await?.expect("just updated");
    Ok(Json(cert_json(&cert)))
}

/// A DNS-01 provider reference must point at an existing credential profile.
async fn ensure_dns_provider_exists(
    state: &AppState,
    input: &model::CertificateInput,
) -> ApiResult<()> {
    if let Some(pid) = &input.dns_provider {
        let exists = match pid.parse::<i64>() {
            Ok(id) => repo::get_dns_credential(&state.db, id).await?.is_some(),
            Err(_) => false,
        };
        if !exists {
            return Err(ApiError::bad_request(
                "invalid_dns_provider",
                "the selected DNS provider profile does not exist",
            ));
        }
    }
    Ok(())
}

fn name_taken(name: &str) -> ApiError {
    ApiError::new(
        axum::http::StatusCode::CONFLICT,
        "cert_name_taken",
        format!("certificate name '{name}' is already in use"),
    )
}

pub async fn delete(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    // Refuse to delete a certificate a host still references — otherwise the
    // host would silently drop to plain HTTP on the next apply (a downgrade).
    let referencing = repo::hosts_using_cert(&state.db, id).await?;
    if !referencing.is_empty() {
        return Err(ApiError::new(
            axum::http::StatusCode::CONFLICT,
            "cert_in_use",
            format!(
                "certificate is used by host(s) {}; detach them first",
                referencing.join(", ")
            ),
        ));
    }
    if !repo::delete_cert(&state.db, id).await? {
        return Err(ApiError::not_found(format!("no certificate #{id}")));
    }
    // NOTE: Angie owns /var/lib/angie/acme/<name>/ (account key, cert, key);
    // we intentionally leave it in place (the panel has no rights there).
    Ok(Json(json!({ "ok": true })))
}

/// Fetch `/status/http/acme_clients/` as a name → status-object map.
async fn acme_status_map(state: &AppState) -> std::collections::HashMap<String, Value> {
    let url = format!(
        "{}/http/acme_clients/",
        state.cfg.angie.status_api_url.trim_end_matches('/')
    );
    let mut map = std::collections::HashMap::new();
    if let Ok(resp) = state.http_client.get(&url).send().await {
        if let Ok(Value::Object(obj)) = resp.json::<Value>().await {
            for (name, v) in obj {
                map.insert(name, v);
            }
        }
    }
    map
}

// Re-export AppState for the handler signatures above.
use crate::state::AppState;

/// Best-effort DNS/reachability precheck before issuance (PLAN.md §5). For
/// http-01/alpn we can only confirm the domains resolve to *some* address; the
/// real reachability check happens when Angie attempts issuance. This gives
/// the user a fast, friendly "does this look right" signal.
pub async fn precheck(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let cert = repo::get_cert(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no certificate #{id}")))?;
    // Delegating a full DNS resolver check to M2-followup; for now report the
    // effective resolvers and the delegation hint for dns-01 wildcards.
    let eff = settings::effective_settings(&state).await?;
    let mut hints: Vec<Value> = Vec::new();
    for d in &cert.domains {
        if d.starts_with("*.") {
            let base = d.trim_start_matches("*.");
            hints.push(json!({
                "domain": d,
                "requires": "dns-01 NS delegation",
                "records": [
                    format!("_acme-challenge.{base}. IN NS <this-host>."),
                ],
            }));
        }
    }
    Ok(Json(json!({
        "challenge": cert.challenge.as_str(),
        "resolvers": eff.resolvers,
        "delegation_hints": hints,
    })))
}
