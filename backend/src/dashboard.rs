//! Live dashboard (M3). Aggregates Angie's read-only /status API into a single
//! typed view: server health, per-host request/upstream metrics, certificate
//! issuance state, config drift, and computed alerts. All /status subtrees are
//! fetched concurrently and every one degrades gracefully when unreachable
//! (off-device or Angie down) — the dashboard then reports `angie.up = false`.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::error::ApiResult;
use crate::state::AppState;
use crate::{apply, repo, settings};

async fn fetch_status(state: &AppState, path: &str) -> Option<Value> {
    let url = format!(
        "{}/{}",
        state.cfg.angie.status_api_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    let resp = state.http_client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json::<Value>().await.ok()
}

fn zone_key(host_id: i64) -> String {
    format!("host_{host_id}")
}

/// Summarize an upstream's peers into a health verdict for the host row.
fn upstream_health(upstream: &Value) -> Value {
    let peers = upstream.get("peers").and_then(Value::as_object);
    let (mut up, mut down, mut fails) = (0u64, 0u64, 0u64);
    if let Some(peers) = peers {
        for peer in peers.values() {
            match peer.get("state").and_then(Value::as_str) {
                Some("up") => up += 1,
                Some(_) => down += 1,
                None => {}
            }
            fails += peer
                .get("health")
                .and_then(|h| h.get("fails"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
        }
    }
    json!({ "peers_up": up, "peers_down": down, "fails": fails })
}

pub async fn get_dashboard(
    _u: AuthUser,
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<Value>> {
    // Fetch every /status subtree + the local view concurrently.
    let (angie, connections, server_zones, upstreams, acme_clients, hosts, certs, streams, preview) = tokio::join!(
        fetch_status(&state, "angie"),
        fetch_status(&state, "connections"),
        fetch_status(&state, "http/server_zones/"),
        fetch_status(&state, "http/upstreams/"),
        fetch_status(&state, "http/acme_clients/"),
        repo::list_hosts(&state.db),
        repo::list_certs(&state.db),
        repo::list_streams(&state.db),
        settings::build_fileset(&state),
    );

    let hosts = hosts?;
    let certs = certs?;
    let streams = streams?;
    let up = angie.is_some();

    // --- angie summary ---
    let angie_summary = json!({
        "up": up,
        "version": angie.as_ref().and_then(|a| a.get("version").cloned()),
        "generation": angie.as_ref().and_then(|a| a.get("generation").cloned()),
        "load_time": angie.as_ref().and_then(|a| a.get("load_time").cloned()),
        "connections": connections,
    });

    // --- per-host rows (zone + upstream keyed by host_<id>) ---
    let zones = server_zones.as_ref().and_then(Value::as_object);
    let ups = upstreams.as_ref().and_then(Value::as_object);
    let cert_by_id: std::collections::HashMap<i64, &crate::model::Certificate> =
        certs.iter().map(|c| (c.id, c)).collect();
    let acme = acme_clients.as_ref().and_then(Value::as_object);

    let host_rows: Vec<Value> = hosts
        .iter()
        .map(|h| {
            let key = zone_key(h.id);
            let zone = zones.and_then(|z| z.get(&key));
            let upstream = ups.and_then(|u| u.get(&key)).map(upstream_health);
            // HTTPS is "active" when a cert is attached and Angie reports it valid.
            let cert = h.certificate_id.and_then(|cid| cert_by_id.get(&cid));
            let https_active = cert
                .and_then(|c| acme.and_then(|a| a.get(&c.name)))
                .and_then(|s| s.get("certificate"))
                .and_then(Value::as_str)
                == Some("valid");
            json!({
                "id": h.id,
                "domains": h.domains,
                "enabled": h.enabled,
                "forward": format!("{}://{}:{}", h.forward_scheme.as_str(), h.forward_host, h.forward_port),
                "certificate_id": h.certificate_id,
                "https_active": https_active,
                "zone": zone,
                "upstream": upstream,
            })
        })
        .collect();

    // --- certificates with live issuance status ---
    let cert_rows: Vec<Value> = certs
        .iter()
        .map(|c| {
            let status = acme.and_then(|a| a.get(&c.name)).cloned();
            json!({
                "id": c.id,
                "name": c.name,
                "domains": c.domains,
                "challenge": c.challenge.as_str(),
                "staging": c.staging,
                "status": status,
            })
        })
        .collect();

    // --- drift + pending changes (reuse the apply preview) ---
    let diff = preview
        .as_ref()
        .ok()
        .and_then(|fs| apply::preview(&state.cfg, fs).ok());
    let has_drift = diff.as_ref().map(|d| d.has_drift).unwrap_or(false);
    let pending = diff.as_ref().map(|d| d.has_changes()).unwrap_or(false);
    let foreign: Vec<String> = diff
        .as_ref()
        .map(|d| d.foreign.iter().map(|f| f.name.clone()).collect())
        .unwrap_or_default();

    // --- alerts ---
    let mut alerts: Vec<Value> = Vec::new();
    if !up {
        alerts.push(json!({"severity":"error","code":"angie_down","message":"Angie status API is unreachable"}));
    }
    for c in &certs {
        if let Some(st) = acme.and_then(|a| a.get(&c.name)) {
            let state_s = st.get("state").and_then(Value::as_str).unwrap_or("");
            let cert_s = st.get("certificate").and_then(Value::as_str).unwrap_or("");
            if state_s == "failed" {
                alerts.push(json!({"severity":"error","code":"cert_failed",
                    "message":format!("Certificate '{}' issuance failed: {}", c.name,
                        st.get("details").and_then(Value::as_str).unwrap_or(""))}));
            } else if cert_s == "expired" {
                alerts.push(json!({"severity":"error","code":"cert_expired",
                    "message":format!("Certificate '{}' has expired", c.name)}));
            }
        }
    }
    if has_drift {
        alerts.push(json!({"severity":"warning","code":"drift",
            "message":"A managed config file was edited on disk; re-apply to restore"}));
    }
    if pending {
        alerts.push(json!({"severity":"info","code":"pending",
            "message":"There are unapplied changes"}));
    }

    // --- streams (TCP/UDP forwarding) ---
    let stream_ctx_active = crate::streams::context_active(&state);
    let enabled_streams = streams.iter().filter(|s| s.enabled).count();
    if enabled_streams > 0 && !stream_ctx_active {
        alerts.push(json!({"severity":"warning","code":"stream_context_off",
            "message":"TCP/UDP streams are configured but the Angie stream context is off; enable it before applying"}));
    }

    Ok(Json(json!({
        "angie": angie_summary,
        "hosts": host_rows,
        "certificates": cert_rows,
        "streams": { "configured": streams.len(), "enabled": enabled_streams, "context_active": stream_ctx_active },
        "drift": { "detected": has_drift, "foreign_files": foreign },
        "pending_changes": pending,
        "alerts": alerts,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_health_counts_peers_and_fails() {
        // Matches the real /status/http/upstreams/ shape captured from Angie.
        let up = json!({
            "peers": {
                "10.0.0.1:80": {"state": "up", "health": {"fails": 0}},
                "10.0.0.2:80": {"state": "unavailable", "health": {"fails": 3}},
                "10.0.0.3:80": {"state": "down", "health": {"fails": 1}},
            }
        });
        let h = upstream_health(&up);
        assert_eq!(h["peers_up"], json!(1));
        assert_eq!(h["peers_down"], json!(2));
        assert_eq!(h["fails"], json!(4));
    }

    #[test]
    fn upstream_health_handles_empty() {
        let h = upstream_health(&json!({}));
        assert_eq!(h["peers_up"], json!(0));
        assert_eq!(h["peers_down"], json!(0));
    }
}
