use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{header, HeaderValue, Method, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::state::AppState;

/// Custom header required on every mutating request. A cross-origin page
/// cannot set it without a CORS preflight (which we never grant), so together
/// with SameSite=Lax cookies it blocks CSRF.
pub const REQUEST_HEADER: &str = "x-ap-request";

/// Mutating endpoints any authenticated (or unauthenticated) user may call —
/// exempt from the admin role gate. Everything else requires admin.
fn is_self_service(path: &str) -> bool {
    matches!(
        path,
        "/api/auth/login" | "/api/auth/setup" | "/api/auth/logout" | "/api/users/me/password"
    )
}

fn reject(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message } })),
    )
        .into_response()
}

fn host_allowed(state: &AppState, host: &str) -> bool {
    if state.allowed_hostnames.is_empty() {
        return true; // explicitly disabled (warned at startup)
    }
    // Strip the port; handle bracketed IPv6 literals.
    let hostname = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or(rest)
    } else {
        host.rsplit_once(':')
            .map(|(h, _)| h)
            .filter(|h| !h.is_empty())
            .unwrap_or(host)
    };
    state.allowed_hostnames.contains(hostname)
}

fn origin_allowed(state: &AppState, origin: &str) -> bool {
    // Origin: scheme://host[:port] — reuse the host allowlist.
    let Some(rest) = origin.split_once("://").map(|(_, r)| r) else {
        return false;
    };
    host_allowed(state, rest)
}

pub async fn security_layer(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    // 1. Host allowlist (DNS-rebinding protection).
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !host_allowed(&state, host) {
        return reject(
            StatusCode::MISDIRECTED_REQUEST,
            "bad_host",
            "Host header not in the allowlist (see allowed_hosts in angie-panel.toml)",
        );
    }

    // 2. Mutation guard: custom header + Origin check.
    let mutating = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if mutating {
        let has_marker = req
            .headers()
            .get(REQUEST_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(|v| v == "1")
            .unwrap_or(false);
        if !has_marker {
            return reject(StatusCode::FORBIDDEN, "csrf", "missing X-AP-Request header");
        }
        if let Some(origin) = req
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok())
        {
            if origin != "null" && !origin_allowed(&state, origin) {
                return reject(StatusCode::FORBIDDEN, "csrf", "cross-origin request denied");
            }
        }

        // 2b. Role gate (central authz): every mutation requires an admin,
        //     except a small self-service allowlist. This is the single choke
        //     point — a viewer (or a future endpoint) can never mutate config
        //     even if a handler forgets to check. Unauthenticated requests fall
        //     through so the handler's AuthUser extractor returns a clean 401.
        if !is_self_service(req.uri().path()) {
            if let Some(crate::auth::Role::Viewer) =
                crate::auth::session_role(&state, req.headers()).await
            {
                return reject(
                    StatusCode::FORBIDDEN,
                    "forbidden",
                    "this action requires an administrator",
                );
            }
        }
    }

    let is_api = req.uri().path().starts_with("/api/");
    let mut res = next.run(req).await;

    // 3. Security headers. No CORS headers are ever emitted: the API is
    //    strictly same-origin (lesson from NPM's CVE-2025-50579).
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; \
             frame-ancestors 'none'; base-uri 'self'; form-action 'self'",
        ),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    if is_api {
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    res
}

/// JSON 404 for unknown /api/* paths (instead of falling back to the SPA).
pub async fn api_not_found(uri: Uri) -> Response {
    reject(
        StatusCode::NOT_FOUND,
        "not_found",
        &format!("no such endpoint: {}", uri.path()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PanelConfig;

    // The SQLite pool spawns onto the runtime even with connect_lazy, so
    // these state-building tests must run inside a Tokio context.
    fn state_with_bind(bind: &str) -> AppState {
        let cfg: PanelConfig = toml::from_str(&format!("bind_addr = \"{bind}\"")).unwrap();
        let db = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        AppState::new(cfg, "/tmp/test.toml".into(), db)
    }

    #[tokio::test]
    async fn host_allowlist() {
        let st = state_with_bind("192.168.1.5");
        assert!(host_allowed(&st, "192.168.1.5:8080"));
        assert!(host_allowed(&st, "192.168.1.5"));
        assert!(host_allowed(&st, "localhost:8080"));
        assert!(!host_allowed(&st, "evil.example.com"));
        assert!(!host_allowed(&st, "evil.example.com:8080"));
    }

    #[tokio::test]
    async fn host_check_disabled_on_unspecified_bind() {
        let st = state_with_bind("0.0.0.0");
        assert!(host_allowed(&st, "anything.example.com"));
    }

    #[tokio::test]
    async fn origin_check() {
        let st = state_with_bind("192.168.1.5");
        assert!(origin_allowed(&st, "http://192.168.1.5:8080"));
        assert!(origin_allowed(&st, "http://localhost:5173"));
        assert!(!origin_allowed(&st, "http://attacker.example"));
        assert!(!origin_allowed(&st, "garbage"));
    }
}
