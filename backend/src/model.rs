//! Domain model + strict server-side field validation (PLAN.md §7).
//!
//! Every user-controlled value that ends up inside generated Angie config
//! passes through here. Policy: ALLOWLIST and reject — never escape. A value
//! that merely *parses* in Angie is not safe (a syntactically valid injected
//! directive is the attack), so each field admits only characters that can
//! never terminate or extend a directive.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    Http,
    Https,
}

impl Scheme {
    pub fn as_str(self) -> &'static str {
        match self {
            Scheme::Http => "http",
            Scheme::Https => "https",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomLocation {
    pub path: String,
    pub forward_scheme: Scheme,
    pub forward_host: String,
    pub forward_port: u16,
    #[serde(default)]
    pub rewrite: Option<String>,
    #[serde(default)]
    pub snippet: Option<String>,
}

/// Per-host rate limiting (Angie `limit_req` / `limit_conn`, keyed on the
/// client IP). Stored as one JSON column on the host; all-zero / disabled by
/// default. The generator emits a shared-memory zone per host plus the
/// server-scope limit directives (see generator::gen_rate_limits).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimit {
    #[serde(default)]
    pub enabled: bool,
    /// Request rate ceiling in requests/second (`limit_req rate=Nr/s`). 0 = no
    /// request-rate limit (connection limiting may still apply).
    #[serde(default)]
    pub rps: u32,
    /// Burst allowance above `rps` before requests are rejected/delayed.
    #[serde(default)]
    pub burst: u32,
    /// Serve the burst immediately instead of queueing it (`nodelay`).
    #[serde(default)]
    pub nodelay: bool,
    /// Max concurrent connections per client IP (`limit_conn`). 0 = no limit.
    #[serde(default)]
    pub conn: u32,
}

/// Sanity ceilings — generous, just to reject absurd values that would make no
/// operational sense (and keep the generated numbers bounded).
pub const MAX_RATE_RPS: u32 = 1_000_000;
pub const MAX_RATE_BURST: u32 = 100_000;
pub const MAX_RATE_CONN: u32 = 100_000;

/// Mutual TLS (client certificate) requirement for a host. The `ca_pem` is the
/// CA bundle that verifies presented client certs; when set, the generator
/// emits `ssl_client_certificate` + `ssl_verify_client` on the HTTPS server.
/// `optional` requests a cert but does not reject clients that omit one (the
/// result is passed upstream via `$ssl_client_verify`); the default requires it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mtls {
    #[serde(default)]
    pub ca_pem: Option<String>,
    #[serde(default)]
    pub optional: bool,
}

impl Mtls {
    /// mTLS is active only when a CA bundle is present.
    pub fn active(&self) -> bool {
        self.ca_pem.as_ref().is_some_and(|p| !p.trim().is_empty())
    }
}

/// Max CA bundle size — a generous ceiling for a chain of a few certs.
pub const MAX_CA_PEM_LEN: usize = 64 * 1024;

/// Validate + normalize a host's mTLS config. The CA bundle must look like PEM
/// certificate(s): at least one BEGIN/END CERTIFICATE block and only
/// PEM-safe characters. It is written to a file (never a directive), and
/// `angie -t` is the final gate on a structurally-valid but broken cert.
pub fn validate_mtls(mut mtls: Mtls) -> Result<Mtls, ApiError> {
    let pem = match mtls.ca_pem.map(|p| p.trim().to_string()) {
        Some(p) if !p.is_empty() => p,
        _ => return Ok(Mtls::default()),
    };
    if pem.len() > MAX_CA_PEM_LEN {
        return Err(bad("invalid_ca", "the CA bundle is too large"));
    }
    let begins = pem.matches("-----BEGIN CERTIFICATE-----").count();
    let ends = pem.matches("-----END CERTIFICATE-----").count();
    if begins == 0 || begins != ends {
        return Err(bad(
            "invalid_ca",
            "expected one or more PEM CERTIFICATE blocks",
        ));
    }
    // Only characters that legitimately appear in a PEM file (base64 + the
    // header/footer punctuation + whitespace). Rejects anything that could be
    // smuggled in — though the value only ever lands in a file, not a directive.
    let pem_ok = pem.bytes().all(|b| {
        b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=' | b'-' | b' ' | b'\n' | b'\r')
    });
    if !pem_ok {
        return Err(bad(
            "invalid_ca",
            "the CA bundle contains invalid characters",
        ));
    }
    mtls.ca_pem = Some(pem);
    Ok(mtls)
}

/// Forward authentication (SSO gateway) via Angie's `auth_request`. Every
/// request to the host is sub-verified against an external auth service
/// (oauth2-proxy / Authelia / Authentik). On a 2xx the request proceeds; a 401
/// either returns 401 or (if `sign_in_url` is set) redirects the browser to the
/// SSO login. Selected identity headers from the auth response are copied to the
/// upstream so the app knows who the user is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForwardAuth {
    #[serde(default)]
    pub enabled: bool,
    /// Internal verification endpoint. A server-side subrequest target, so it is
    /// SSRF-guarded exactly like an upstream (e.g. `http://10.0.0.5:9091/api/verify`).
    #[serde(default)]
    pub verify_url: String,
    /// Optional browser redirect target on 401 — the SSO sign-in page. The
    /// original URL is appended as `?rd=`, so this must carry no query of its own.
    #[serde(default)]
    pub sign_in_url: Option<String>,
    /// Identity headers from the auth response to forward to the upstream
    /// (e.g. `Remote-User`, `Remote-Groups`, `Remote-Email`).
    #[serde(default)]
    pub copy_headers: Vec<String>,
}

impl ForwardAuth {
    /// Active only when enabled AND a verification endpoint is set.
    pub fn active(&self) -> bool {
        self.enabled && !self.verify_url.trim().is_empty()
    }
}

pub const MAX_FORWARD_AUTH_HEADERS: usize = 20;
const MAX_URL_LEN: usize = 512;

/// Validate + normalize a host's forward-auth config. Every field is interpolated
/// into a directive (`proxy_pass`, `return 302`, `auth_request_set`,
/// `proxy_set_header`), so allowlist-and-reject: split each URL into
/// scheme/host/port/path and admit only characters that cannot break out of a
/// directive.
pub fn validate_forward_auth(
    mut fa: ForwardAuth,
    upstream_policy: &UpstreamPolicy,
) -> Result<ForwardAuth, ApiError> {
    if !fa.enabled {
        return Ok(ForwardAuth::default());
    }
    // The verify endpoint is fetched by Angie itself → SSRF-guarded host.
    fa.verify_url = validate_http_url(&fa.verify_url, Some(upstream_policy))?;
    // The sign-in URL is only ever handed to the browser in a redirect, so it is
    // not an SSRF vector; still charset-validated to prevent config injection.
    fa.sign_in_url = match fa.sign_in_url.map(|s| s.trim().to_string()) {
        Some(s) if !s.is_empty() => Some(validate_http_url(&s, None)?),
        _ => None,
    };
    if fa.copy_headers.len() > MAX_FORWARD_AUTH_HEADERS {
        return Err(bad("invalid_forward_auth", "too many identity headers"));
    }
    let mut headers = Vec::new();
    for h in &fa.copy_headers {
        let name = validate_header_name(h)?;
        if !headers.contains(&name) {
            headers.push(name);
        }
    }
    fa.copy_headers = headers;
    Ok(fa)
}

/// Parse + validate an `http(s)://host[:port][/path]` URL. When `ssrf` is set,
/// the host is checked against the upstream policy (loopback/link-local block);
/// otherwise only its format is validated (a browser-redirect target).
fn validate_http_url(raw: &str, ssrf: Option<&UpstreamPolicy>) -> Result<String, ApiError> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(bad("invalid_forward_auth", "empty URL"));
    }
    if s.len() > MAX_URL_LEN {
        return Err(bad("invalid_forward_auth", "URL is too long"));
    }
    let (scheme, rest) = s.split_once("://").ok_or_else(|| {
        bad(
            "invalid_forward_auth",
            "URL must start with http:// or https://",
        )
    })?;
    let scheme = scheme.to_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(bad(
            "invalid_forward_auth",
            "URL scheme must be http or https",
        ));
    }
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    if authority.is_empty() {
        return Err(bad("invalid_forward_auth", "URL is missing a host"));
    }
    if authority.contains('@') {
        return Err(bad(
            "invalid_forward_auth",
            "URL must not embed credentials",
        ));
    }
    let (host, port) = split_host_port(authority)?;
    // SSRF-guarded when a policy is given; a permissive policy just validates the
    // host format (used for the browser-facing sign-in URL).
    let permissive = UpstreamPolicy {
        allow_loopback: true,
    };
    let host_norm = validate_forward_host(&host, ssrf.unwrap_or(&permissive))?;
    if !path.is_empty() {
        validate_url_path(path)?;
    }
    let mut out = format!("{scheme}://{host_norm}");
    if let Some(p) = port {
        out.push_str(&format!(":{p}"));
    }
    out.push_str(path);
    Ok(out)
}

/// Split a URL authority into (host, port). IPv6 literals must be bracketed
/// (`[::1]:9091`); a bare hostname/IPv4 splits on the last colon.
fn split_host_port(authority: &str) -> Result<(String, Option<u16>), ApiError> {
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, after) = rest
            .split_once(']')
            .ok_or_else(|| bad("invalid_forward_auth", "unterminated IPv6 literal in URL"))?;
        let port = if after.is_empty() {
            None
        } else if let Some(p) = after.strip_prefix(':') {
            Some(parse_port(p)?)
        } else {
            return Err(bad(
                "invalid_forward_auth",
                "malformed IPv6 authority in URL",
            ));
        };
        return Ok((host.to_string(), port));
    }
    match authority.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => {
            Ok((h.to_string(), Some(parse_port(p)?)))
        }
        _ => Ok((authority.to_string(), None)),
    }
}

fn parse_port(s: &str) -> Result<u16, ApiError> {
    s.parse::<u16>()
        .ok()
        .filter(|&p| p > 0)
        .ok_or_else(|| bad("invalid_forward_auth", "invalid port in URL"))
}

/// A URL path (no query/fragment): a conservative charset that cannot terminate
/// or extend a directive, plus a traversal guard.
fn validate_url_path(path: &str) -> Result<(), ApiError> {
    let ok = path
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.'));
    if !ok || path.contains("..") {
        return Err(bad(
            "invalid_forward_auth",
            "URL path contains invalid characters (no query, spaces, or '..')",
        ));
    }
    Ok(())
}

/// An HTTP header name (RFC 7230 token, simplified): starts alphanumeric, then
/// alphanumerics and hyphens. Case is preserved for `proxy_set_header`.
fn validate_header_name(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(bad("invalid_forward_auth", "empty header name"));
    }
    if s.len() > 64 {
        return Err(bad("invalid_forward_auth", "header name is too long"));
    }
    let bytes = s.as_bytes();
    let ok = bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'-');
    if !ok {
        return Err(bad(
            "invalid_forward_auth",
            format!("'{raw}' is not a valid header name"),
        ));
    }
    Ok(s.to_string())
}

/// Load-balancing method for a host's upstream pool.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BalanceMethod {
    /// Weighted round-robin (Angie/nginx default — no method directive emitted).
    #[default]
    RoundRobin,
    /// Fewest active connections wins.
    LeastConn,
    /// Sticky by client IP (cannot be combined with `backup` servers).
    IpHash,
}

impl BalanceMethod {
    /// The upstream directive to emit, or None for round-robin (the default).
    pub fn directive(self) -> Option<&'static str> {
        match self {
            BalanceMethod::RoundRobin => None,
            BalanceMethod::LeastConn => Some("least_conn"),
            BalanceMethod::IpHash => Some("ip_hash"),
        }
    }
}

/// One additional backend server beyond the host's primary
/// (`forward_host:forward_port`). Validated exactly like the primary upstream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpstreamServer {
    pub host: String,
    pub port: u16,
    #[serde(default = "default_weight")]
    pub weight: u32,
    /// Only used when all primary/non-backup servers are unavailable.
    #[serde(default)]
    pub backup: bool,
    /// Marked out of rotation (kept for quick re-enable).
    #[serde(default)]
    pub down: bool,
}

impl Default for UpstreamServer {
    fn default() -> Self {
        UpstreamServer {
            host: String::new(),
            port: 0,
            weight: 1,
            backup: false,
            down: false,
        }
    }
}

/// Load balancing + passive health for a host. `servers` are ADDITIONAL peers;
/// the primary is always the host's `forward_host:forward_port`. Passive health
/// (`max_fails`/`fail_timeout`) applies to every peer — Angie removes a peer
/// after `max_fails` failures within `fail_timeout` and retries after it.
/// Active health checks (`health_check`) are Angie PRO only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Upstream {
    #[serde(default)]
    pub servers: Vec<UpstreamServer>,
    #[serde(default)]
    pub method: BalanceMethod,
    #[serde(default = "default_weight")]
    pub primary_weight: u32,
    #[serde(default = "default_max_fails")]
    pub max_fails: u32,
    #[serde(default = "default_fail_timeout")]
    pub fail_timeout_secs: u32,
}

// Manual Default so `Upstream::default()` matches the serde field defaults
// (a plain single-server host with Angie's own max_fails=1 / fail_timeout=10s),
// NOT the all-zero derive which would emit `weight=0 max_fails=0`.
impl Default for Upstream {
    fn default() -> Self {
        Upstream {
            servers: Vec::new(),
            method: BalanceMethod::RoundRobin,
            primary_weight: 1,
            max_fails: 1,
            fail_timeout_secs: 10,
        }
    }
}

fn default_weight() -> u32 {
    1
}
fn default_max_fails() -> u32 {
    1
}
fn default_fail_timeout() -> u32 {
    10
}

pub const MAX_UPSTREAM_SERVERS: usize = 16;
pub const MAX_WEIGHT: u32 = 1000;
pub const MAX_FAILS: u32 = 1000;
pub const MAX_FAIL_TIMEOUT: u32 = 86400;

/// Validate + normalize a host's upstream/load-balancing config. Each extra
/// server's host runs through the SAME SSRF-guarded validation as the primary.
pub fn validate_upstream(mut up: Upstream, policy: &UpstreamPolicy) -> Result<Upstream, ApiError> {
    if up.servers.len() > MAX_UPSTREAM_SERVERS {
        return Err(bad("invalid_upstream", "too many backend servers"));
    }
    let weight_ok = |w: u32| (1..=MAX_WEIGHT).contains(&w);
    if !weight_ok(up.primary_weight) {
        return Err(bad("invalid_upstream", "primary weight out of range"));
    }
    if up.max_fails > MAX_FAILS {
        return Err(bad("invalid_upstream", "max_fails is too high"));
    }
    if up.fail_timeout_secs == 0 || up.fail_timeout_secs > MAX_FAIL_TIMEOUT {
        return Err(bad("invalid_upstream", "fail_timeout out of range"));
    }
    for s in &mut up.servers {
        s.host = validate_forward_host(&s.host, policy)?;
        if s.port == 0 {
            return Err(bad("invalid_upstream", "server port must be 1-65535"));
        }
        if !weight_ok(s.weight) {
            return Err(bad("invalid_upstream", "server weight out of range"));
        }
        // ip_hash cannot be combined with backup servers (Angie rejects it).
        if up.method == BalanceMethod::IpHash && s.backup {
            return Err(bad(
                "invalid_upstream",
                "ip_hash balancing cannot be combined with backup servers",
            ));
        }
    }
    Ok(up)
}

/// Validate + normalize a rate-limit config. When disabled it is flattened to
/// the default (so the DB/JSON never carries stale numbers). When enabled it
/// must define at least one of a request rate or a connection limit.
pub fn validate_rate_limit(mut rl: RateLimit) -> Result<RateLimit, ApiError> {
    if !rl.enabled {
        return Ok(RateLimit::default());
    }
    if rl.rps == 0 && rl.conn == 0 {
        return Err(bad(
            "invalid_rate_limit",
            "enable at least a request rate (rps) or a connection limit (conn)",
        ));
    }
    if rl.rps > MAX_RATE_RPS {
        return Err(bad("invalid_rate_limit", "request rate is too high"));
    }
    if rl.burst > MAX_RATE_BURST {
        return Err(bad("invalid_rate_limit", "burst is too high"));
    }
    if rl.conn > MAX_RATE_CONN {
        return Err(bad("invalid_rate_limit", "connection limit is too high"));
    }
    // burst / nodelay only mean anything alongside a request rate.
    if rl.rps == 0 {
        rl.burst = 0;
        rl.nodelay = false;
    }
    Ok(rl)
}

#[derive(Debug, Clone, Serialize)]
pub struct ProxyHost {
    pub id: i64,
    pub domains: Vec<String>,
    pub forward_scheme: Scheme,
    pub forward_host: String,
    pub forward_port: u16,
    pub websockets_upgrade: bool,
    pub block_exploits: bool,
    pub cache_assets: bool,
    pub http2: bool,
    pub http3: bool,
    pub force_ssl: bool,
    pub hsts: bool,
    pub hsts_subdomains: bool,
    pub trust_forwarded_proto: bool,
    pub certificate_id: Option<i64>,
    pub access_list_id: Option<i64>,
    pub locations: Vec<CustomLocation>,
    pub advanced_snippet: Option<String>,
    pub rate_limit: RateLimit,
    pub upstream: Upstream,
    pub mtls: Mtls,
    pub forward_auth: ForwardAuth,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Create/update payload; validation normalizes it in place.
#[derive(Debug, Clone, Deserialize)]
pub struct ProxyHostInput {
    pub domains: Vec<String>,
    pub forward_scheme: Scheme,
    pub forward_host: String,
    pub forward_port: u16,
    #[serde(default)]
    pub websockets_upgrade: bool,
    #[serde(default)]
    pub block_exploits: bool,
    #[serde(default)]
    pub cache_assets: bool,
    #[serde(default = "default_true")]
    pub http2: bool,
    #[serde(default)]
    pub http3: bool,
    #[serde(default)]
    pub force_ssl: bool,
    #[serde(default)]
    pub hsts: bool,
    #[serde(default)]
    pub hsts_subdomains: bool,
    #[serde(default)]
    pub trust_forwarded_proto: bool,
    #[serde(default)]
    pub certificate_id: Option<i64>,
    #[serde(default)]
    pub access_list_id: Option<i64>,
    #[serde(default)]
    pub locations: Vec<CustomLocation>,
    #[serde(default)]
    pub advanced_snippet: Option<String>,
    #[serde(default)]
    pub rate_limit: RateLimit,
    #[serde(default)]
    pub upstream: Upstream,
    #[serde(default)]
    pub mtls: Mtls,
    #[serde(default)]
    pub forward_auth: ForwardAuth,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

pub const MAX_DOMAINS_PER_HOST: usize = 50;
pub const MAX_LOCATIONS_PER_HOST: usize = 30;
pub const MAX_SNIPPET_LEN: usize = 8 * 1024;

// ------------------------------------------------------------- validation

fn bad(code: &'static str, msg: impl Into<String>) -> ApiError {
    ApiError::bad_request(code, msg)
}

/// Normalize + validate a public-facing domain (server_name / SAN member).
/// Accepts an optional leading `*.` wildcard; IDN is punycode-encoded first.
pub fn validate_domain(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim().trim_end_matches('.').to_lowercase();
    if s.is_empty() {
        return Err(bad("invalid_domain", "empty domain"));
    }
    let (wildcard, rest) = match s.strip_prefix("*.") {
        Some(r) => (true, r),
        None => (false, s.as_str()),
    };
    let ascii = idna::domain_to_ascii(rest)
        .map_err(|_| bad("invalid_domain", format!("not a valid domain: {raw}")))?;
    if ascii.len() > 253 || ascii.is_empty() {
        return Err(bad("invalid_domain", format!("domain too long: {raw}")));
    }
    let labels: Vec<&str> = ascii.split('.').collect();
    if labels.len() < 2 {
        return Err(bad(
            "invalid_domain",
            format!("'{raw}': need at least two labels (e.g. host.example.com)"),
        ));
    }
    for label in &labels {
        if !valid_dns_label(label) {
            return Err(bad(
                "invalid_domain",
                format!("invalid label '{label}' in {raw}"),
            ));
        }
    }
    Ok(if wildcard {
        format!("*.{ascii}")
    } else {
        ascii
    })
}

fn valid_dns_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Upstream target: a bare IP or a bare hostname. No scheme, port, path,
/// userinfo, brackets — those belong to other fields, and any of them inside
/// this string could extend the generated `server` directive.
pub fn validate_forward_host(raw: &str, policy: &UpstreamPolicy) -> Result<String, ApiError> {
    let s = raw.trim().to_lowercase();
    if s.is_empty() {
        return Err(bad("invalid_forward_host", "empty forward host"));
    }
    if let Ok(ip) = s.parse::<std::net::IpAddr>() {
        policy.check_ip(ip)?;
        return Ok(s);
    }
    // Bare hostname: single label allowed (LAN hosts), dots allowed.
    if s.len() > 253 {
        return Err(bad("invalid_forward_host", "hostname too long"));
    }
    if !s.split('.').all(valid_dns_label) {
        return Err(bad(
            "invalid_forward_host",
            format!("'{raw}' is not a bare IP or hostname (no scheme/port/path here)"),
        ));
    }
    Ok(s)
}

/// SSRF guard for upstream IPs (PLAN.md §7): loopback and link-local targets
/// expose panel/status management endpoints; require an explicit opt-in.
pub struct UpstreamPolicy {
    pub allow_loopback: bool,
}

impl UpstreamPolicy {
    fn check_ip(&self, ip: std::net::IpAddr) -> Result<(), ApiError> {
        use std::net::IpAddr;
        let blocked = match ip {
            IpAddr::V4(v4) => {
                v4.is_loopback() || v4.is_link_local() || v4.is_unspecified() || v4.is_broadcast()
            }
            IpAddr::V6(v6) => {
                v6.is_loopback() || v6.is_unspecified() || (v6.segments()[0] & 0xffc0) == 0xfe80
                // link-local
            }
        };
        if blocked && !self.allow_loopback {
            return Err(bad(
                "forbidden_forward_host",
                format!(
                    "{ip} points at this machine (loopback/link-local); it can expose the \
                     panel or the Angie status API. Set allow_loopback_upstreams = true in \
                     /etc/angie-panel.toml to permit it deliberately"
                ),
            ));
        }
        Ok(())
    }
}

/// Location path: prefix match only in v1 (regex/named locations are an
/// unbounded validation surface — deliberately not supported).
pub fn validate_location_path(raw: &str) -> Result<String, ApiError> {
    // Trim spaces/tabs only — NOT newlines/CR. `str::trim()` would strip a
    // trailing control char and let it pass the charset check below; keeping
    // it in makes the charset check reject it.
    let s = raw.trim_matches([' ', '\t']).to_string();
    let ok = s.starts_with('/')
        && s.len() <= 200
        && !s.contains("..")
        && s.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'/' | b'_' | b'.' | b'-' | b'~' | b'%')
        });
    if !ok {
        return Err(bad(
            "invalid_location_path",
            format!("'{raw}': must start with / and use only [A-Za-z0-9/_.-~%]"),
        ));
    }
    Ok(s)
}

pub fn validate_rewrite(raw: &str) -> Result<String, ApiError> {
    // See validate_location_path: trim only spaces/tabs so control chars are
    // caught by the charset check rather than silently stripped.
    let s = raw.trim_matches([' ', '\t']).to_string();
    let ok = s.starts_with('/')
        && s.len() <= 200
        && !s.contains("..")
        && s.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'/' | b'_' | b'.' | b'-' | b'~' | b'%')
        });
    if !ok {
        return Err(bad(
            "invalid_rewrite",
            format!("'{raw}': must start with / and use only [A-Za-z0-9/_.-~%]"),
        ));
    }
    Ok(s)
}

fn validate_snippet(raw: &str, allow_advanced_snippets: bool) -> Result<String, ApiError> {
    if !allow_advanced_snippets {
        return Err(ApiError::forbidden(
            "snippets_disabled",
            "custom config snippets are disabled; a root user must set \
             allow_advanced_snippets = true in /etc/angie-panel.toml (they are \
             root-equivalent — see the docs)",
        ));
    }
    if raw.len() > MAX_SNIPPET_LEN {
        return Err(bad("snippet_too_long", "snippet exceeds 8 KiB"));
    }
    Ok(raw.to_string())
}

/// Validate and normalize the whole payload. `allow_*` flags come from the
/// root-owned config file.
pub fn validate_host_input(
    mut input: ProxyHostInput,
    allow_advanced_snippets: bool,
    upstream_policy: &UpstreamPolicy,
) -> Result<ProxyHostInput, ApiError> {
    if input.domains.is_empty() {
        return Err(bad("invalid_domain", "at least one domain is required"));
    }
    if input.domains.len() > MAX_DOMAINS_PER_HOST {
        return Err(bad("invalid_domain", "too many domains"));
    }
    let mut domains = Vec::with_capacity(input.domains.len());
    for d in &input.domains {
        let norm = validate_domain(d)?;
        if !domains.contains(&norm) {
            domains.push(norm);
        }
    }
    input.domains = domains;

    input.forward_host = validate_forward_host(&input.forward_host, upstream_policy)?;
    if input.forward_port == 0 {
        return Err(bad("invalid_forward_port", "port must be 1-65535"));
    }

    if input.locations.len() > MAX_LOCATIONS_PER_HOST {
        return Err(bad("invalid_location_path", "too many locations"));
    }
    let mut seen_paths = std::collections::HashSet::new();
    for loc in &mut input.locations {
        loc.path = validate_location_path(&loc.path)?;
        if !seen_paths.insert(loc.path.clone()) {
            return Err(bad(
                "invalid_location_path",
                format!("duplicate location path {}", loc.path),
            ));
        }
        loc.forward_host = validate_forward_host(&loc.forward_host, upstream_policy)?;
        if loc.forward_port == 0 {
            return Err(bad("invalid_forward_port", "port must be 1-65535"));
        }
        if let Some(r) = &loc.rewrite {
            if r.trim().is_empty() {
                loc.rewrite = None;
            } else {
                loc.rewrite = Some(validate_rewrite(r)?);
            }
        }
        if let Some(s) = &loc.snippet {
            if s.trim().is_empty() {
                loc.snippet = None;
            } else {
                loc.snippet = Some(validate_snippet(s, allow_advanced_snippets)?);
            }
        }
    }

    if let Some(s) = &input.advanced_snippet {
        if s.trim().is_empty() {
            input.advanced_snippet = None;
        } else {
            input.advanced_snippet = Some(validate_snippet(s, allow_advanced_snippets)?);
        }
    }

    input.rate_limit = validate_rate_limit(input.rate_limit)?;
    input.upstream = validate_upstream(input.upstream, upstream_policy)?;
    input.mtls = validate_mtls(input.mtls)?;
    input.forward_auth = validate_forward_auth(input.forward_auth, upstream_policy)?;

    // hsts_subdomains only makes sense with hsts, http2/force_ssl only with
    // TLS — kept as-is in the DB; the generator applies the actual gating.
    Ok(input)
}

// ============================================================ certificates

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Challenge {
    /// http-01 — default; the ACME module serves the challenge on port 80.
    Http,
    /// dns-01 — the only challenge that can issue wildcards; Angie answers
    /// the validation DNS query itself (needs NS delegation + UDP/53).
    Dns,
    /// tls-alpn-01 — issues entirely over 443 (no port 80 needed). Angie 1.11+.
    Alpn,
}

impl Challenge {
    pub fn as_str(self) -> &'static str {
        match self {
            Challenge::Http => "http",
            Challenge::Dns => "dns",
            Challenge::Alpn => "alpn",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyType {
    Ecdsa,
    Rsa,
}

impl KeyType {
    pub fn as_str(self) -> &'static str {
        match self {
            KeyType::Ecdsa => "ecdsa",
            KeyType::Rsa => "rsa",
        }
    }
}

/// A certificate row as returned to the UI (issuance status is layered on
/// separately from the /status API — see the certs handler).
#[derive(Debug, Clone, Serialize)]
pub struct Certificate {
    pub id: i64,
    pub name: String,
    pub domains: Vec<String>,
    pub challenge: Challenge,
    pub key_type: KeyType,
    pub email: Option<String>,
    pub staging: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CertificateInput {
    pub name: String,
    pub domains: Vec<String>,
    #[serde(default = "default_challenge")]
    pub challenge: Challenge,
    #[serde(default = "default_key_type")]
    pub key_type: KeyType,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub staging: bool,
}

fn default_challenge() -> Challenge {
    Challenge::Http
}
fn default_key_type() -> KeyType {
    KeyType::Ecdsa
}

pub const MAX_CERT_NAME_LEN: usize = 32;

/// The acme_client name is interpolated into directive names AND into the
/// `$acme_cert_<name>` variable, so it must be a strict identifier.
pub fn validate_cert_name(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    let ok = !s.is_empty()
        && s.len() <= MAX_CERT_NAME_LEN
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if !ok {
        return Err(bad(
            "invalid_cert_name",
            format!("'{raw}': name must match ^[a-z0-9_]{{1,{MAX_CERT_NAME_LEN}}}$"),
        ));
    }
    Ok(s.to_string())
}

pub fn validate_cert_input(mut input: CertificateInput) -> Result<CertificateInput, ApiError> {
    input.name = validate_cert_name(&input.name)?;

    if input.domains.is_empty() {
        return Err(bad(
            "invalid_domain",
            "a certificate needs at least one domain",
        ));
    }
    if input.domains.len() > MAX_DOMAINS_PER_HOST {
        return Err(bad("invalid_domain", "too many domains"));
    }
    let mut domains = Vec::with_capacity(input.domains.len());
    let mut has_wildcard = false;
    for d in &input.domains {
        let norm = validate_domain(d)?;
        if norm.starts_with("*.") {
            has_wildcard = true;
        }
        if !domains.contains(&norm) {
            domains.push(norm);
        }
    }
    input.domains = domains;

    // Wildcards require dns-01 (Angie/ACME constraint); http-01 and alpn cannot
    // issue them.
    if has_wildcard && input.challenge != Challenge::Dns {
        return Err(bad(
            "wildcard_needs_dns",
            "wildcard domains (*.example.com) require the DNS-01 challenge",
        ));
    }

    if let Some(email) = &input.email {
        let e = email.trim();
        if e.is_empty() {
            input.email = None;
        } else if !e.contains('@') || e.len() < 3 || e.len() > 254 {
            return Err(bad("invalid_email", "invalid contact email"));
        } else {
            input.email = Some(e.to_lowercase());
        }
    }

    Ok(input)
}

// ============================================================ access lists

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Satisfy {
    /// Access granted if EITHER auth or IP rules pass.
    Any,
    /// Access requires BOTH auth and IP rules to pass.
    All,
}

impl Satisfy {
    pub fn as_str(self) -> &'static str {
        match self {
            Satisfy::Any => "any",
            Satisfy::All => "all",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Directive {
    Allow,
    Deny,
}

impl Directive {
    pub fn as_str(self) -> &'static str {
        match self {
            Directive::Allow => "allow",
            Directive::Deny => "deny",
        }
    }
}

/// A basic-auth user as returned to the UI. The password hash is NEVER exposed.
#[derive(Debug, Clone, Serialize)]
pub struct AccessListUser {
    pub username: String,
    /// True when a password is stored (so the UI can show "set" without the hash).
    pub has_password: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccessListClient {
    pub directive: Directive,
    pub address: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccessList {
    pub id: i64,
    pub name: String,
    pub satisfy: Satisfy,
    pub pass_auth: bool,
    pub users: Vec<AccessListUser>,
    pub clients: Vec<AccessListClient>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccessListUserInput {
    pub username: String,
    /// Absent on update = keep the existing password for this username.
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccessListClientInput {
    pub directive: Directive,
    pub address: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccessListInput {
    pub name: String,
    #[serde(default = "default_satisfy")]
    pub satisfy: Satisfy,
    #[serde(default)]
    pub pass_auth: bool,
    #[serde(default)]
    pub users: Vec<AccessListUserInput>,
    #[serde(default)]
    pub clients: Vec<AccessListClientInput>,
}

fn default_satisfy() -> Satisfy {
    Satisfy::All
}

pub const MAX_ACL_NAME_LEN: usize = 100;
pub const MAX_ACL_USERS: usize = 200;
pub const MAX_ACL_CLIENTS: usize = 200;
/// bcrypt only hashes the first 72 bytes; reject longer to avoid silent truncation.
pub const MAX_PASSWORD_LEN: usize = 72;

/// Basic-auth username — goes into the htpasswd file as `username:hash`, so it
/// must not contain `:`, whitespace, or control characters.
pub fn validate_acl_username(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    let ok = !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'@'));
    if !ok {
        return Err(bad(
            "invalid_username",
            format!("'{raw}': username must be 1-64 chars of [A-Za-z0-9._@-]"),
        ));
    }
    Ok(s.to_string())
}

/// An IP allow/deny target: a bare IP, an IP/CIDR, or the literal `all`. This
/// value is interpolated into an `allow`/`deny` directive, so it is validated
/// strictly (never escaped).
pub fn validate_acl_address(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    if s == "all" {
        return Ok(s.to_string());
    }
    let bad_addr = || {
        bad(
            "invalid_address",
            format!("'{raw}' is not an IP, CIDR, or 'all'"),
        )
    };
    match s.split_once('/') {
        Some((ip, prefix)) => {
            let addr: std::net::IpAddr = ip.parse().map_err(|_| bad_addr())?;
            let max = if addr.is_ipv4() { 32 } else { 128 };
            let p: u8 = prefix.parse().map_err(|_| bad_addr())?;
            if p as u16 > max {
                return Err(bad_addr());
            }
            // Re-render canonically from the parsed parts (no raw passthrough).
            Ok(format!("{addr}/{p}"))
        }
        None => {
            let addr: std::net::IpAddr = s.parse().map_err(|_| bad_addr())?;
            Ok(addr.to_string())
        }
    }
}

// =============================================================== ip blocklist

/// A banned IP or CIDR (global `deny` rule). `reason` is UI metadata only — it
/// is NEVER written into the config (so it can't inject anything).
#[derive(Debug, Clone, Serialize)]
pub struct Ban {
    pub id: i64,
    pub address: String,
    pub reason: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BanInput {
    pub address: String,
    #[serde(default)]
    pub reason: Option<String>,
}

pub const MAX_BAN_REASON_LEN: usize = 200;

/// Validate a blocklist entry. The address is a bare IP or IP/CIDR (never
/// `all` — that would deny the whole internet). The reason is trimmed and
/// length-bounded; it stays out of the generated config entirely.
pub fn validate_ban(mut input: BanInput) -> Result<BanInput, ApiError> {
    let s = input.address.trim();
    if s == "all" {
        return Err(bad(
            "invalid_address",
            "'all' would block everyone — ban specific IPs or CIDRs",
        ));
    }
    input.address = validate_acl_address(s)?;
    input.reason = match input.reason.map(|r| r.trim().to_string()) {
        Some(r) if r.is_empty() => None,
        Some(r) if r.len() > MAX_BAN_REASON_LEN => {
            return Err(bad("invalid_reason", "reason is too long"));
        }
        other => other,
    };
    Ok(input)
}

/// Validate a bcrypt hash coming from an UNTRUSTED import. The hash is written
/// verbatim into an htpasswd line (`username:hash`), so a malformed value could
/// inject extra lines or break parsing — accept only the canonical bcrypt shape
/// (`$2[aby]$NN$` + 53 base64 chars, 60 total). This is the trust boundary for
/// imported password material.
pub fn validate_bcrypt_hash(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    let bytes = s.as_bytes();
    let shape_ok = bytes.len() == 60
        && bytes.starts_with(b"$2")
        && matches!(bytes[2], b'a' | b'b' | b'y')
        && bytes[3] == b'$'
        && bytes[4].is_ascii_digit()
        && bytes[5].is_ascii_digit()
        && bytes[6] == b'$'
        && bytes[7..]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'/'));
    if !shape_ok {
        return Err(bad(
            "invalid_password_hash",
            "a user password hash is not a valid bcrypt hash",
        ));
    }
    Ok(s.to_string())
}

/// Validate + normalize an access-list payload. Passwords are NOT hashed here
/// (the handler does that, preserving existing hashes on update).
pub fn validate_acl_input(mut input: AccessListInput) -> Result<AccessListInput, ApiError> {
    let name = input.name.trim().to_string();
    if name.is_empty() || name.len() > MAX_ACL_NAME_LEN || name.contains(['\n', '\r']) {
        return Err(bad("invalid_name", "invalid access-list name"));
    }
    input.name = name;

    if input.users.len() > MAX_ACL_USERS {
        return Err(bad("too_many_users", "too many users"));
    }
    let mut seen = std::collections::HashSet::new();
    for u in &mut input.users {
        u.username = validate_acl_username(&u.username)?;
        if !seen.insert(u.username.clone()) {
            return Err(bad(
                "duplicate_username",
                format!("duplicate username {}", u.username),
            ));
        }
        if let Some(p) = &u.password {
            if p.is_empty() {
                u.password = None; // "keep existing" sentinel
            } else if p.len() > MAX_PASSWORD_LEN {
                return Err(bad(
                    "password_too_long",
                    format!("password must be at most {MAX_PASSWORD_LEN} bytes"),
                ));
            }
        }
    }

    if input.clients.len() > MAX_ACL_CLIENTS {
        return Err(bad("too_many_clients", "too many IP rules"));
    }
    for c in &mut input.clients {
        c.address = validate_acl_address(&c.address)?;
    }

    // An access list with neither users nor clients is meaningless.
    if input.users.is_empty() && input.clients.is_empty() {
        return Err(bad(
            "empty_access_list",
            "an access list needs at least one user or one IP rule",
        ));
    }

    Ok(input)
}

// ====================================================== redirect / 404 hosts

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RedirectScheme {
    /// Preserve the incoming scheme ($scheme).
    Auto,
    Http,
    Https,
}

impl RedirectScheme {
    /// The scheme literal used in the generated `return` (or `$scheme`).
    pub fn as_target(self) -> &'static str {
        match self {
            RedirectScheme::Auto => "$scheme",
            RedirectScheme::Http => "http",
            RedirectScheme::Https => "https",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RedirectHost {
    pub id: i64,
    pub domains: Vec<String>,
    pub forward_scheme: RedirectScheme,
    pub forward_domain: String,
    pub forward_http_code: u16,
    pub preserve_path: bool,
    pub certificate_id: Option<i64>,
    pub force_ssl: bool,
    pub hsts: bool,
    pub hsts_subdomains: bool,
    pub http2: bool,
    pub block_exploits: bool,
    pub advanced_snippet: Option<String>,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedirectHostInput {
    pub domains: Vec<String>,
    #[serde(default = "default_redirect_scheme")]
    pub forward_scheme: RedirectScheme,
    pub forward_domain: String,
    #[serde(default = "default_redirect_code")]
    pub forward_http_code: u16,
    #[serde(default = "default_true")]
    pub preserve_path: bool,
    #[serde(default)]
    pub certificate_id: Option<i64>,
    #[serde(default)]
    pub force_ssl: bool,
    #[serde(default)]
    pub hsts: bool,
    #[serde(default)]
    pub hsts_subdomains: bool,
    #[serde(default = "default_true")]
    pub http2: bool,
    #[serde(default)]
    pub block_exploits: bool,
    #[serde(default)]
    pub advanced_snippet: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_redirect_scheme() -> RedirectScheme {
    RedirectScheme::Auto
}
fn default_redirect_code() -> u16 {
    301
}

#[derive(Debug, Clone, Serialize)]
pub struct DeadHost {
    pub id: i64,
    pub domains: Vec<String>,
    pub certificate_id: Option<i64>,
    pub force_ssl: bool,
    pub hsts: bool,
    pub hsts_subdomains: bool,
    pub http2: bool,
    pub advanced_snippet: Option<String>,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeadHostInput {
    pub domains: Vec<String>,
    #[serde(default)]
    pub certificate_id: Option<i64>,
    #[serde(default)]
    pub force_ssl: bool,
    #[serde(default)]
    pub hsts: bool,
    #[serde(default)]
    pub hsts_subdomains: bool,
    #[serde(default = "default_true")]
    pub http2: bool,
    #[serde(default)]
    pub advanced_snippet: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Validate the domains list shared by every host type.
fn validate_domains(raw: &[String]) -> Result<Vec<String>, ApiError> {
    if raw.is_empty() {
        return Err(bad("invalid_domain", "at least one domain is required"));
    }
    if raw.len() > MAX_DOMAINS_PER_HOST {
        return Err(bad("invalid_domain", "too many domains"));
    }
    let mut out = Vec::with_capacity(raw.len());
    for d in raw {
        let norm = validate_domain(d)?;
        if !out.contains(&norm) {
            out.push(norm);
        }
    }
    Ok(out)
}

pub fn validate_redirect_input(
    mut input: RedirectHostInput,
    allow_advanced_snippets: bool,
) -> Result<RedirectHostInput, ApiError> {
    input.domains = validate_domains(&input.domains)?;

    // forward_domain is interpolated into a `return` directive — validate it as
    // a strict domain (no scheme/path/injection). Wildcards make no sense here.
    let fd = validate_domain(&input.forward_domain)?;
    if fd.starts_with("*.") {
        return Err(bad(
            "invalid_forward_domain",
            "the redirect target cannot be a wildcard domain",
        ));
    }
    input.forward_domain = fd;

    if !(300..=308).contains(&input.forward_http_code) {
        return Err(bad(
            "invalid_redirect_code",
            "HTTP redirect code must be 300-308",
        ));
    }

    if let Some(s) = &input.advanced_snippet {
        input.advanced_snippet = normalize_snippet(s, allow_advanced_snippets)?;
    }
    Ok(input)
}

pub fn validate_dead_input(
    mut input: DeadHostInput,
    allow_advanced_snippets: bool,
) -> Result<DeadHostInput, ApiError> {
    input.domains = validate_domains(&input.domains)?;
    if let Some(s) = &input.advanced_snippet {
        input.advanced_snippet = normalize_snippet(s, allow_advanced_snippets)?;
    }
    Ok(input)
}

/// Trim + gate a snippet (shared by redirect/dead hosts). Empty → None.
fn normalize_snippet(s: &str, allow_advanced_snippets: bool) -> Result<Option<String>, ApiError> {
    if s.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(validate_snippet(s, allow_advanced_snippets)?))
    }
}

// ============================================================ streams (v2)

/// How a stream handles TLS on its incoming port.
///
/// `None` is a plain L4 forward (encrypted or not — Angie passes the bytes
/// through untouched). `Terminate` makes Angie decrypt using a panel-managed
/// certificate (`listen … ssl` + `$acme_cert_<name>`) and forward plaintext to
/// the backend — putting auto-renewed TLS in front of a plaintext TCP service.
/// TLS is TCP-only here (stream_ssl has no DTLS), so `Terminate` requires
/// `tcp` and forbids `udp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamTls {
    #[default]
    None,
    Terminate,
}

impl StreamTls {
    pub fn as_str(self) -> &'static str {
        match self {
            StreamTls::None => "none",
            StreamTls::Terminate => "terminate",
        }
    }

    /// Fail-safe parse for the repo: an unknown/NULL value degrades to `None`
    /// (a plain forward), never a surprise TLS listener.
    pub fn from_stored(s: &str) -> Self {
        match s {
            "terminate" => StreamTls::Terminate,
            _ => StreamTls::None,
        }
    }
}

/// A TCP/UDP port forward (Angie `stream {}` context), optionally terminating
/// TLS with a panel-managed certificate (see [`StreamTls`]).
#[derive(Debug, Clone, Serialize)]
pub struct Stream {
    pub id: i64,
    pub incoming_port: u16,
    pub forward_host: String,
    pub forward_port: u16,
    pub tcp: bool,
    pub udp: bool,
    #[serde(default)]
    pub tls: StreamTls,
    /// Certificate used when `tls == Terminate` (references `certificates.id`).
    #[serde(default)]
    pub certificate_id: Option<i64>,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamInput {
    pub incoming_port: u16,
    pub forward_host: String,
    pub forward_port: u16,
    #[serde(default = "default_true")]
    pub tcp: bool,
    #[serde(default)]
    pub udp: bool,
    #[serde(default)]
    pub tls: StreamTls,
    #[serde(default)]
    pub certificate_id: Option<i64>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

pub fn validate_stream_input(
    mut input: StreamInput,
    upstream_policy: &UpstreamPolicy,
) -> Result<StreamInput, ApiError> {
    if input.incoming_port == 0 {
        return Err(bad("invalid_port", "incoming port must be 1-65535"));
    }
    if input.forward_port == 0 {
        return Err(bad("invalid_port", "forward port must be 1-65535"));
    }
    if !input.tcp && !input.udp {
        return Err(bad(
            "no_protocol",
            "a stream must forward at least one of TCP or UDP",
        ));
    }
    match input.tls {
        StreamTls::Terminate => {
            // TLS termination is a TCP-only SSL listener.
            if !input.tcp {
                return Err(bad(
                    "tls_requires_tcp",
                    "TLS termination needs TCP; enable TCP or turn TLS off",
                ));
            }
            if input.udp {
                return Err(bad(
                    "tls_tcp_only",
                    "TLS termination cannot be combined with UDP (no DTLS)",
                ));
            }
            if input.certificate_id.is_none() {
                return Err(bad("cert_required", "TLS termination needs a certificate"));
            }
        }
        // A plain forward never carries a certificate reference — drop any
        // stray id so it can't dangle or resurrect on a later mode flip.
        StreamTls::None => input.certificate_id = None,
    }
    // Reuse the strict upstream validation (bare IP or hostname, SSRF guard).
    input.forward_host = validate_forward_host(&input.forward_host, upstream_policy)?;
    Ok(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> UpstreamPolicy {
        UpstreamPolicy {
            allow_loopback: false,
        }
    }

    #[test]
    fn domains_normalize_and_reject_injection() {
        assert_eq!(validate_domain("Example.COM.").unwrap(), "example.com");
        assert_eq!(validate_domain("*.example.com").unwrap(), "*.example.com");
        // IDN → punycode
        assert_eq!(
            validate_domain("почта.рф").unwrap(),
            "xn--80a1acny.xn--p1ai"
        );
        // Injection attempts must die, not escape.
        for evil in [
            "example.com;",
            "example.com }",
            "example.com{",
            "exa mple.com",
            "example.com\nserver_name evil.com",
            "$host.example.com",
            "example.com\"",
            "single-label",
            "*.*.example.com",
            "",
        ] {
            assert!(validate_domain(evil).is_err(), "should reject {evil:?}");
        }
    }

    #[test]
    fn forward_host_strictness() {
        assert_eq!(
            validate_forward_host("192.168.1.10", &policy()).unwrap(),
            "192.168.1.10"
        );
        assert_eq!(
            validate_forward_host("HomeAssistant", &policy()).unwrap(),
            "homeassistant"
        );
        assert_eq!(
            validate_forward_host("nas.lan", &policy()).unwrap(),
            "nas.lan"
        );
        assert_eq!(
            validate_forward_host("2a01:4f8::1", &policy()).unwrap(),
            "2a01:4f8::1"
        );
        for evil in [
            // The critic's canonical injection:
            "1.2.3.4:80; } location /fs { root /; autoindex on; ",
            "host:8080",
            "http://host",
            "host/path",
            "unix:/var/run/x.sock",
            "host;",
            "a b",
        ] {
            assert!(
                validate_forward_host(evil, &policy()).is_err(),
                "should reject {evil:?}"
            );
        }
    }

    #[test]
    fn loopback_upstreams_gated() {
        assert!(validate_forward_host("127.0.0.1", &policy()).is_err());
        assert!(validate_forward_host("::1", &policy()).is_err());
        assert!(validate_forward_host("169.254.1.1", &policy()).is_err());
        let permissive = UpstreamPolicy {
            allow_loopback: true,
        };
        assert!(validate_forward_host("127.0.0.1", &permissive).is_ok());
    }

    fn base_stream_input() -> StreamInput {
        StreamInput {
            incoming_port: 5432,
            forward_host: "192.168.1.20".into(),
            forward_port: 5432,
            tcp: true,
            udp: false,
            tls: StreamTls::None,
            certificate_id: None,
            enabled: true,
        }
    }

    #[test]
    fn stream_tls_terminate_rules() {
        // Terminate needs a certificate.
        let mut s = base_stream_input();
        s.tls = StreamTls::Terminate;
        assert_eq!(
            validate_stream_input(s, &policy()).unwrap_err().code,
            "cert_required"
        );

        // Terminate needs TCP.
        let mut s = base_stream_input();
        s.tls = StreamTls::Terminate;
        s.certificate_id = Some(1);
        s.tcp = false;
        s.udp = true;
        assert_eq!(
            validate_stream_input(s, &policy()).unwrap_err().code,
            "tls_requires_tcp"
        );

        // Terminate cannot ride UDP (no DTLS).
        let mut s = base_stream_input();
        s.tls = StreamTls::Terminate;
        s.certificate_id = Some(1);
        s.udp = true;
        assert_eq!(
            validate_stream_input(s, &policy()).unwrap_err().code,
            "tls_tcp_only"
        );

        // Valid terminate stream passes.
        let mut s = base_stream_input();
        s.tls = StreamTls::Terminate;
        s.certificate_id = Some(1);
        let out = validate_stream_input(s, &policy()).unwrap();
        assert_eq!(out.tls, StreamTls::Terminate);
        assert_eq!(out.certificate_id, Some(1));
    }

    #[test]
    fn stream_none_drops_stray_cert() {
        // A plain forward must never keep a certificate reference.
        let mut s = base_stream_input();
        s.certificate_id = Some(42);
        let out = validate_stream_input(s, &policy()).unwrap();
        assert_eq!(out.certificate_id, None);
    }

    #[test]
    fn location_paths() {
        assert_eq!(validate_location_path("/api/v1").unwrap(), "/api/v1");
        for evil in [
            "api",
            "/x { root /; } location /y",
            "/x;",
            "/x$",
            "/a/../b",
            "/x\n",
        ] {
            assert!(
                validate_location_path(evil).is_err(),
                "should reject {evil:?}"
            );
        }
    }

    #[test]
    fn snippets_require_optin() {
        let input = ProxyHostInput {
            domains: vec!["a.example.com".into()],
            forward_scheme: Scheme::Http,
            forward_host: "10.0.0.1".into(),
            forward_port: 80,
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
            advanced_snippet: Some("client_max_body_size 100m;".into()),
            rate_limit: RateLimit::default(),
            upstream: Upstream::default(),
            mtls: Mtls::default(),
            forward_auth: ForwardAuth::default(),
            enabled: true,
        };
        let err = validate_host_input(input.clone(), false, &policy()).unwrap_err();
        assert_eq!(err.code, "snippets_disabled");
        assert!(validate_host_input(input, true, &policy()).is_ok());
    }

    fn cert_input(name: &str, domains: &[&str], challenge: Challenge) -> CertificateInput {
        CertificateInput {
            name: name.into(),
            domains: domains.iter().map(|s| s.to_string()).collect(),
            challenge,
            key_type: KeyType::Ecdsa,
            email: None,
            staging: false,
        }
    }

    #[test]
    fn cert_name_is_identifier_safe() {
        assert_eq!(validate_cert_name("my_cert_1").unwrap(), "my_cert_1");
        for evil in [
            "My-Cert",
            "cert name",
            "cert;",
            "café",
            "",
            "a".repeat(33).as_str(),
        ] {
            assert!(validate_cert_name(evil).is_err(), "should reject {evil:?}");
        }
    }

    #[test]
    fn mtls_validation() {
        let ca = "-----BEGIN CERTIFICATE-----\nMIIBdata==\n-----END CERTIFICATE-----";
        // A structurally valid PEM is accepted and trimmed.
        let ok = validate_mtls(Mtls {
            ca_pem: Some(format!("  {ca}  ")),
            optional: true,
        })
        .unwrap();
        assert_eq!(ok.ca_pem.as_deref(), Some(ca));
        assert!(ok.active());

        // Empty / absent → inactive default.
        assert!(!validate_mtls(Mtls {
            ca_pem: Some("   ".into()),
            optional: false,
        })
        .unwrap()
        .active());

        // Not a certificate block, or mismatched markers, or junk chars → rejected.
        for bad_pem in [
            "not a cert",
            "-----BEGIN CERTIFICATE-----\nonly a begin",
            "-----BEGIN CERTIFICATE-----\n<script>\n-----END CERTIFICATE-----",
        ] {
            assert!(
                validate_mtls(Mtls {
                    ca_pem: Some(bad_pem.into()),
                    optional: false,
                })
                .is_err(),
                "should reject {bad_pem:?}"
            );
        }
    }

    #[test]
    fn forward_auth_validation() {
        // A full config is accepted and normalized (host lowercased, deduped headers).
        let ok = validate_forward_auth(
            ForwardAuth {
                enabled: true,
                verify_url: "http://10.0.0.5:9091/api/verify".into(),
                sign_in_url: Some("https://auth.example.com".into()),
                copy_headers: vec![
                    "Remote-User".into(),
                    "Remote-User".into(),
                    "Remote-Groups".into(),
                ],
            },
            &policy(),
        )
        .unwrap();
        assert_eq!(ok.verify_url, "http://10.0.0.5:9091/api/verify");
        assert_eq!(ok.sign_in_url.as_deref(), Some("https://auth.example.com"));
        assert_eq!(ok.copy_headers, vec!["Remote-User", "Remote-Groups"]);
        assert!(ok.active());

        // Disabled → inactive default regardless of the other fields.
        assert!(!validate_forward_auth(
            ForwardAuth {
                enabled: false,
                verify_url: "http://10.0.0.5:9091/verify".into(),
                ..Default::default()
            },
            &policy(),
        )
        .unwrap()
        .active());

        // Enabled needs a verify URL.
        assert!(validate_forward_auth(
            ForwardAuth {
                enabled: true,
                ..Default::default()
            },
            &policy(),
        )
        .is_err());

        // The verify endpoint is SSRF-guarded (loopback blocked by default).
        assert_eq!(
            validate_forward_auth(
                ForwardAuth {
                    enabled: true,
                    verify_url: "http://127.0.0.1:9091/verify".into(),
                    ..Default::default()
                },
                &policy(),
            )
            .unwrap_err()
            .code,
            "forbidden_forward_host"
        );

        // Injection / malformed URLs and header names are rejected.
        for evil in [
            "http://10.0.0.5:9091/verify;return 200",
            "http://10.0.0.5:9091/ver ify",
            "ftp://10.0.0.5/verify",
            "http://10.0.0.5:9091/a/../b",
            "10.0.0.5/verify",
            "http://10.0.0.5:99999/verify",
        ] {
            assert!(
                validate_forward_auth(
                    ForwardAuth {
                        enabled: true,
                        verify_url: evil.into(),
                        ..Default::default()
                    },
                    &policy(),
                )
                .is_err(),
                "should reject verify_url {evil:?}"
            );
        }
        for bad_header in ["Remote User", "Remote:User", "-bad", "x$y"] {
            assert!(
                validate_forward_auth(
                    ForwardAuth {
                        enabled: true,
                        verify_url: "http://10.0.0.5:9091/verify".into(),
                        copy_headers: vec![bad_header.into()],
                        ..Default::default()
                    },
                    &policy(),
                )
                .is_err(),
                "should reject header {bad_header:?}"
            );
        }
    }

    #[test]
    fn upstream_validation() {
        // A valid pool with an extra server passes and keeps its values.
        let up = validate_upstream(
            Upstream {
                method: BalanceMethod::LeastConn,
                primary_weight: 2,
                max_fails: 3,
                fail_timeout_secs: 15,
                servers: vec![UpstreamServer {
                    host: "10.0.0.2".into(),
                    port: 8080,
                    weight: 5,
                    backup: true,
                    down: false,
                }],
            },
            &policy(),
        )
        .unwrap();
        assert_eq!(up.servers[0].host, "10.0.0.2");

        // ip_hash + a backup server is rejected (Angie forbids the combo).
        let err = validate_upstream(
            Upstream {
                method: BalanceMethod::IpHash,
                servers: vec![UpstreamServer {
                    host: "10.0.0.2".into(),
                    port: 8080,
                    weight: 1,
                    backup: true,
                    down: false,
                }],
                ..Default::default()
            },
            &policy(),
        )
        .unwrap_err();
        assert_eq!(err.code, "invalid_upstream");

        // An injected server host is rejected via the SSRF-guarded validator.
        assert!(validate_upstream(
            Upstream {
                servers: vec![UpstreamServer {
                    host: "1.2.3.4; } location /x { root /; ".into(),
                    port: 80,
                    weight: 1,
                    backup: false,
                    down: false,
                }],
                ..Default::default()
            },
            &policy(),
        )
        .is_err());

        // Out-of-range weight / fail_timeout are rejected.
        assert!(validate_upstream(
            Upstream {
                primary_weight: MAX_WEIGHT + 1,
                ..Default::default()
            },
            &policy(),
        )
        .is_err());
        assert!(validate_upstream(
            Upstream {
                fail_timeout_secs: 0,
                ..Default::default()
            },
            &policy(),
        )
        .is_err());
    }

    #[test]
    fn rate_limit_validation() {
        // Disabled → flattened to default regardless of stale numbers.
        let flat = validate_rate_limit(RateLimit {
            enabled: false,
            rps: 99,
            burst: 99,
            nodelay: true,
            conn: 99,
        })
        .unwrap();
        assert_eq!(flat, RateLimit::default());

        // Enabled but no actual limit → rejected.
        let err = validate_rate_limit(RateLimit {
            enabled: true,
            ..Default::default()
        })
        .unwrap_err();
        assert_eq!(err.code, "invalid_rate_limit");

        // burst/nodelay are cleared when there is no request rate (conn-only).
        let conn_only = validate_rate_limit(RateLimit {
            enabled: true,
            rps: 0,
            burst: 50,
            nodelay: true,
            conn: 5,
        })
        .unwrap();
        assert_eq!(conn_only.burst, 0);
        assert!(!conn_only.nodelay);
        assert_eq!(conn_only.conn, 5);

        // Absurd values are rejected.
        assert!(validate_rate_limit(RateLimit {
            enabled: true,
            rps: MAX_RATE_RPS + 1,
            ..Default::default()
        })
        .is_err());
    }

    #[test]
    fn bcrypt_hash_import_guard() {
        // A canonical bcrypt hash (60 chars, $2b$NN$ + 53 base64) is accepted.
        let good = "$2b$12$abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0";
        assert_eq!(good.len(), 60);
        assert_eq!(validate_bcrypt_hash(good).unwrap(), good);
        // Anything off the canonical shape is rejected — especially a newline
        // (would inject a second htpasswd line) or a `:` (breaks user:hash).
        let with_colon = "$2b$12$abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ:";
        assert_eq!(with_colon.len(), 60);
        for evil in [
            "not-a-hash",
            "",
            "$2b$12$short",
            "$2b$12$abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0\nroot:x",
            "$1$md5salt$xxxxxxxxxxxxxxxxxxxxxx",
            with_colon,
        ] {
            assert!(
                validate_bcrypt_hash(evil).is_err(),
                "should reject {evil:?}"
            );
        }
    }

    #[test]
    fn cert_validation_normalizes_and_gates_wildcard() {
        // Normalizes domains, keeps http-01 for plain domains.
        let c =
            validate_cert_input(cert_input("web", &["App.Example.com"], Challenge::Http)).unwrap();
        assert_eq!(c.domains, vec!["app.example.com"]);

        // Wildcard with http-01 is rejected; with dns-01 it is allowed.
        let err =
            validate_cert_input(cert_input("w", &["*.example.com"], Challenge::Http)).unwrap_err();
        assert_eq!(err.code, "wildcard_needs_dns");
        assert!(validate_cert_input(cert_input("w", &["*.example.com"], Challenge::Dns)).is_ok());

        // Injection in a domain still dies here.
        assert!(validate_cert_input(cert_input("w", &["a.com; }"], Challenge::Http)).is_err());
    }

    #[test]
    fn acl_address_strictness() {
        assert_eq!(validate_acl_address("all").unwrap(), "all");
        assert_eq!(validate_acl_address("192.168.1.1").unwrap(), "192.168.1.1");
        assert_eq!(validate_acl_address("10.0.0.0/8").unwrap(), "10.0.0.0/8");
        assert_eq!(
            validate_acl_address("2a01:4f8::/32").unwrap(),
            "2a01:4f8::/32"
        );
        for evil in [
            "1.2.3.4; deny all",
            "1.2.3.4 }",
            "10.0.0.0/33",
            "10.0.0.0/999",
            "not-an-ip",
            "$remote_addr",
            "",
        ] {
            assert!(
                validate_acl_address(evil).is_err(),
                "should reject {evil:?}"
            );
        }
    }

    #[test]
    fn acl_username_rejects_htpasswd_delimiter() {
        assert_eq!(validate_acl_username("bob.smith_1").unwrap(), "bob.smith_1");
        for evil in ["a:b", "a b", "a\nb", "", "user;"] {
            assert!(
                validate_acl_username(evil).is_err(),
                "should reject {evil:?}"
            );
        }
    }

    #[test]
    fn redirect_validation() {
        let base = || RedirectHostInput {
            domains: vec!["old.example.com".into()],
            forward_scheme: RedirectScheme::Https,
            forward_domain: "New.Example.com".into(),
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
        };
        let ok = validate_redirect_input(base(), false).unwrap();
        assert_eq!(ok.forward_domain, "new.example.com"); // normalized

        // Bad redirect code.
        let mut bad_code = base();
        bad_code.forward_http_code = 200;
        assert_eq!(
            validate_redirect_input(bad_code, false).unwrap_err().code,
            "invalid_redirect_code"
        );

        // Injection in the forward domain is rejected.
        let mut evil = base();
        evil.forward_domain = "x.com; return 200 \"pwned\"".into();
        assert!(validate_redirect_input(evil, false).is_err());

        // Wildcard target rejected.
        let mut wild = base();
        wild.forward_domain = "*.example.com".into();
        assert_eq!(
            validate_redirect_input(wild, false).unwrap_err().code,
            "invalid_forward_domain"
        );
    }

    #[test]
    fn acl_input_requires_content() {
        let empty = AccessListInput {
            name: "office".into(),
            satisfy: Satisfy::All,
            pass_auth: false,
            users: vec![],
            clients: vec![],
        };
        assert_eq!(
            validate_acl_input(empty).unwrap_err().code,
            "empty_access_list"
        );
    }
}
