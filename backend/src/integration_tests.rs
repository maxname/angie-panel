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

/// Build state with an already-created admin and return (app, session cookie).
async fn authed_app(dir: &std::path::Path) -> (axum::Router, String) {
    let http_d = dir.join("http.d");
    std::fs::create_dir_all(&http_d).unwrap();
    // status_api_url points at a dead port so tests never depend on a real
    // Angie that happens to be listening on the default 8100.
    let cfg: config::PanelConfig = toml::from_str(&format!(
        "bind_addr = \"127.0.0.1\"\ndata_dir = \"{}\"\n[angie]\nhttp_d_dir = \"{}\"\n\
         status_api_url = \"http://127.0.0.1:9/status\"",
        dir.display(),
        http_d.display()
    ))
    .unwrap();
    let pool = db::connect(dir).await.unwrap();
    let state = Arc::new(AppState::new(cfg, dir.join("test.toml"), pool));
    let token = auth::write_setup_token(dir).unwrap();
    let app = api::router(state);
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/auth/setup",
            Some(json!({"token": token, "email": "a@b.c", "password": "secret123"})),
            None,
        ))
        .await
        .unwrap();
    let cookie = res
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    (app, cookie)
}

#[tokio::test]
async fn hosts_crud_and_preview() {
    let dir = tempfile::tempdir().unwrap();
    let (app, cookie) = authed_app(dir.path()).await;

    // Reject an injection attempt in forward_host (validation, not escaping).
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/hosts",
            Some(json!({
                "domains": ["app.example.com"],
                "forward_scheme": "http",
                "forward_host": "1.2.3.4; } location /x { root /; ",
                "forward_port": 8080
            })),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // Create a valid host.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/hosts",
            Some(json!({
                "domains": ["App.Example.com"],
                "forward_scheme": "http",
                "forward_host": "192.168.1.10",
                "forward_port": 8123,
                "websockets_upgrade": true
            })),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let host = body_json(res).await;
    let id = host["id"].as_i64().unwrap();
    assert_eq!(host["domains"][0], json!("app.example.com")); // normalized

    // Duplicate domain on another enabled host → 409.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/hosts",
            Some(json!({
                "domains": ["app.example.com"],
                "forward_scheme": "http",
                "forward_host": "10.0.0.2",
                "forward_port": 80
            })),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);

    // Preview shows the new host file as Added.
    let res = app
        .clone()
        .oneshot(request(
            Method::GET,
            "/api/apply/preview",
            None,
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let preview = body_json(res).await;
    let files = preview["diff"]["files"]
        .as_array()
        .expect("diff.files array");
    let host_file = format!("20-host-{id}-app-example-com.conf");
    assert!(
        files.iter().any(|f| f["name"] == json!(host_file)),
        "preview should list {host_file}, got {files:?}"
    );

    // Disable → delete.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            &format!("/api/hosts/{id}/disable"),
            Some(json!({})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let res = app
        .clone()
        .oneshot(request(
            Method::DELETE,
            &format!("/api/hosts/{id}"),
            None,
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let res = app
        .clone()
        .oneshot(request(
            Method::GET,
            &format!("/api/hosts/{id}"),
            None,
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn export_import_roundtrip_and_rejects_injection() {
    let dir = tempfile::tempdir().unwrap();
    let (app, cookie) = authed_app(dir.path()).await;

    // Seed a cert, an access list (with a basic-auth user), a proxy host that
    // references both, a redirect host, and a stream — one of every type.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/certificates",
            Some(json!({"name":"web","domains":["app.example.com"],"challenge":"http"})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let cert_id = body_json(res).await["id"].as_i64().unwrap();

    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/access-lists",
            Some(json!({"name":"team","satisfy":"all","pass_auth":false,
                "users":[{"username":"alice","password":"s3cret-pw"}],
                "clients":[{"directive":"allow","address":"10.0.0.0/8"}]})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let acl_id = body_json(res).await["id"].as_i64().unwrap();

    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/hosts",
            Some(json!({
                "domains":["app.example.com"],"forward_scheme":"http",
                "forward_host":"192.168.1.10","forward_port":8123,
                "certificate_id":cert_id,"access_list_id":acl_id
            })),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/redirect-hosts",
            Some(json!({"domains":["old.example.com"],"forward_domain":"new.example.com"})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/streams",
            Some(json!({"incoming_port":5432,"forward_host":"192.168.1.20","forward_port":5432})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Configure a geo policy so the export must carry it and the import must
    // accept it back (regression: geo_mode/geo_countries were exported but not
    // in the import allowlist, so restoring any geo-configured backup 400'd).
    let res = app
        .clone()
        .oneshot(request(
            Method::PUT,
            "/api/geo",
            Some(json!({"mode":"deny","countries":["ru","cn"]})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Export the full config.
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/export", None, Some(&cookie)))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let export = body_json(res).await;
    assert_eq!(export["version"], json!(2));
    assert_eq!(export["hosts"].as_array().unwrap().len(), 1);
    assert_eq!(export["certificates"].as_array().unwrap().len(), 1);
    assert_eq!(export["access_lists"].as_array().unwrap().len(), 1);
    assert_eq!(export["redirect_hosts"].as_array().unwrap().len(), 1);
    assert_eq!(export["streams"].as_array().unwrap().len(), 1);
    // The access list carries the user's bcrypt hash (faithful restore).
    let hash = export["access_lists"][0]["users"][0]["password_hash"]
        .as_str()
        .unwrap();
    assert!(
        hash.starts_with("$2"),
        "expected a bcrypt hash, got {hash:?}"
    );

    // Re-import the same doc → round-trips cleanly, every type counted.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/import",
            Some(export.clone()),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let summary = body_json(res).await;
    assert_eq!(summary["imported"]["hosts"], json!(1));
    assert_eq!(summary["imported"]["certificates"], json!(1));
    assert_eq!(summary["imported"]["access_lists"], json!(1));
    assert_eq!(summary["imported"]["redirect_hosts"], json!(1));
    assert_eq!(summary["imported"]["streams"], json!(1));

    // Host survived with both references intact; the stream survived too.
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/hosts", None, Some(&cookie)))
        .await
        .unwrap();
    let hosts = body_json(res).await;
    assert_eq!(hosts["hosts"][0]["certificate_id"], json!(cert_id));
    assert_eq!(hosts["hosts"][0]["access_list_id"], json!(acl_id));
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/streams", None, Some(&cookie)))
        .await
        .unwrap();
    assert_eq!(
        body_json(res).await["streams"][0]["incoming_port"],
        json!(5432)
    );

    // The geo policy round-tripped (normalized to upper-case ISO codes).
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/geo", None, Some(&cookie)))
        .await
        .unwrap();
    let geo = body_json(res).await;
    assert_eq!(geo["mode"], json!("deny"));
    assert_eq!(geo["countries"], json!(["RU", "CN"]));

    // An import with an injected forward_host is rejected — validation runs on
    // untrusted import exactly like the API.
    let malicious = json!({
        "version": 2,
        "hosts": [{"id":1,"domains":["x.example.com"],"forward_scheme":"http",
            "forward_host":"1.2.3.4; } location /r { root /; ","forward_port":80}],
    });
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/import",
            Some(malicious),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // A host referencing a non-existent certificate is rejected.
    let dangling = json!({
        "version": 2,
        "hosts": [{"id":1,"domains":["y.example.com"],"forward_scheme":"http",
            "forward_host":"10.0.0.9","forward_port":80,"certificate_id":999}],
    });
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/import",
            Some(dangling),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // An access list whose user hash is not a real bcrypt hash is rejected
    // (it would land in an htpasswd file verbatim).
    let bad_hash = json!({
        "version": 2,
        "access_lists": [{"id":1,"name":"evil","satisfy":"all","pass_auth":false,
            "users":[{"username":"bob","password_hash":"not-a-hash\nroot:x"}],
            "clients":[]}],
    });
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/import",
            Some(bad_hash),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn host_create_rejects_dangling_cert_and_acl_refs() {
    let dir = tempfile::tempdir().unwrap();
    let (app, cookie) = authed_app(dir.path()).await;

    // A non-existent certificate_id is rejected (would emit a dangling
    // $acme_cert_* reference in the generated config).
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/hosts",
            Some(json!({"domains":["a.example.com"],"forward_scheme":"http",
                "forward_host":"10.0.0.2","forward_port":80,"certificate_id":999})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // A non-existent access_list_id is rejected too (a bad ref would silently
    // drop the intended IP/basic-auth restriction — fail-open).
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/hosts",
            Some(json!({"domains":["b.example.com"],"forward_scheme":"http",
                "forward_host":"10.0.0.3","forward_port":80,"access_list_id":999})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn redirect_host_and_cross_type_domain_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let (app, cookie) = authed_app(dir.path()).await;

    // Create a proxy host on app.example.com.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/hosts",
            Some(
                json!({"domains":["app.example.com"],"forward_scheme":"http",
                "forward_host":"10.0.0.1","forward_port":80}),
            ),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // A redirect host claiming the SAME domain → cross-type 409.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/redirect-hosts",
            Some(json!({"domains":["app.example.com"],"forward_domain":"new.example.com"})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
    assert_eq!(
        body_json(res).await["error"]["code"],
        json!("domain_conflict")
    );

    // A redirect host on a fresh domain works; injection in forward_domain is
    // rejected.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/redirect-hosts",
            Some(
                json!({"domains":["old.example.com"],"forward_domain":"new.example.com",
                "forward_http_code":302,"preserve_path":false}),
            ),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let rid = body_json(res).await["id"].as_i64().unwrap();

    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/redirect-hosts",
            Some(json!({"domains":["evil.example.com"],
                "forward_domain":"x.com; return 200 \"pwned\""})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // A 404 host works and shows up in the apply preview as a 40-* file.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/dead-hosts",
            Some(json!({"domains":["parked.example.com"]})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let res = app
        .clone()
        .oneshot(request(
            Method::GET,
            "/api/apply/preview",
            None,
            Some(&cookie),
        ))
        .await
        .unwrap();
    let files = body_json(res).await["diff"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap_or("").to_string())
        .collect::<Vec<_>>();
    assert!(
        files.iter().any(|n| n.starts_with("30-redirect-")),
        "{files:?}"
    );
    assert!(files.iter().any(|n| n.starts_with("40-dead-")), "{files:?}");

    // Delete the redirect host.
    let res = app
        .clone()
        .oneshot(request(
            Method::DELETE,
            &format!("/api/redirect-hosts/{rid}"),
            None,
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn host_rate_limit_persists_and_generates() {
    let dir = tempfile::tempdir().unwrap();
    let (app, cookie) = authed_app(dir.path()).await;

    // Create a host with a rate limit (rps + burst + nodelay + conn).
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/hosts",
            Some(json!({
                "domains":["api.example.com"],"forward_scheme":"http",
                "forward_host":"10.0.0.5","forward_port":80,
                "rate_limit":{"enabled":true,"rps":15,"burst":30,"nodelay":true,"conn":5}
            })),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let host = body_json(res).await;
    let id = host["id"].as_i64().unwrap();
    assert_eq!(host["rate_limit"]["rps"], json!(15));
    assert_eq!(host["rate_limit"]["nodelay"], json!(true));

    // It round-trips through GET.
    let res = app
        .clone()
        .oneshot(request(
            Method::GET,
            &format!("/api/hosts/{id}"),
            None,
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(body_json(res).await["rate_limit"]["conn"], json!(5));

    // Apply preview includes the rate-limit zone file.
    let res = app
        .clone()
        .oneshot(request(
            Method::GET,
            "/api/apply/preview",
            None,
            Some(&cookie),
        ))
        .await
        .unwrap();
    let files = body_json(res).await["diff"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap_or("").to_string())
        .collect::<Vec<_>>();
    assert!(
        files.iter().any(|n| n == "15-rate-limits.conf"),
        "expected the rate-limit zone file, got {files:?}"
    );

    // Enabling the limit with no actual ceiling is rejected.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/hosts",
            Some(json!({
                "domains":["bad.example.com"],"forward_scheme":"http",
                "forward_host":"10.0.0.6","forward_port":80,
                "rate_limit":{"enabled":true,"rps":0,"conn":0}
            })),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        body_json(res).await["error"]["code"],
        json!("invalid_rate_limit")
    );
}

#[tokio::test]
async fn stream_crud_port_conflict_and_preview() {
    let dir = tempfile::tempdir().unwrap();
    let (app, cookie) = authed_app(dir.path()).await;

    // Create a TCP forward on :5432.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/streams",
            Some(json!({"incoming_port":5432,"forward_host":"192.168.1.20",
                "forward_port":5432,"tcp":true,"udp":false})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let sid = body_json(res).await["id"].as_i64().unwrap();

    // Another TCP stream on the SAME port → 409 port_conflict.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/streams",
            Some(json!({"incoming_port":5432,"forward_host":"10.0.0.9","forward_port":80})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
    assert_eq!(
        body_json(res).await["error"]["code"],
        json!("port_conflict")
    );

    // Same port but UDP-only does NOT clash with the TCP stream → 200.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/streams",
            Some(json!({"incoming_port":5432,"forward_host":"10.0.0.53",
                "forward_port":53,"tcp":false,"udp":true})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // A stream with neither protocol → 400 no_protocol.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/streams",
            Some(json!({"incoming_port":6000,"forward_host":"10.0.0.9",
                "forward_port":80,"tcp":false,"udp":false})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(res).await["error"]["code"], json!("no_protocol"));

    // Injection in forward_host is rejected up front.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/streams",
            Some(json!({"incoming_port":7000,
                "forward_host":"1.2.3.4:80; } server { listen 25; ","forward_port":80})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // Apply preview shows the enabled streams as stream.d/ files.
    let res = app
        .clone()
        .oneshot(request(
            Method::GET,
            "/api/apply/preview",
            None,
            Some(&cookie),
        ))
        .await
        .unwrap();
    let files = body_json(res).await["diff"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap_or("").to_string())
        .collect::<Vec<_>>();
    assert!(
        files.iter().any(|n| n.starts_with("stream.d/stream-")),
        "expected a stream.d file in the preview, got {files:?}"
    );

    // Disable then delete the first stream.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            &format!("/api/streams/{sid}/disable"),
            None,
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let res = app
        .clone()
        .oneshot(request(
            Method::DELETE,
            &format!("/api/streams/{sid}"),
            None,
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn acme_hook_requires_a_valid_token() {
    // The ACME hook is called by Angie, not a user; it self-authenticates by a
    // token in the query string. A caller without the (or a wrong) token gets
    // 403 and triggers NO provider action — the security gate for a loopback
    // endpoint that is exempt from CSRF/role.
    let dir = tempfile::tempdir().unwrap();
    let (app, _cookie) = authed_app(dir.path()).await;

    // No token → 403.
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/acme/hook", None, None))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // Wrong token → 403 (no token has been generated yet, so anything is wrong).
    let res = app
        .clone()
        .oneshot(request(
            Method::GET,
            "/api/acme/hook?t=deadbeef",
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn dns_credential_profiles_crud() {
    let dir = tempfile::tempdir().unwrap();
    let (app, cookie) = authed_app(dir.path()).await;

    // The static type registry lists provider types (no per-type creds now).
    let res = app
        .clone()
        .oneshot(request(
            Method::GET,
            "/api/dns-providers",
            None,
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let providers = body_json(res).await["providers"]
        .as_array()
        .unwrap()
        .clone();
    assert!(providers.iter().any(|p| p["id"] == "cloudflare"));

    // TWO Cloudflare profiles can coexist (the whole point).
    let mk = |name: &str, token: &str| json!({"provider":"cloudflare","name":name,"credentials":{"CF_Token":token}});
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/dns-credentials",
            Some(mk("CF personal", "tok1")),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let p1 = body_json(res).await;
    assert_eq!(p1["configured"], json!(true));
    assert_eq!(p1["provider_label"], json!("Cloudflare"));
    let id1 = p1["id"].as_i64().unwrap();

    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/dns-credentials",
            Some(mk("CF work", "tok2")),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Both appear in the list.
    let res = app
        .clone()
        .oneshot(request(
            Method::GET,
            "/api/dns-credentials",
            None,
            Some(&cookie),
        ))
        .await
        .unwrap();
    let creds = body_json(res).await["credentials"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(
        creds
            .iter()
            .filter(|c| c["provider"] == "cloudflare")
            .count(),
        2
    );

    // Unknown provider type → 400; unknown field → 400.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/dns-credentials",
            Some(json!({"provider":"nope","name":"x","credentials":{}})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/dns-credentials",
            Some(json!({"provider":"cloudflare","name":"x","credentials":{"BOGUS":"y"}})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // A cert may reference a profile; then deleting it is blocked (409).
    let res = app
        .clone()
        .oneshot(request(Method::POST, "/api/certificates",
            Some(json!({"name":"wild","domains":["*.example.com"],"challenge":"dns","dns_provider":id1.to_string()})),
            Some(&cookie)))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let res = app
        .clone()
        .oneshot(request(
            Method::DELETE,
            &format!("/api/dns-credentials/{id1}"),
            None,
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);

    // A cert referencing a non-existent profile is rejected.
    let res = app
        .clone()
        .oneshot(request(Method::POST, "/api/certificates",
            Some(json!({"name":"wild2","domains":["*.other.com"],"challenge":"dns","dns_provider":"9999"})),
            Some(&cookie)))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // Credentials never leak via the settings GET.
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/settings", None, Some(&cookie)))
        .await
        .unwrap();
    let raw = body_json(res).await["raw"].as_object().unwrap().clone();
    assert!(!raw.keys().any(|k| k.starts_with("dns_cred:")));
}

/// Editing a certificate updates it in place (same id) so a host that references
/// it stays bound — even though the cert is in use and could not be deleted.
#[tokio::test]
async fn certificate_edit_keeps_host_binding() {
    let dir = tempfile::tempdir().unwrap();
    let (app, cookie) = authed_app(dir.path()).await;

    // A cert, and a host that references it.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/certificates",
            Some(json!({"name":"web","domains":["app.example.com"],"challenge":"http"})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let cert_id = body_json(res).await["id"].as_i64().unwrap();

    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/hosts",
            Some(json!({
                "domains":["app.example.com"],"forward_scheme":"http",
                "forward_host":"192.168.1.10","forward_port":8123,
                "certificate_id":cert_id
            })),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Rename it and change the SAN set — an in-place edit, not delete+create.
    let res = app
        .clone()
        .oneshot(request(
            Method::PUT,
            &format!("/api/certificates/{cert_id}"),
            Some(
                json!({"name":"web_v2","domains":["app.example.com","www.example.com"],
                "challenge":"http","key_type":"rsa"}),
            ),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let updated = body_json(res).await;
    assert_eq!(updated["name"], json!("web_v2"));
    assert_eq!(updated["key_type"], json!("rsa"));
    assert_eq!(updated["domains"].as_array().unwrap().len(), 2);

    // The host still points at the same cert id — the binding survived the edit.
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/hosts", None, Some(&cookie)))
        .await
        .unwrap();
    let hosts = body_json(res).await;
    assert_eq!(hosts["hosts"][0]["certificate_id"], json!(cert_id));

    // A second cert; renaming the first onto its name is a 409 clash.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/certificates",
            Some(json!({"name":"other","domains":["b.example.com"],"challenge":"http"})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let res = app
        .clone()
        .oneshot(request(
            Method::PUT,
            &format!("/api/certificates/{cert_id}"),
            Some(json!({"name":"other","domains":["app.example.com"],"challenge":"http"})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);

    // Editing to reference a non-existent DNS profile is rejected.
    let res = app
        .clone()
        .oneshot(request(
            Method::PUT,
            &format!("/api/certificates/{cert_id}"),
            Some(json!({"name":"web_v2","domains":["*.example.com"],
                "challenge":"dns","dns_provider":"9999"})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // Editing a cert that does not exist is a 404.
    let res = app
        .clone()
        .oneshot(request(
            Method::PUT,
            "/api/certificates/99999",
            Some(json!({"name":"ghost","domains":["g.example.com"],"challenge":"http"})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

/// NPM-style: creating a certificate without a name derives a unique one from
/// the first domain (the name is only the acme_client id).
#[tokio::test]
async fn certificate_name_auto_generated_from_domain() {
    let dir = tempfile::tempdir().unwrap();
    let (app, cookie) = authed_app(dir.path()).await;

    let create = |body: Value| {
        app.clone().oneshot(request(
            Method::POST,
            "/api/certificates",
            Some(body),
            Some(&cookie),
        ))
    };

    // No name → slug of the first domain.
    let res = create(json!({"domains": ["app.example.com"], "challenge": "http"}))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_json(res).await["name"], json!("app_example_com"));

    // Same domain again → the slug is taken, so it gets a numeric suffix.
    let res = create(json!({"domains": ["app.example.com"], "challenge": "http"}))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_json(res).await["name"], json!("app_example_com_2"));

    // Wildcard: the leading "*." is stripped in the slug.
    let res = create(json!({"domains": ["*.example.com"], "challenge": "dns"}))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_json(res).await["name"], json!("example_com"));
}

#[tokio::test]
async fn sni_router_crud_conflict_and_preview() {
    let dir = tempfile::tempdir().unwrap();
    let (app, cookie) = authed_app(dir.path()).await;

    // Create a router on :443 with two routes and a catch-all.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/sni-routers",
            Some(json!({
                "name": "edge",
                "incoming_port": 443,
                "routes": [
                    {"sni":"app.example.com","forward_host":"10.0.0.10","forward_port":443},
                    {"sni":"*.internal.example.com","forward_host":"10.0.0.20","forward_port":8443}
                ],
                "default_host":"10.0.0.1","default_port":443
            })),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let rid = body_json(res).await["id"].as_i64().unwrap();

    // Another router on the SAME port → 409 port_conflict.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/sni-routers",
            Some(json!({"name":"dup","incoming_port":443,
                "routes":[{"sni":"x.example.com","forward_host":"10.0.0.9","forward_port":443}]})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
    assert_eq!(
        body_json(res).await["error"]["code"],
        json!("port_conflict")
    );

    // A TCP stream on the router's port also conflicts (shared stream context).
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/streams",
            Some(json!({"incoming_port":443,"forward_host":"10.0.0.9","forward_port":80})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);

    // A single-label SNI (would collide with a map keyword) → 400.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/sni-routers",
            Some(json!({"name":"bad","incoming_port":8443,
                "routes":[{"sni":"default","forward_host":"10.0.0.9","forward_port":443}]})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // Apply preview emits the router as a stream.d/ file.
    let res = app
        .clone()
        .oneshot(request(
            Method::GET,
            "/api/apply/preview",
            None,
            Some(&cookie),
        ))
        .await
        .unwrap();
    let files = body_json(res).await["diff"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap_or("").to_string())
        .collect::<Vec<_>>();
    assert!(
        files.iter().any(|n| n == "stream.d/sni-1.conf"),
        "expected stream.d/sni-1.conf in the preview, got {files:?}"
    );

    // Delete the router.
    let res = app
        .clone()
        .oneshot(request(
            Method::DELETE,
            &format!("/api/sni-routers/{rid}"),
            None,
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn bans_crud_and_preview() {
    let dir = tempfile::tempdir().unwrap();
    let (app, cookie) = authed_app(dir.path()).await;

    // Ban an IP and a CIDR.
    for addr in ["203.0.113.7", "198.51.100.0/24"] {
        let res = app
            .clone()
            .oneshot(request(
                Method::POST,
                "/api/bans",
                Some(json!({ "address": addr, "reason": "brute force" })),
                Some(&cookie),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "banning {addr}");
    }

    // Duplicate address → 409.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/bans",
            Some(json!({ "address": "203.0.113.7" })),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);

    // 'all' and garbage are rejected.
    for bad in ["all", "not-an-ip", "1.2.3.4; deny 0.0.0.0/0"] {
        let res = app
            .clone()
            .oneshot(request(
                Method::POST,
                "/api/bans",
                Some(json!({ "address": bad })),
                Some(&cookie),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "rejecting {bad:?}");
    }

    // Apply preview shows the global blocklist file.
    let res = app
        .clone()
        .oneshot(request(
            Method::GET,
            "/api/apply/preview",
            None,
            Some(&cookie),
        ))
        .await
        .unwrap();
    let files = body_json(res).await["diff"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap_or("").to_string())
        .collect::<Vec<_>>();
    assert!(
        files.iter().any(|n| n == "03-bans.conf"),
        "expected the blocklist file, got {files:?}"
    );

    // List, then delete one.
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/bans", None, Some(&cookie)))
        .await
        .unwrap();
    let bans = body_json(res).await;
    let arr = bans["bans"].as_array().unwrap();
    assert_eq!(arr.len(), 2);
    let id = arr[0]["id"].as_i64().unwrap();
    let res = app
        .clone()
        .oneshot(request(
            Method::DELETE,
            &format!("/api/bans/{id}"),
            None,
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn dashboard_degrades_without_angie() {
    let dir = tempfile::tempdir().unwrap();
    let (app, cookie) = authed_app(dir.path()).await;

    // No Angie status API reachable in tests → dashboard must still return 200
    // with angie.up=false and an angie_down alert, never error.
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/dashboard", None, Some(&cookie)))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["angie"]["up"], json!(false));
    assert!(v["alerts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["code"] == json!("angie_down")));
    // Unauthenticated access is rejected.
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/dashboard", None, None))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
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

/// Extract the `ap_session=...` cookie from a Set-Cookie response.
fn session_cookie(res: &axum::response::Response) -> String {
    res.headers()
        .get(header::SET_COOKIE)
        .expect("session cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn roles_gate_mutations_and_protect_last_admin() {
    let dir = tempfile::tempdir().unwrap();
    let (app, admin) = authed_app(dir.path()).await;

    // Admin sees a role of "admin".
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/auth/me", None, Some(&admin)))
        .await
        .unwrap();
    assert_eq!(body_json(res).await["role"], json!("admin"));

    // Admin creates a viewer.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/users",
            Some(json!({"email":"viewer@example.com","password":"viewerpass","role":"viewer"})),
            Some(&admin),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let viewer_id = body_json(res).await["id"].as_i64().unwrap();

    // Log in as the viewer.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/auth/login",
            Some(json!({"email":"viewer@example.com","password":"viewerpass"})),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let viewer = session_cookie(&res);

    // Viewer can READ hosts…
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/hosts", None, Some(&viewer)))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // …but every mutation is rejected by the central role gate (403).
    for (method, path, body) in [
        (
            Method::POST,
            "/api/hosts",
            Some(
                json!({"domains":["v.example.com"],"forward_scheme":"http","forward_host":"10.0.0.1","forward_port":80}),
            ),
        ),
        (
            Method::POST,
            "/api/streams",
            Some(json!({"incoming_port":9,"forward_host":"10.0.0.1","forward_port":9})),
        ),
        (Method::POST, "/api/apply", Some(json!({}))),
        (
            Method::POST,
            "/api/users",
            Some(json!({"email":"x@y.z","password":"password1","role":"viewer"})),
        ),
    ] {
        let res = app
            .clone()
            .oneshot(request(method, path, body, Some(&viewer)))
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            StatusCode::FORBIDDEN,
            "{path} must be admin-only"
        );
    }

    // Viewer cannot list users (admin-only handler check), even via GET.
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/users", None, Some(&viewer)))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // Viewer cannot export: it carries secrets (access-list password hashes) and
    // GET is not covered by the method-based gate, so the handler guards it.
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/export", None, Some(&viewer)))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    // Admin still can.
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/export", None, Some(&admin)))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Viewer CAN change their own password (self-service allowlist).
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/users/me/password",
            Some(json!({"current_password":"viewerpass","new_password":"newviewerpass"})),
            Some(&viewer),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Admin can list both users.
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/users", None, Some(&admin)))
        .await
        .unwrap();
    assert_eq!(body_json(res).await["users"].as_array().unwrap().len(), 2);

    // Last-admin protection: the admin cannot demote themselves (only admin).
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/users", None, Some(&admin)))
        .await
        .unwrap();
    let admin_id = body_json(res).await["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["role"] == json!("admin"))
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let res = app
        .clone()
        .oneshot(request(
            Method::PUT,
            &format!("/api/users/{admin_id}/role"),
            Some(json!({"role":"viewer"})),
            Some(&admin),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(res).await["error"]["code"], json!("last_admin"));

    // Admin cannot delete their own account.
    let res = app
        .clone()
        .oneshot(request(
            Method::DELETE,
            &format!("/api/users/{admin_id}"),
            None,
            Some(&admin),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        body_json(res).await["error"]["code"],
        json!("cannot_delete_self")
    );

    // Admin CAN delete the viewer.
    let res = app
        .clone()
        .oneshot(request(
            Method::DELETE,
            &format!("/api/users/{viewer_id}"),
            None,
            Some(&admin),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
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

#[tokio::test]
async fn audit_log_records_mutations_and_is_admin_only() {
    let dir = tempfile::tempdir().unwrap();
    let (app, cookie) = authed_app(dir.path()).await;

    // A mutation to be audited.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/hosts",
            Some(json!({
                "domains": ["app.example.com"],
                "forward_scheme": "http",
                "forward_host": "10.0.0.5",
                "forward_port": 8080
            })),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // The admin sees the audit trail; the newest entry is the host creation.
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/audit", None, Some(&cookie)))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let entries = body_json(res).await;
    let first = &entries["entries"][0];
    assert_eq!(first["method"], json!("POST"));
    assert_eq!(first["path"], json!("/api/hosts"));
    assert_eq!(first["status"], json!(200));
    assert_eq!(first["user_email"], json!("a@b.c"));
    // The setup call earlier was also audited (its session had no user yet).
    let all = entries["entries"].as_array().unwrap();
    assert!(all.iter().any(|e| e["path"] == json!("/api/auth/setup")));

    // GET is not covered by the mutation role gate, so the handler enforces
    // admin: an unauthenticated caller is rejected.
    let res = app
        .clone()
        .oneshot(request(Method::GET, "/api/audit", None, None))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn dns_credentials_are_sealed_at_rest() {
    let dir = tempfile::tempdir().unwrap();
    let (app, cookie) = authed_app(dir.path()).await;

    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/dns-credentials",
            Some(json!({"provider":"cloudflare","name":"CF",
                "credentials":{"CF_Token":"super-secret-token"}})),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let id = body_json(res).await["id"].as_i64().unwrap();

    // The whole point: a database read on its own must not yield the token.
    let pool = crate::db::connect(dir.path()).await.unwrap();
    let stored = crate::repo::get_setting(&pool, &format!("dns_cred:{id}:CF_Token"))
        .await
        .unwrap()
        .expect("credential row");
    assert!(
        crate::secretbox::is_sealed(&stored),
        "stored credential must be sealed, got {stored}"
    );
    assert!(!stored.contains("super-secret-token"));

    // …and it still opens back to the plaintext the hook needs.
    let key = crate::secretbox::load_or_create_key(dir.path()).unwrap();
    assert_eq!(
        crate::secretbox::open(&key, &stored).unwrap(),
        "super-secret-token"
    );
}

#[tokio::test]
async fn legacy_plaintext_credentials_are_sealed_on_startup() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    // A pre-encryption install: the token sits in the clear.
    crate::repo::set_setting(&state.db, "dns_cred:7:CF_Token", "legacy-token")
        .await
        .unwrap();

    let sealed = crate::acme_hook::seal_legacy_credentials(&state)
        .await
        .unwrap();
    assert_eq!(sealed, 1);

    let stored = crate::repo::get_setting(&state.db, "dns_cred:7:CF_Token")
        .await
        .unwrap()
        .unwrap();
    assert!(crate::secretbox::is_sealed(&stored));
    assert!(!stored.contains("legacy-token"));
    let key = crate::secretbox::load_or_create_key(dir.path()).unwrap();
    assert_eq!(
        crate::secretbox::open(&key, &stored).unwrap(),
        "legacy-token"
    );

    // Idempotent — a second startup seals nothing.
    assert_eq!(
        crate::acme_hook::seal_legacy_credentials(&state)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn hosts_revision_tracks_sni_router_edits() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    // Fresh DB: no host-like rows, so the revision is 0.
    assert_eq!(crate::repo::hosts_revision(&state.db).await.unwrap(), 0);
    // An SNI router is a user-editable entity; its edits must move the revision
    // so the reconciler doesn't treat them as "no pending user changes" and
    // silently auto-apply them (audit finding).
    let input = crate::model::SniRouterInput {
        name: "r1".into(),
        incoming_port: 443,
        routes: vec![crate::model::SniRoute {
            sni: "app.example.com".into(),
            forward_host: "10.0.0.2".into(),
            forward_port: 443,
        }],
        default_host: String::new(),
        default_port: 0,
        enabled: true,
    };
    crate::repo::insert_sni_router(&state.db, &input)
        .await
        .unwrap();
    assert!(crate::repo::hosts_revision(&state.db).await.unwrap() > 0);
}

#[tokio::test]
async fn host_health_checks_persist() {
    let dir = tempfile::tempdir().unwrap();
    let (app, cookie) = authed_app(dir.path()).await;

    // A host carrying both kinds: one overriding the app defaults, one
    // inheriting them. The nulls are the point — they must survive the round
    // trip as nulls, not be frozen into today's default, or raising the default
    // later would move nobody.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/hosts",
            Some(json!({
                "domains":["mon.example.com"],"forward_scheme":"http",
                "forward_host":"10.0.0.5","forward_port":8080,
                "health_checks":[
                    {"kind":"tcp","enabled":true,"interval_secs":30,"timeout_secs":5,"port":9000},
                    {"kind":"http","enabled":true,"path":"/healthz","expected_status":[200,204],
                     "keyword":"ok","keyword_absent":false}
                ]
            })),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let host = body_json(res).await;
    let id = host["id"].as_i64().unwrap();

    // Read it back rather than trusting the create response.
    let res = app
        .clone()
        .oneshot(request(
            Method::GET,
            &format!("/api/hosts/{id}"),
            None,
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let host = body_json(res).await;
    let checks = host["health_checks"].as_array().unwrap();
    assert_eq!(checks.len(), 2);

    assert_eq!(checks[0]["kind"], json!("tcp"));
    assert_eq!(checks[0]["interval_secs"], json!(30));
    assert_eq!(checks[0]["port"], json!(9000));

    assert_eq!(checks[1]["kind"], json!("http"));
    assert_eq!(checks[1]["path"], json!("/healthz"));
    assert_eq!(checks[1]["expected_status"], json!([200, 204]));
    assert_eq!(checks[1]["keyword"], json!("ok"));
    // Inherits: never written, so it must come back null.
    assert_eq!(checks[1]["interval_secs"], json!(null));
    assert_eq!(checks[1]["timeout_secs"], json!(null));

    // A host that was never given checks has none — not a default probe.
    let res = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/hosts",
            Some(json!({
                "domains":["quiet.example.com"],"forward_scheme":"http",
                "forward_host":"10.0.0.6","forward_port":80
            })),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_json(res).await["health_checks"], json!([]));
}
