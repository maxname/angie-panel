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
    pub locations: Vec<CustomLocation>,
    pub advanced_snippet: Option<String>,
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
    pub locations: Vec<CustomLocation>,
    #[serde(default)]
    pub advanced_snippet: Option<String>,
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
            return Err(bad("invalid_domain", format!("invalid label '{label}' in {raw}")));
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
                v6.is_loopback()
                    || v6.is_unspecified()
                    || (v6.segments()[0] & 0xffc0) == 0xfe80 // link-local
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
    let s = raw.trim().to_string();
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
    let s = raw.trim().to_string();
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

fn validate_snippet(
    raw: &str,
    allow_advanced_snippets: bool,
) -> Result<String, ApiError> {
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

    // hsts_subdomains only makes sense with hsts, http2/force_ssl only with
    // TLS — kept as-is in the DB; the generator applies the actual gating.
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
        assert_eq!(validate_domain("почта.рф").unwrap(), "xn--80a1acny.xn--p1ai");
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
            assert!(validate_location_path(evil).is_err(), "should reject {evil:?}");
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
            locations: vec![],
            advanced_snippet: Some("client_max_body_size 100m;".into()),
            enabled: true,
        };
        let err = validate_host_input(input.clone(), false, &policy()).unwrap_err();
        assert_eq!(err.code, "snippets_disabled");
        assert!(validate_host_input(input, true, &policy()).is_ok());
    }
}
