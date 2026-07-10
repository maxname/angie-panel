//! End-to-end tests over the real router: setup → login → me → logout,
//! including the CSRF/host middleware and cookie handling.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::state::AppState;
use crate::{api, auth, config, db};

const HOST: &str = "127.0.0.1";

/// Locks the cross-module contract between the generator (which writes the
/// MANAGED-BY header) and the apply pipeline (which parses it for
/// drift/managed detection). These were built in parallel against a written
/// spec; this test fails loudly if either side's format drifts.
#[test]
fn generator_header_roundtrips_through_apply_parser() {
    use crate::apply::header;
    use crate::generator;

    let body = "server {\n    listen 80;\n}\n";
    let wrapped = generator::with_header(body);

    // Apply's parser recognizes it as managed, with a matching hash.
    let parsed = header::parse(&wrapped).expect("apply must recognize generator output");
    assert!(parsed.hash_matches, "hash must validate across modules");

    // Generator's own parser agrees.
    let meta = generator::managed_meta(&wrapped).expect("generator parses its own header");
    assert!(meta.hash_matches);
    assert_eq!(meta.declared_hash, parsed.declared_hash);

    // A hand-edited body (drift) is detected by the apply parser.
    let tampered = wrapped.replace("listen 80;", "listen 8080;");
    let drifted = header::parse(&tampered).expect("still has our header");
    assert!(!drifted.hash_matches, "drift must be detected");

    // A foreign file is not claimed as managed.
    assert!(header::parse("server { listen 80; }\n").is_none());
}

async fn test_state(dir: &std::path::Path) -> Arc<AppState> {
    let cfg: config::PanelConfig = toml::from_str(&format!(
        "bind_addr = \"127.0.0.1\"\ndata_dir = \"{}\"",
        dir.display()
    ))
    .unwrap();
    let pool = db::connect(dir).await.unwrap();
    Arc::new(AppState::new(cfg, dir.join("test.toml"), pool))
}

fn request(method: Method, uri: &str, body: Option<Value>, cookie: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method.clone())
        .uri(uri)
        .header(header::HOST, HOST);
    if method != Method::GET {
        builder = builder.header("x-ap-request", "1");
    }
    if let Some(c) = cookie {
        builder = builder.header(header::COOKIE, c);
    }
    let mut req = match body {
        Some(v) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(v.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    // The router is normally served with connect-info; inject it for oneshot.
    req.extensions_mut()
        .insert(ConnectInfo::<SocketAddr>("127.0.0.1:9999".parse().unwrap()));
    req
}

async fn body_json(res: axum::response::Response) -> Value {
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn full_auth_flow() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    let token = auth::write_setup_token(dir.path()).unwrap();
    let app = api::router(state);

    // Fresh install: setup required, not authenticated.
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/auth/state", None, None))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    // Security headers are applied by the middleware.
    assert!(res.headers().contains_key(header::CONTENT_SECURITY_POLICY));
    let v = body_json(res).await;
    assert_eq!(v["setup_required"], json!(true));
    assert_eq!(v["authenticated"], json!(false));

    // Mutation without the CSRF marker header is rejected.
    let mut no_marker = request(Method::POST, "/api/auth/setup", Some(json!({})), None);
    no_marker.headers_mut().remove("x-ap-request");
    let res = app.clone().oneshot(no_marker).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // Wrong token is rejected.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/auth/setup",
            Some(json!({"token": "deadbeef", "email": "a@b.c", "password": "secret123"})),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // Correct token creates the admin and returns a session cookie.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/auth/setup",
            Some(json!({"token": token, "email": "Admin@Example.com", "password": "secret123"})),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let cookie = res
        .headers()
        .get(header::SET_COOKIE)
        .expect("session cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    assert!(cookie.starts_with("ap_session="));

    // Token file is consumed: setup is no longer possible.
    assert!(!dir.path().join(auth::TOKEN_FILE).exists());

    // Authenticated /me works; email was normalized to lowercase.
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/auth/me", None, Some(&cookie)))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_json(res).await["email"], json!("admin@example.com"));

    // Wrong password fails, correct one logs in.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/auth/login",
            Some(json!({"email": "admin@example.com", "password": "wrongwrong"})),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/auth/login",
            Some(json!({"email": "admin@example.com", "password": "secret123"})),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Logout invalidates the session.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/auth/logout",
            Some(json!({})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/auth/me", None, Some(&cookie)))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn host_and_api_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    let app = api::router(state);

    // Foreign Host header → 421 (DNS-rebinding defense).
    let mut req = request(Method::GET, "/api/auth/state", None, None);
    req.headers_mut()
        .insert(header::HOST, "evil.example.com".parse().unwrap());
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::MISDIRECTED_REQUEST);

    // Unknown API path → JSON 404, not the SPA.
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/nope", None, None))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert_eq!(body_json(res).await["error"]["code"], json!("not_found"));
}
