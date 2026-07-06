//! Level-2 allowlist linter tests: a corpus of hostile files, each expected to
//! produce at least one violation, plus clean-config negatives.

use std::path::PathBuf;

use super::*;
use crate::generator::FileSet;

fn policy(allow_snippets: bool) -> LintPolicy {
    LintPolicy {
        snippets_dir: PathBuf::from("/usr/share/angie-panel/snippets"),
        public_dir: PathBuf::from("/var/lib/angie-panel/public"),
        allow_advanced_snippets: allow_snippets,
    }
}

/// Lint a single-file fileset and return the violations.
fn lint_one(body: &str) -> Vec<LintViolation> {
    let mut files = FileSet::new();
    files.insert("20-host-1-x.conf".to_string(), body.to_string());
    check_fileset(&files, &policy(true))
}

/// Assert that `body` produces a violation whose message contains `needle`.
#[track_caller]
fn assert_violates(body: &str, needle: &str) {
    let v = lint_one(body);
    assert!(
        v.iter().any(|x| x.message.contains(needle)),
        "expected a violation containing {needle:?} for:\n{body}\n--- got: {v:#?}"
    );
    // Every violation must carry a file name and (for these) a line number.
    for viol in &v {
        assert_eq!(viol.file, "20-host-1-x.conf");
        assert!(
            viol.line.is_some(),
            "violation should pinpoint a line: {viol:?}"
        );
    }
}

#[test]
fn denies_load_module() {
    assert_violates("load_module modules/ngx_evil.so;", "load_module");
}

#[test]
fn denies_error_log_outside_jail() {
    assert_violates("error_log /etc/shadow;", "/var/log/angie");
    assert_violates("access_log /root/.ssh/authorized_keys;", "/var/log/angie");
    // In-jail paths and special sinks are fine.
    assert!(lint_one("error_log /var/log/angie/host_1.log;").is_empty());
    assert!(lint_one("access_log off;").is_empty());
    assert!(lint_one("error_log stderr;").is_empty());
    assert!(lint_one("access_log syslog:server=unix:/dev/log;").is_empty());
}

#[test]
fn denies_proxy_pass_to_management_port() {
    // Loopback + a management port gets the specific callout.
    assert_violates("proxy_pass http://127.0.0.1:8100;", "management port");
    assert_violates("proxy_pass http://localhost:8080;", "management port");
    // Loopback / link-local on ANY port is denied (reaches the panel/status).
    assert_violates("proxy_pass http://127.0.0.1:3000;", "loopback");
    assert_violates("proxy_pass http://[::1]:9000;", "loopback");
    assert_violates("proxy_pass http://localhost:3000;", "loopback");
    assert_violates("proxy_pass http://169.254.1.1:80;", "link-local");
    // unix sockets.
    assert_violates("proxy_pass http://unix:/var/run/x.sock;", "unix socket");
    assert_violates("proxy_pass unix:/var/run/docker.sock:;", "unix socket");
}

#[test]
fn allows_lan_upstream_on_common_app_port() {
    // A non-loopback LAN upstream on 8080 (or 8100) is a legitimate target —
    // the management-port rule only bites loopback/local addresses.
    assert!(lint_one("proxy_pass http://192.168.1.5:8080;").is_empty());
    assert!(lint_one("proxy_pass http://10.0.0.3:8100;").is_empty());
}

#[test]
fn allows_legit_proxy_pass() {
    assert!(lint_one("proxy_pass http://host_7;").is_empty());
    assert!(lint_one("proxy_pass http://192.168.1.10:8080;").is_empty());
    assert!(lint_one("proxy_pass https://backend.lan:8443;").is_empty());
    // A non-management LAN port is fine even on a private IP.
    assert!(lint_one("proxy_pass http://10.0.0.2:9000;").is_empty());
}

#[test]
fn denies_root_outside_public() {
    assert_violates("root /;", "public dir");
    assert_violates("root /etc;", "public dir");
    assert_violates("alias /var/lib/angie-panel/panel.db;", "public dir");
    // The real public dir is allowed.
    assert!(lint_one("root /var/lib/angie-panel/public;").is_empty());
    assert!(lint_one("root /var/lib/angie-panel/public/site;").is_empty());
}

#[test]
fn denies_include_outside_snippets_dir() {
    assert_violates("include /etc/angie/angie.conf;", "snippets dir");
    assert_violates(
        "include /usr/share/angie-panel/../../etc/passwd;",
        "snippets dir",
    );
    assert_violates("include relative/path.conf;", "not absolute");
    // Legit package snippet includes pass.
    assert!(lint_one("include /usr/share/angie-panel/snippets/block-exploits.conf;").is_empty());
    assert!(lint_one("include /usr/share/angie-panel/snippets/cache-assets.conf;").is_empty());
}

#[test]
fn denies_autoindex_on() {
    assert_violates("autoindex on;", "autoindex");
    // autoindex off is fine.
    assert!(lint_one("autoindex off;").is_empty());
}

#[test]
fn denies_scripting() {
    assert_violates("perl_set $x 'sub { }';", "perl");
    assert_violates("js_import http.js;", "njs");
    assert_violates("js_content handler;", "njs");
    assert_violates("content_by_lua_block { os.execute('id') }", "lua");
}

#[test]
fn denies_ssl_certificate_filesystem_path() {
    assert_violates("ssl_certificate /etc/angie/evil.pem;", "$acme_cert_*");
    assert_violates("ssl_certificate_key /tmp/key.pem;", "$acme_cert_*");
    assert_violates("ssl_certificate data:foo;", "$acme_cert_*");
    // The variable form is accepted.
    assert!(lint_one("ssl_certificate $acme_cert_secure;").is_empty());
    assert!(lint_one("ssl_certificate_key $acme_cert_key_secure;").is_empty());
    // Wrong prefix pairing is rejected.
    assert_violates("ssl_certificate $acme_cert_key_x;", "$acme_cert_*");
}

#[test]
fn denies_context_breakout_snippet() {
    // The canonical snippet-injection: close our location, open a new one that
    // exposes the filesystem. Even inserted verbatim, the tokenizer sees the
    // `root /;` as a fresh directive and the include/root checks fire.
    let evil = "location / {\n    proxy_pass http://host_1;\n} location /x { root /;\n}\n";
    assert_violates(evil, "public dir");
    // A breakout that turns on autoindex.
    let evil2 = "proxy_set_header Host $host;\n} location /fs { root /var; autoindex on;";
    let v = lint_one(evil2);
    assert!(v.iter().any(|x| x.message.contains("autoindex")));
    assert!(v.iter().any(|x| x.message.contains("public dir")));
}

#[test]
fn denies_brace_imbalance() {
    // A stray closing brace (drops subsequent directives into a parent context).
    assert_violates("proxy_set_header Host $host;\n}\n", "context breakout");
    // Leaving a block open swallows whatever the generator appends after.
    assert_violates(
        "location /x {\n    proxy_pass http://host_1;\n",
        "left open",
    );
    // Balanced braces (and braces inside strings/comments) do NOT trip it.
    assert!(lint_one("location / {\n    proxy_pass http://host_1;\n}\n").is_empty());
    assert!(lint_one("add_header X \"a { b } c\";\nproxy_pass http://host_1;").is_empty());
    assert!(lint_one("# a stray } in a comment\nproxy_pass http://host_1;").is_empty());
}

#[test]
fn tokenizer_ignores_comments_and_strings() {
    // A dangerous directive that is only a comment must NOT fire.
    assert!(lint_one("# load_module modules/x.so;\nproxy_pass http://host_1;").is_empty());
    // A dangerous-looking token inside a quoted string must NOT fire: here the
    // add_header value merely mentions "root /" but it is not a directive.
    assert!(lint_one("add_header X-Note \"root / is fine as text\";").is_empty());
    // But a real directive after a string on the same logical line still fires.
    assert_violates("add_header X-Note \"hello\"; root /;", "public dir");
}

#[test]
fn reports_precise_line_numbers() {
    let body = "server {\n    listen 80;\n    load_module x.so;\n    root /var/lib/angie-panel/public;\n}\n";
    let v = lint_one(body);
    let lm = v
        .iter()
        .find(|x| x.message.contains("load_module"))
        .unwrap();
    assert_eq!(lm.line, Some(3), "load_module is on line 3");
}

#[test]
fn unterminated_string_is_reported() {
    let v = lint_one("add_header X \"unclosed;\nproxy_pass http://host_1;");
    assert!(
        v.iter().any(|x| x.message.contains("tokenize")),
        "an unterminated string should be flagged, got {v:#?}"
    );
}

#[test]
fn clean_generated_host_passes() {
    // A realistic generated 443 server body must be violation-free.
    let body = r#"upstream host_7 {
    zone host_7 64k;
    server 192.168.1.10:8080;
}

server {
    listen 80;
    server_name secure.example.com;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl;
    http2 on;
    server_name secure.example.com;
    status_zone host_7;
    ssl_certificate     $acme_cert_secure;
    ssl_certificate_key $acme_cert_key_secure;
    add_header Strict-Transport-Security "max-age=63072000; includeSubDomains" always;
    include /usr/share/angie-panel/snippets/block-exploits.conf;
    location / {
        proxy_pass http://host_7;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        include /usr/share/angie-panel/snippets/cache-assets.conf;
    }
}
"#;
    assert!(lint_one(body).is_empty(), "got {:#?}", lint_one(body));
}

#[test]
fn multi_file_violations_carry_filename() {
    let mut files = FileSet::new();
    files.insert(
        "20-host-1-a.conf".into(),
        "proxy_pass http://host_1;\n".into(),
    );
    files.insert("20-host-2-b.conf".into(), "root /;\n".into());
    let v = check_fileset(&files, &policy(true));
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].file, "20-host-2-b.conf");
}
