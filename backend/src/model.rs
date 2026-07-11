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
    pub force_ssl: bool,
    pub hsts: bool,
    pub hsts_subdomains: bool,
    pub trust_forwarded_proto: bool,
    pub certificate_id: Option<i64>,
    pub access_list_id: Option<i64>,
    pub locations: Vec<CustomLocation>,
    pub advanced_snippet: Option<String>,
    pub rate_limit: RateLimit,
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

/// A TCP/UDP port forward (v1: plain forwarding, no TLS termination — that
/// would need stream-context ACME, a follow-up).
#[derive(Debug, Clone, Serialize)]
pub struct Stream {
    pub id: i64,
    pub incoming_port: u16,
    pub forward_host: String,
    pub forward_port: u16,
    pub tcp: bool,
    pub udp: bool,
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
            force_ssl: false,
            hsts: false,
            hsts_subdomains: false,
            trust_forwarded_proto: false,
            certificate_id: None,
            access_list_id: None,
            locations: vec![],
            advanced_snippet: Some("client_max_body_size 100m;".into()),
            rate_limit: RateLimit::default(),
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
