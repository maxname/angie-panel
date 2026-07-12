//! Golden + property tests for the generator. The golden files live under
//! `tests/golden/` and are asserted byte-for-byte; regenerate them with
//! `UPDATE_GOLDEN=1 cargo test -p angie-panel generator::tests` after an
//! intentional template change, then review the diff.

use std::path::PathBuf;

use super::*;
use crate::model::{
    BalanceMethod, CustomHeader, CustomLocation, DeadHost, ForwardAuth, GeoMode, GeoPolicy, Gzip,
    HeaderDirection, Maintenance, Mtls, ProxyHost, RateLimit, RedirectHost, RedirectScheme, Scheme,
    Stream, StreamTls, Upstream, UpstreamServer,
};

// --------------------------------------------------------------- fixtures

fn snippets_dir() -> PathBuf {
    PathBuf::from("/usr/share/angie-panel/snippets")
}

fn public_dir() -> PathBuf {
    PathBuf::from("/var/lib/angie-panel/public")
}

fn settings(default_site: DefaultSite, ipv6: bool) -> EffectiveSettings {
    EffectiveSettings {
        default_site,
        ipv6_enabled: ipv6,
        resolvers: vec!["127.0.0.53".into()],
    }
}

/// A minimal, all-toggles-off host. Callers mutate fields they care about.
fn base_host(id: i64, domain: &str) -> ProxyHost {
    ProxyHost {
        id,
        domains: vec![domain.to_string()],
        forward_scheme: Scheme::Http,
        forward_host: "192.168.1.10".into(),
        forward_port: 8080,
        websockets_upgrade: false,
        block_exploits: false,
        cache_assets: false,
        http2: true,
        http3: false,
        force_ssl: false,
        hsts: false,
        hsts_subdomains: false,
        trust_forwarded_proto: false,
        certificate_id: None,
        access_list_id: None,
        locations: vec![],
        advanced_snippet: None,
        rate_limit: RateLimit::default(),
        upstream: Upstream::default(),
        mtls: Mtls::default(),
        forward_auth: ForwardAuth::default(),
        custom_headers: vec![],
        maintenance: Maintenance::default(),
        gzip: Gzip::default(),
        enabled: true,
        created_at: 0,
        updated_at: 0,
    }
}

fn input(
    hosts: Vec<ProxyHost>,
    certs: Vec<Certificate>,
    settings: EffectiveSettings,
) -> GeneratorInput {
    input_acl(hosts, certs, settings, vec![])
}

fn input_acl(
    hosts: Vec<ProxyHost>,
    certs: Vec<Certificate>,
    settings: EffectiveSettings,
    access_lists: Vec<AccessList>,
) -> GeneratorInput {
    GeneratorInput {
        hosts,
        settings,
        snippets_dir: snippets_dir(),
        status_port: 8100,
        public_dir: public_dir(),
        certificates: certs,
        acme_socket_dir: PathBuf::from("/run/angie-panel"),
        access_lists,
        http_d_dir: PathBuf::from("/etc/angie/http.d"),
        redirect_hosts: vec![],
        dead_hosts: vec![],
        streams: vec![],
        bans: vec![],
        geo_policy: GeoPolicy::default(),
        geo_cidrs: vec![],
    }
}

/// A TCP-only stream forward. Callers flip tcp/udp/enabled as needed.
fn base_stream(id: i64, incoming_port: u16, forward_host: &str, forward_port: u16) -> Stream {
    Stream {
        id,
        incoming_port,
        forward_host: forward_host.into(),
        forward_port,
        tcp: true,
        udp: false,
        tls: StreamTls::None,
        certificate_id: None,
        enabled: true,
        created_at: 0,
        updated_at: 0,
    }
}

/// A ready ECDSA http-01 certificate named `name` covering `domains`.
fn ready_cert(id: i64, name: &str, domains: &[&str]) -> Certificate {
    Certificate {
        id,
        name: name.into(),
        domains: domains.iter().map(|s| s.to_string()).collect(),
        challenge: "http".into(),
        key_type: "ecdsa".into(),
        email: None,
        staging: false,
        enabled: true,
        ready: true,
    }
}

/// Assert a single generated file matches its committed golden. Set
/// UPDATE_GOLDEN=1 to (re)write the golden instead of asserting.
#[track_caller]
fn assert_golden(name: &str, actual: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden {}: {e} (run with UPDATE_GOLDEN=1)",
            path.display()
        )
    });
    assert_eq!(
        actual,
        expected,
        "generated output for {name} does not match golden {}",
        path.display()
    );
}

fn only_host_file(files: &FileSet) -> (&String, &String) {
    files
        .iter()
        .find(|(k, _)| k.starts_with("20-host-"))
        .expect("expected exactly one host file")
}

// --------------------------------------------------------------- golden tests

#[test]
fn golden_00_panel() {
    let files = generate(&input(
        vec![],
        vec![],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    assert_golden("00-panel.conf", &files["00-panel.conf"]);
}

#[test]
fn golden_10_acme_empty() {
    let files = generate(&input(
        vec![],
        vec![],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    assert_golden("10-acme.conf", &files["10-acme.conf"]);
}

#[test]
fn golden_10_acme_clients() {
    // http-01 (default), dns-01 wildcard (staging), and a paused rsa cert —
    // exercises directory URL, challenge/key_type params, enabled=off,
    // acme_dns_port, and the unix-socket collector blocks.
    let http = ready_cert(1, "web", &["app.example.com", "www.example.com"]);
    let mut wild = ready_cert(2, "wild", &["*.example.com", "example.com"]);
    wild.challenge = "dns".into();
    wild.staging = true;
    wild.email = Some("admin@example.com".into());
    let mut paused = ready_cert(3, "legacy", &["old.example.com"]);
    paused.key_type = "rsa".into();
    paused.enabled = false;

    let files = generate(&input(
        vec![],
        vec![http, wild, paused],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    assert_golden("10-acme-clients.conf", &files["10-acme.conf"]);
}

#[test]
fn golden_default_site_variants() {
    let cases = [
        (DefaultSite::NotFound, "05-default-notfound.conf"),
        (DefaultSite::Drop444, "05-default-drop444.conf"),
        (
            DefaultSite::Redirect("https://example.com/".into()),
            "05-default-redirect.conf",
        ),
        (DefaultSite::Html, "05-default-html.conf"),
    ];
    for (site, golden) in cases {
        let files = generate(&input(vec![], vec![], settings(site, false))).unwrap();
        assert_golden(golden, &files["05-default.conf"]);
    }
}

#[test]
fn golden_default_site_ipv6() {
    // The ipv6 flag adds [::]:80 / [::]:443 listen lines.
    let files = generate(&input(
        vec![],
        vec![],
        settings(DefaultSite::NotFound, true),
    ))
    .unwrap();
    assert_golden("05-default-ipv6.conf", &files["05-default.conf"]);
}

#[test]
fn golden_plain_http_host() {
    let host = base_host(1, "app.example.com");
    let files = generate(&input(
        vec![host],
        vec![],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    let (name, body) = only_host_file(&files);
    assert_eq!(name, "20-host-1-app-example-com.conf");
    assert_golden("20-host-plain-http.conf", body);
}

#[test]
fn golden_host_load_balanced() {
    // Primary + two extra servers, least_conn, weights, a backup, a down, and
    // tuned passive health (max_fails/fail_timeout on every peer).
    let mut host = base_host(4, "lb.example.com");
    host.upstream = Upstream {
        method: BalanceMethod::LeastConn,
        primary_weight: 3,
        max_fails: 2,
        fail_timeout_secs: 20,
        servers: vec![
            UpstreamServer {
                host: "10.0.0.2".into(),
                port: 8080,
                weight: 1,
                backup: false,
                down: false,
            },
            UpstreamServer {
                host: "10.0.0.3".into(),
                port: 8080,
                weight: 1,
                backup: true,
                down: false,
            },
        ],
    };
    let files = generate(&input(
        vec![host],
        vec![],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    let (_, body) = only_host_file(&files);
    assert_golden("20-host-load-balanced.conf", body);
}

#[test]
fn plain_host_upstream_unchanged() {
    // A default upstream must emit exactly the classic single-server block.
    let files = generate(&input(
        vec![base_host(9, "plain.example.com")],
        vec![],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    let (_, body) = only_host_file(&files);
    assert!(
        body.contains("upstream host_9 {\n    zone host_9 64k;\n    server 192.168.1.10:8080;\n}")
    );
    assert!(!body.contains("least_conn"));
    assert!(!body.contains("max_fails"));
}

#[test]
fn golden_host_rate_limited() {
    // Requests/sec + burst + nodelay AND a per-IP connection cap.
    let mut host = base_host(2, "api.example.com");
    host.rate_limit = RateLimit {
        enabled: true,
        rps: 10,
        burst: 20,
        nodelay: true,
        conn: 5,
    };
    let files = generate(&input(
        vec![host],
        vec![],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    // The zone file is emitted with both zones.
    assert_golden("15-rate-limits.conf", &files["15-rate-limits.conf"]);
    // The host server block carries the limit_req/limit_conn directives.
    let (_, body) = only_host_file(&files);
    assert_golden("20-host-rate-limited.conf", body);
}

#[test]
fn rate_limit_zone_omitted_when_inactive() {
    // Enabled but all-zero → validator would reject, but a stored zeroed config
    // (or a disabled host) must emit no zone file and no directives.
    let mut host = base_host(3, "plain.example.com");
    host.rate_limit = RateLimit {
        enabled: false,
        rps: 10,
        burst: 5,
        nodelay: false,
        conn: 0,
    };
    let files = generate(&input(
        vec![host],
        vec![],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    assert!(!files.contains_key("15-rate-limits.conf"));
    let (_, body) = only_host_file(&files);
    assert!(!body.contains("limit_req"), "no directives when disabled");
    assert!(!body.contains("limit_conn"), "no directives when disabled");
}

#[test]
fn golden_bans() {
    use crate::model::Ban;
    let mut inp = input(vec![], vec![], settings(DefaultSite::NotFound, false));
    inp.bans = vec![
        Ban {
            id: 1,
            address: "203.0.113.7".into(),
            reason: Some("brute force".into()),
            created_at: 0,
        },
        Ban {
            id: 2,
            address: "198.51.100.0/24".into(),
            reason: None,
            created_at: 0,
        },
        Ban {
            id: 3,
            address: "2001:db8::/32".into(),
            reason: None,
            created_at: 0,
        },
    ];
    let files = generate(&inp).unwrap();
    assert_golden("03-bans.conf", &files["03-bans.conf"]);
}

#[test]
fn no_bans_file_when_empty() {
    let files = generate(&input(
        vec![],
        vec![],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    assert!(!files.contains_key("03-bans.conf"));
}

#[test]
fn golden_streams_tcp_udp() {
    let mut inp = input(vec![], vec![], settings(DefaultSite::NotFound, false));
    inp.streams = vec![
        // TCP-only forward (e.g. Postgres).
        base_stream(1, 5432, "192.168.1.20", 5432),
        // UDP-only forward (e.g. DNS).
        {
            let mut s = base_stream(2, 5353, "10.0.0.53", 53);
            s.tcp = false;
            s.udp = true;
            s
        },
        // Both protocols on one port to a hostname upstream.
        {
            let mut s = base_stream(3, 8443, "nas.lan", 443);
            s.udp = true;
            s
        },
        // Disabled — must NOT be emitted.
        {
            let mut s = base_stream(4, 9999, "192.168.1.99", 9999);
            s.enabled = false;
            s
        },
    ];
    let files = generate(&inp).unwrap();
    assert!(
        !files.contains_key("stream.d/stream-4.conf"),
        "disabled stream must not be emitted"
    );
    assert_golden("stream-1-tcp.conf", &files["stream.d/stream-1.conf"]);
    assert_golden("stream-2-udp.conf", &files["stream.d/stream-2.conf"]);
    assert_golden("stream-3-tcp-udp.conf", &files["stream.d/stream-3.conf"]);
}

#[test]
fn golden_stream_tls_terminate() {
    // A TLS-terminating stream decrypts on its port with a panel cert and
    // forwards plaintext. The cert need not be `ready` — the `$acme_cert_<name>`
    // variable is lazy — so an unready cert still emits the ssl listener.
    let cert = ready_cert(7, "streamcert", &["db.example.com"]);
    let mut inp = input(vec![], vec![cert], settings(DefaultSite::NotFound, false));
    inp.streams = vec![{
        let mut s = base_stream(1, 5432, "192.168.1.20", 5432);
        s.tls = StreamTls::Terminate;
        s.certificate_id = Some(7);
        s
    }];
    let files = generate(&inp).unwrap();
    assert_golden("stream-1-tls.conf", &files["stream.d/stream-1.conf"]);
}

#[test]
fn stream_tls_skipped_when_cert_missing() {
    // Defensive: a terminate stream whose cert reference dangles is skipped
    // entirely, never downgraded to a plaintext forward.
    let mut inp = input(vec![], vec![], settings(DefaultSite::NotFound, false));
    inp.streams = vec![{
        let mut s = base_stream(1, 5432, "192.168.1.20", 5432);
        s.tls = StreamTls::Terminate;
        s.certificate_id = Some(999);
        s
    }];
    let files = generate(&inp).unwrap();
    assert!(
        !files.contains_key("stream.d/stream-1.conf"),
        "terminate stream with a missing cert must be skipped, not emitted"
    );
}

#[test]
fn golden_https_host_with_cert() {
    let mut host = base_host(7, "secure.example.com");
    host.certificate_id = Some(42);
    host.force_ssl = true;
    host.http2 = true;
    let cert = ready_cert(42, "secure", &["secure.example.com"]);
    let files = generate(&input(
        vec![host],
        vec![cert],
        settings(DefaultSite::NotFound, true),
    ))
    .unwrap();
    let (name, body) = only_host_file(&files);
    assert_eq!(name, "20-host-7-secure-example-com.conf");
    assert_golden("20-host-https.conf", body);
}

#[test]
fn golden_https_host_http3() {
    // HTTP/3 adds quic listeners (v4 + v6), `http3 on;`, and the Alt-Svc header
    // alongside the normal TLS listener. Verified valid by angie -t on real Angie.
    let mut host = base_host(8, "quic.example.com");
    host.certificate_id = Some(1);
    host.force_ssl = true;
    host.http2 = true;
    host.http3 = true;
    let cert = ready_cert(1, "quic", &["quic.example.com"]);
    let files = generate(&input(
        vec![host],
        vec![cert],
        settings(DefaultSite::NotFound, true),
    ))
    .unwrap();
    let (_, body) = only_host_file(&files);
    assert_golden("20-host-http3.conf", body);
}

#[test]
fn golden_https_host_mtls() {
    // Client-cert verification: the :443 block gets ssl_client_certificate (a
    // RELATIVE http.d path) + ssl_verify_client, and the CA is materialized as
    // a managed file. Verified valid by angie -t on real Angie.
    let mut host = base_host(11, "mtls.example.com");
    host.certificate_id = Some(1);
    host.mtls = Mtls {
        ca_pem: Some(
            "-----BEGIN CERTIFICATE-----\nMIIBdummyCAdata==\n-----END CERTIFICATE-----".into(),
        ),
        optional: false,
    };
    let cert = ready_cert(1, "mtls", &["mtls.example.com"]);
    let files = generate(&input(
        vec![host],
        vec![cert],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    // The CA bundle is emitted as a managed file.
    assert!(files["client-ca-host-11.pem"].contains("BEGIN CERTIFICATE"));
    let (_, body) = only_host_file(&files);
    assert_golden("20-host-mtls.conf", body);
}

#[test]
fn mtls_only_over_tls_and_omitted_when_inactive() {
    // A plain-HTTP host (no ready cert) never emits client-cert directives or
    // the CA file — mTLS is TLS-only.
    let mut host = base_host(12, "plain.example.com");
    host.mtls = Mtls {
        ca_pem: Some("-----BEGIN CERTIFICATE-----\nx\n-----END CERTIFICATE-----".into()),
        optional: false,
    };
    let files = generate(&input(
        vec![host],
        vec![],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    assert!(!files.contains_key("client-ca-host-12.pem"));
    let (_, body) = only_host_file(&files);
    assert!(!body.contains("ssl_verify_client"));
}

#[test]
fn golden_forward_auth() {
    // Forward auth: the internal verify location, a 401 → sign-in redirect, and
    // per-location auth_request + identity-header copy. Verified by angie -t
    // (and runtime: deny→401/redirect, allow→backend) on real Angie.
    let mut host = base_host(14, "app.example.com");
    host.forward_auth = ForwardAuth {
        enabled: true,
        verify_url: "http://10.0.0.9:9091/api/verify".into(),
        sign_in_url: Some("https://auth.example.com".into()),
        copy_headers: vec!["Remote-User".into(), "Remote-Groups".into()],
    };
    let files = generate(&input(
        vec![host],
        vec![],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    let (_, body) = only_host_file(&files);
    assert_golden("20-host-forward-auth.conf", body);
}

#[test]
fn golden_gzip() {
    // gzip on + gzip_proxied any (so proxied responses compress) + tuning +
    // a custom type list. Verified by angie -t (and a live gzipped response)
    // on real Angie.
    let mut host = base_host(21, "app.example.com");
    host.gzip = Gzip {
        enabled: true,
        comp_level: 6,
        min_length: 256,
        types: vec!["text/css".into(), "application/json".into()],
    };
    let files = generate(&input(
        vec![host],
        vec![],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    let (_, body) = only_host_file(&files);
    assert!(body.contains("gzip_proxied any;"));
    assert_golden("20-host-gzip.conf", body);
}

#[test]
fn gzip_uses_default_types_when_empty() {
    let mut host = base_host(22, "a.example.com");
    host.gzip = Gzip {
        enabled: true,
        ..Default::default()
    };
    let files = generate(&input(
        vec![host],
        vec![],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    let (_, body) = only_host_file(&files);
    // Empty type list → curated default; level/min_length omitted at 0.
    assert!(body.contains("image/svg+xml"));
    assert!(!body.contains("gzip_comp_level"));
    assert!(!body.contains("gzip_min_length"));
}

#[test]
fn golden_maintenance() {
    // Maintenance mode replaces the proxy locations with a single 503 page and
    // skips upstreams/auth. Verified by angie -t (and a live 503 response) on
    // real Angie. The `<` from the escaped title proves HTML-escaping.
    let mut host = base_host(20, "app.example.com");
    host.maintenance = Maintenance {
        enabled: true,
        title: "Back <soon>".into(),
        message: "Upgrading the database.".into(),
    };
    let files = generate(&input(
        vec![host],
        vec![],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    let (_, body) = only_host_file(&files);
    assert!(body.contains("return 503"));
    assert!(
        body.contains("Back &lt;soon&gt;"),
        "title must be HTML-escaped"
    );
    assert!(
        !body.contains("proxy_pass"),
        "maintenance host must not proxy"
    );
    assert_golden("20-host-maintenance.conf", body);
}

#[test]
fn golden_geo_deny() {
    // Deny mode: the geo block defaults 0 and flags blocked CIDRs 1; each proxy
    // host gets `if ($ap_geo_deny) return 403`. Verified by angie -t on real Angie.
    let host = base_host(17, "app.example.com");
    let mut inp = input(vec![host], vec![], settings(DefaultSite::NotFound, false));
    inp.geo_policy = GeoPolicy {
        mode: GeoMode::Deny,
        countries: vec!["RU".into(), "CN".into()],
    };
    inp.geo_cidrs = vec![
        "1.2.3.0/24".into(),
        "5.6.0.0/16".into(),
        "2001:db8::/32".into(),
    ];
    let files = generate(&inp).unwrap();
    assert_golden("12-geo-deny.conf", &files["12-geo.conf"]);
    let (_, body) = only_host_file(&files);
    assert!(body.contains("if ($ap_geo_deny) { return 403; }"));
}

#[test]
fn geo_allow_defaults_inverted_and_inert_without_cidrs() {
    // Allow mode inverts the geo default (deny everyone, allow the listed CIDRs).
    let mut inp = input(
        vec![base_host(18, "a.example.com")],
        vec![],
        settings(DefaultSite::NotFound, false),
    );
    inp.geo_policy = GeoPolicy {
        mode: GeoMode::Allow,
        countries: vec!["DE".into()],
    };
    inp.geo_cidrs = vec!["9.10.0.0/16".into()];
    let files = generate(&inp).unwrap();
    assert!(files["12-geo.conf"].contains("default 1;"));
    assert!(files["12-geo.conf"].contains("9.10.0.0/16 0;"));

    // A policy with NO resolved CIDRs (missing dataset) is inert — no geo file,
    // no guard — so an allow-list can never lock every visitor out.
    let mut inert = input(
        vec![base_host(19, "b.example.com")],
        vec![],
        settings(DefaultSite::NotFound, false),
    );
    inert.geo_policy = GeoPolicy {
        mode: GeoMode::Allow,
        countries: vec!["DE".into()],
    };
    inert.geo_cidrs = vec![];
    let files = generate(&inert).unwrap();
    assert!(!files.contains_key("12-geo.conf"));
    let (_, body) = only_host_file(&files);
    assert!(!body.contains("ap_geo_deny"));
}

#[test]
fn golden_custom_headers() {
    // Response headers → add_header at server scope; request headers →
    // proxy_set_header inside the location, after the standard ones. Verified
    // by angie -t on real Angie.
    let mut host = base_host(16, "app.example.com");
    host.custom_headers = vec![
        CustomHeader {
            name: "X-Frame-Options".into(),
            value: "SAMEORIGIN".into(),
            direction: HeaderDirection::Response,
        },
        CustomHeader {
            name: "Content-Security-Policy".into(),
            value: "default-src 'self'; img-src 'self' data:".into(),
            direction: HeaderDirection::Response,
        },
        CustomHeader {
            name: "X-Tenant".into(),
            value: "acme-corp".into(),
            direction: HeaderDirection::Request,
        },
    ];
    let files = generate(&input(
        vec![host],
        vec![],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    let (_, body) = only_host_file(&files);
    assert_golden("20-host-custom-headers.conf", body);
}

#[test]
fn forward_auth_omitted_when_disabled() {
    // A host without forward auth emits no auth_request / verify location.
    let host = base_host(15, "plain.example.com");
    let files = generate(&input(
        vec![host],
        vec![],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    let (_, body) = only_host_file(&files);
    assert!(!body.contains("auth_request"));
    assert!(!body.contains("_forward_auth"));
}

#[test]
fn http3_only_over_tls() {
    // http3 on a plain-HTTP host (no cert) emits NOTHING quic — QUIC is TLS-only.
    let mut host = base_host(9, "plain.example.com");
    host.http3 = true;
    let files = generate(&input(
        vec![host],
        vec![],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    let (_, body) = only_host_file(&files);
    assert!(!body.contains("quic"), "no QUIC listener without TLS");
    assert!(!body.contains("http3"), "no http3 directive without TLS");
    assert!(!body.contains("Alt-Svc"), "no Alt-Svc without TLS");
}

#[test]
fn golden_host_websockets_hsts_block_exploits() {
    let mut host = base_host(3, "ws.example.com");
    host.certificate_id = Some(1);
    host.websockets_upgrade = true;
    host.hsts = true;
    host.hsts_subdomains = true;
    host.block_exploits = true;
    host.cache_assets = true;
    host.trust_forwarded_proto = true;
    let cert = ready_cert(1, "ws", &["ws.example.com"]);
    let files = generate(&input(
        vec![host],
        vec![cert],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    let (_, body) = only_host_file(&files);
    assert_golden("20-host-ws-hsts-exploits.conf", body);
}

#[test]
fn golden_host_custom_locations() {
    let mut host = base_host(5, "multi.example.com");
    host.locations = vec![
        CustomLocation {
            path: "/api".into(),
            forward_scheme: Scheme::Http,
            forward_host: "10.0.0.2".into(),
            forward_port: 9000,
            rewrite: None,
            snippet: None,
        },
        CustomLocation {
            path: "/legacy".into(),
            forward_scheme: Scheme::Https,
            forward_host: "backend.lan".into(),
            forward_port: 8443,
            rewrite: Some("/v2".into()),
            snippet: None,
        },
    ];
    let files = generate(&input(
        vec![host],
        vec![],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    let (_, body) = only_host_file(&files);
    assert_golden("20-host-custom-locations.conf", body);
}

#[test]
fn golden_host_advanced_snippet() {
    let mut host = base_host(9, "snip.example.com");
    host.advanced_snippet = Some("client_max_body_size 100m;\nproxy_read_timeout 300s;".into());
    // Also exercise a per-location snippet.
    host.locations = vec![CustomLocation {
        path: "/upload".into(),
        forward_scheme: Scheme::Http,
        forward_host: "10.0.0.9".into(),
        forward_port: 8000,
        rewrite: None,
        snippet: Some("client_max_body_size 1g;".into()),
    }];
    let files = generate(&input(
        vec![host],
        vec![],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    let (_, body) = only_host_file(&files);
    assert_golden("20-host-advanced-snippet.conf", body);
}

fn base_redirect(id: i64, domain: &str, target: &str) -> RedirectHost {
    RedirectHost {
        id,
        domains: vec![domain.into()],
        forward_scheme: RedirectScheme::Https,
        forward_domain: target.into(),
        forward_http_code: 301,
        preserve_path: true,
        certificate_id: None,
        force_ssl: false,
        hsts: false,
        hsts_subdomains: false,
        http2: true,
        block_exploits: false,
        advanced_snippet: None,
        enabled: true,
        created_at: 0,
        updated_at: 0,
    }
}

fn input_redirect(redirects: Vec<RedirectHost>, certs: Vec<Certificate>) -> GeneratorInput {
    let mut gi = input(vec![], certs, settings(DefaultSite::NotFound, false));
    gi.redirect_hosts = redirects;
    gi
}

fn input_dead(deads: Vec<DeadHost>, certs: Vec<Certificate>) -> GeneratorInput {
    let mut gi = input(vec![], certs, settings(DefaultSite::NotFound, false));
    gi.dead_hosts = deads;
    gi
}

#[test]
fn golden_redirect_http_only() {
    // No cert → HTTP-only redirect preserving the path.
    let files = generate(&input_redirect(
        vec![base_redirect(1, "old.example.com", "new.example.com")],
        vec![],
    ))
    .unwrap();
    let body = files
        .iter()
        .find(|(k, _)| k.starts_with("30-redirect-"))
        .unwrap();
    assert_eq!(body.0, "30-redirect-1-old-example-com.conf");
    assert_golden("30-redirect-http.conf", body.1);
}

#[test]
fn golden_redirect_https_force_ssl_no_preserve() {
    // Cert ready + force_ssl + no path preservation + custom 302 code.
    let mut rh = base_redirect(2, "go.example.com", "dest.example.com");
    rh.certificate_id = Some(1);
    rh.force_ssl = true;
    rh.hsts = true;
    rh.preserve_path = false;
    rh.forward_http_code = 302;
    rh.forward_scheme = RedirectScheme::Http;
    let files = generate(&input_redirect(
        vec![rh],
        vec![ready_cert(1, "go", &["go.example.com"])],
    ))
    .unwrap();
    let body = files
        .iter()
        .find(|(k, _)| k.starts_with("30-redirect-"))
        .unwrap()
        .1;
    assert_golden("30-redirect-https.conf", body);
}

#[test]
fn golden_dead_host_https() {
    let dh = DeadHost {
        id: 3,
        domains: vec!["parked.example.com".into()],
        certificate_id: Some(1),
        force_ssl: true,
        hsts: false,
        hsts_subdomains: false,
        http2: true,
        advanced_snippet: None,
        enabled: true,
        created_at: 0,
        updated_at: 0,
    };
    let files = generate(&input_dead(
        vec![dh],
        vec![ready_cert(1, "parked", &["parked.example.com"])],
    ))
    .unwrap();
    let body = files
        .iter()
        .find(|(k, _)| k.starts_with("40-dead-"))
        .unwrap();
    assert_eq!(body.0, "40-dead-3-parked-example-com.conf");
    assert_golden("40-dead-https.conf", body.1);
}

#[test]
fn redirect_and_dead_disabled_emit_no_file() {
    let mut rh = base_redirect(1, "a.example.com", "b.example.com");
    rh.enabled = false;
    let mut dh = DeadHost {
        id: 2,
        domains: vec!["c.example.com".into()],
        certificate_id: None,
        force_ssl: false,
        hsts: false,
        hsts_subdomains: false,
        http2: true,
        advanced_snippet: None,
        enabled: false,
        created_at: 0,
        updated_at: 0,
    };
    let _ = &mut dh;
    let mut gi = input(vec![], vec![], settings(DefaultSite::NotFound, false));
    gi.redirect_hosts = vec![rh];
    gi.dead_hosts = vec![dh];
    let files = generate(&gi).unwrap();
    assert!(!files
        .keys()
        .any(|k| k.starts_with("30-redirect-") || k.starts_with("40-dead-")));
}

#[test]
fn golden_host_with_access_list() {
    // A host gated by an access list with both basic-auth users and IP rules
    // (satisfy all), pass_auth off → emits auth_basic + allow/deny + deny all +
    // Authorization strip, plus a separate access-<id>.htpasswd file.
    let mut host = base_host(2, "admin.example.com");
    host.access_list_id = Some(5);
    let acl = AccessList {
        id: 5,
        satisfy: "all".into(),
        pass_auth: false,
        users: vec![
            ("alice".into(), "$2b$10$abcdefghijklmnopqrstuv".into()),
            ("bob".into(), "$2b$10$0123456789012345678901".into()),
        ],
        clients: vec![
            ("allow".into(), "192.168.0.0/16".into()),
            ("deny".into(), "192.168.5.5".into()),
        ],
    };
    let files = generate(&input_acl(
        vec![host],
        vec![],
        settings(DefaultSite::NotFound, false),
        vec![acl],
    ))
    .unwrap();
    let host_body = files
        .iter()
        .find(|(k, _)| k.starts_with("20-host-"))
        .unwrap()
        .1;
    assert_golden("20-host-access-list.conf", host_body);
    // The htpasswd file is emitted with the users' hashes.
    let htp = files.get("access-5.htpasswd").expect("htpasswd file");
    assert_golden("access-5.htpasswd", htp);
}

#[test]
fn access_list_htpasswd_only_when_referenced_and_has_users() {
    // An access list with users but NOT referenced by any host → no file.
    let acl = AccessList {
        id: 9,
        satisfy: "all".into(),
        pass_auth: true,
        users: vec![("x".into(), "$2b$10$zzzzzzzzzzzzzzzzzzzzzz".into())],
        clients: vec![],
    };
    let files = generate(&input_acl(
        vec![base_host(1, "a.example.com")],
        vec![],
        settings(DefaultSite::NotFound, false),
        vec![acl],
    ))
    .unwrap();
    assert!(
        !files.keys().any(|k| k.ends_with(".htpasswd")),
        "unreferenced access list must not emit an htpasswd file"
    );
}

// --------------------------------------------------------------- behaviour

#[test]
fn disabled_host_produces_no_file() {
    let mut host = base_host(2, "off.example.com");
    host.enabled = false;
    let files = generate(&input(
        vec![host],
        vec![],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    assert!(
        !files.keys().any(|k| k.starts_with("20-host-")),
        "disabled host must not emit a file, got {:?}",
        files.keys().collect::<Vec<_>>()
    );
    // The three fixed files are always present.
    assert!(files.contains_key("00-panel.conf"));
    assert!(files.contains_key("05-default.conf"));
    assert!(files.contains_key("10-acme.conf"));
}

#[test]
fn cert_not_ready_falls_back_to_http_only() {
    // A host with a cert that hasn't been issued yet (ready=false) must render
    // HTTP-only: no 443 server, no force-ssl redirect (PLAN.md §4).
    let mut host = base_host(4, "pending.example.com");
    host.certificate_id = Some(11);
    host.force_ssl = true;
    let mut cert = ready_cert(11, "pending", &["pending.example.com"]);
    cert.ready = false;
    let files = generate(&input(
        vec![host],
        vec![cert],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    let (_, body) = only_host_file(&files);
    assert!(
        !body.contains("listen 443"),
        "should not emit 443 when cert not ready:\n{body}"
    );
    assert!(
        !body.contains("return 301 https://"),
        "should not force-ssl when cert not ready:\n{body}"
    );
    assert!(body.contains("listen 80;"));
    assert!(body.contains("proxy_pass http://host_4;"));
}

#[test]
fn missing_cert_row_renders_http_only() {
    // certificate_id points at a cert that isn't in the input list at all.
    let mut host = base_host(6, "orphan.example.com");
    host.certificate_id = Some(999);
    let files = generate(&input(
        vec![host],
        vec![],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    let (_, body) = only_host_file(&files);
    assert!(!body.contains("listen 443"));
    assert!(body.contains("listen 80;"));
}

#[test]
fn resolver_omitted_when_empty() {
    let mut s = settings(DefaultSite::NotFound, false);
    s.resolvers = vec![];
    let files = generate(&input(vec![], vec![], s)).unwrap();
    assert!(
        !files["00-panel.conf"].contains("resolver"),
        "empty resolver list must omit the directive"
    );
    // But the cache path and status server are still there.
    assert!(files["00-panel.conf"].contains("proxy_cache_path"));
    assert!(files["00-panel.conf"].contains("api /status/;"));
}

#[test]
fn multiple_hosts_sorted_and_named() {
    let files = generate(&input(
        vec![
            base_host(20, "b.example.com"),
            base_host(3, "a.example.com"),
        ],
        vec![],
        settings(DefaultSite::NotFound, false),
    ))
    .unwrap();
    let host_files: Vec<&String> = files.keys().filter(|k| k.starts_with("20-host-")).collect();
    assert_eq!(
        host_files,
        vec![
            "20-host-20-b-example-com.conf",
            "20-host-3-a-example-com.conf"
        ]
    );
}

#[test]
fn slugify_sanitizes_domains() {
    assert_eq!(slugify("app.example.com"), "app-example-com");
    assert_eq!(slugify("*.example.com"), "example-com");
    assert_eq!(slugify("XN--80A1ACNY.XN--P1AI"), "xn--80a1acny-xn--p1ai");
    assert_eq!(slugify("a__b"), "a-b");
    assert_eq!(slugify("...---..."), "host");
    assert_eq!(slugify(""), "host");
}

#[test]
fn redirect_url_injection_rejected() {
    // A hostile redirect target must fail generation rather than escape the
    // `return` directive (defence in depth atop upstream validation).
    for evil in [
        "https://x.com/; return 200 \"pwned\"",
        "https://x.com/\nserver_name evil;",
        "javascript:alert(1)",
        "https://x.com/{}",
        "https://x.com/$host",
    ] {
        let s = settings(DefaultSite::Redirect(evil.into()), false);
        assert!(
            generate(&input(vec![], vec![], s)).is_err(),
            "redirect url should be rejected: {evil:?}"
        );
    }
    // A clean absolute URL is accepted.
    let s = settings(
        DefaultSite::Redirect("https://good.example.com/path".into()),
        false,
    );
    assert!(generate(&input(vec![], vec![], s)).is_ok());
}

// --------------------------------------------------------------- header/meta

#[test]
fn header_roundtrips_and_detects_drift() {
    let body = "server {\n    listen 80;\n}\n";
    let wrapped = with_header(body);
    assert!(wrapped.starts_with("# MANAGED BY angie-panel "));
    // Body after the header is byte-identical to the input.
    let after_header = wrapped.split_once('\n').unwrap().1;
    assert_eq!(after_header, body);

    let meta = managed_meta(&wrapped).expect("should parse our own header");
    assert!(meta.hash_matches, "freshly wrapped file must verify");
    assert_eq!(meta.generator_version, env!("CARGO_PKG_VERSION"));

    // Tamper with the body → hash no longer matches.
    let tampered = wrapped.replace("listen 80;", "listen 81;");
    let meta2 = managed_meta(&tampered).unwrap();
    assert!(!meta2.hash_matches, "edited body must be detected");
}

#[test]
fn header_hash_is_stable() {
    // Re-wrapping the same body yields an identical header (determinism is what
    // keeps drift detection from crying wolf).
    let body = "a\nb\nc\n";
    assert_eq!(with_header(body), with_header(body));
}

#[test]
fn managed_meta_ignores_foreign_files() {
    assert!(managed_meta("# some hand-written file\nserver {}\n").is_none());
    assert!(managed_meta("").is_none());
    assert!(managed_meta("no newline at all").is_none());
}

#[test]
fn full_fileset_is_lint_clean() {
    // Everything the generator emits for a rich host set must pass its own
    // level-2 linter — the generator is never allowed to produce a config the
    // trust boundary would reject.
    let mut https = base_host(7, "secure.example.com");
    https.certificate_id = Some(42);
    https.force_ssl = true;
    https.hsts = true;
    https.hsts_subdomains = true;
    https.block_exploits = true;
    https.cache_assets = true;
    https.websockets_upgrade = true;
    https.locations = vec![CustomLocation {
        path: "/api".into(),
        forward_scheme: Scheme::Http,
        forward_host: "10.0.0.2".into(),
        forward_port: 9000,
        rewrite: Some("/v2".into()),
        snippet: Some("client_max_body_size 50m;".into()),
    }];
    https.advanced_snippet = Some("proxy_buffering on;".into());
    let cert = ready_cert(42, "secure", &["secure.example.com"]);
    let plain = base_host(8, "plain.example.com");

    for site in [
        DefaultSite::NotFound,
        DefaultSite::Drop444,
        DefaultSite::Redirect("https://example.com/".into()),
        DefaultSite::Html,
    ] {
        let files = generate(&input(
            vec![https.clone(), plain.clone()],
            vec![cert.clone()],
            settings(site, true),
        ))
        .unwrap();
        let policy = lint::LintPolicy {
            snippets_dir: snippets_dir(),
            public_dir: public_dir(),
            allow_advanced_snippets: true,
        };
        let violations = lint::check_fileset(&files, &policy);
        assert!(
            violations.is_empty(),
            "generator output must be lint-clean, got: {violations:#?}"
        );
    }
}
