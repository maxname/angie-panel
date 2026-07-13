//! ACME DNS-01 hook endpoint. Angie's `acme_hook` proxies here on each
//! add/remove step of a provider (reg.ru) DNS-01 challenge; we create or delete
//! the `_acme-challenge` TXT record via the provider API and return 2xx so
//! issuance proceeds (a non-2xx aborts renewal, by design).
//!
//! SECURITY: this endpoint is called by Angie, not a browser — it carries no
//! session. It is therefore exempt from the CSRF/role gate (see
//! `security::is_acme_hook`) and instead authenticated by a high-entropy token
//! in the query string that only the panel-generated Angie config knows. It is
//! reachable on loopback only (the panel binds localhost; the Host allowlist
//! still applies). It performs NO action for a request without a valid token.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;

use crate::regru::{self, RegruCreds};
use crate::settings::{KEY_ACME_HOOK_TOKEN, KEY_REGRU_PASSWORD, KEY_REGRU_USERNAME};
use crate::state::AppState;

/// Optional override of the reg.ru API base (for tests / the e2e harness). Unset
/// in production ⇒ the real API.
pub const KEY_REGRU_API_BASE: &str = "regru_api_base";

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

pub async fn hook(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // 1. Authenticate by the shared token (constant-time). No token → 403, and
    //    crucially no side effects.
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

    let username = setting(&state, KEY_REGRU_USERNAME).await;
    let password = setting(&state, KEY_REGRU_PASSWORD).await;
    if username.is_empty() || password.is_empty() {
        tracing::error!("ACME hook called but reg.ru credentials are not configured");
        return (StatusCode::INTERNAL_SERVER_ERROR, "no credentials").into_response();
    }
    let creds = RegruCreds { username, password };
    let base = {
        let b = setting(&state, KEY_REGRU_API_BASE).await;
        if b.is_empty() {
            regru::REGRU_API_BASE.to_string()
        } else {
            b
        }
    };

    let result = match action {
        "add" => regru::add_txt(&state.http_client, &base, &creds, domain, keyauth).await,
        "remove" => regru::remove_txt(&state.http_client, &base, &creds, domain, keyauth).await,
        other => {
            tracing::warn!(action = other, "unknown ACME hook action");
            Ok(())
        }
    };

    match result {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => {
            tracing::error!(error = %e, action, domain, "ACME hook (reg.ru) failed");
            (StatusCode::BAD_GATEWAY, "hook failed").into_response()
        }
    }
}
