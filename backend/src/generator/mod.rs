//! Config generation (PLAN.md §4/§5) + the MANAGED-BY header machinery.
//!
//! The database is the source of truth; the files under `/etc/angie/http.d`
//! are a *deterministic* projection of it. Determinism matters twice over:
//! golden tests pin the exact bytes, and the drift detector (§2.2) compares a
//! recomputed hash against the one embedded in each file's header — any
//! non-determinism would produce spurious "manually edited" alerts.
//!
//! We build the config text by hand rather than through a template engine.
//! Angie config is line-oriented and every interpolated value has already been
//! reduced to a safe charset by `model.rs` (level-1 validation, PLAN.md §7);
//! plain string building keeps the byte-for-byte output obvious to a reviewer
//! and free of hidden whitespace an engine might introduce. Whatever we emit is
//! re-checked by the level-2 linter (`lint::check_fileset`) before it is ever
//! written, so the generator is a *convenience* layer, not the trust boundary.

// The generator is a work-package contract consumed by the apply pipeline
// (`apply/`) and the API layer, which land in sibling work packages. Until they
// are wired up, the non-test bin build sees this public surface as unused; the
// test target exercises all of it. Mirror the crate's existing pattern
// (`config.rs` uses targeted `#[allow(dead_code)]` for the same reason).
#![allow(dead_code)]

pub mod lint;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::model::{
    Ban, CustomLocation, DeadHost, ProxyHost, RateLimit, RedirectHost, Scheme, Stream, StreamTls,
};

/// FileSet keys with this prefix are stream configs destined for `stream.d/`
/// (Angie's separate `stream {}` context), not `http.d/`.
pub const STREAM_PREFIX: &str = "stream.d/";

/// Everything the generator needs; assembled by the API layer from DB rows
/// and settings. Filenames map 1:1 to /etc/angie/http.d entries.
pub struct GeneratorInput {
    pub hosts: Vec<ProxyHost>,
    pub settings: EffectiveSettings,
    /// Read-only shared snippet files (block-exploits.conf, cache-assets.conf).
    pub snippets_dir: std::path::PathBuf,
    /// Where the status API server listens (127.0.0.1:<port>).
    pub status_port: u16,
    /// Directory served for the custom-HTML default site.
    pub public_dir: std::path::PathBuf,
    /// Certificates referenced by hosts via `certificate_id`.
    ///
    /// A host's `certificate_id` is resolved against this list to obtain the
    /// acme_client `name` (which is interpolated into `$acme_cert_<name>`) and
    /// the `ready` flag. See [`Certificate`] for why `ready` gates 443
    /// generation.
    pub certificates: Vec<Certificate>,
    /// Directory for the ACME collector unix sockets (one per certificate).
    /// The root helper ensures it exists before reload.
    pub acme_socket_dir: std::path::PathBuf,
    /// Access lists referenced by hosts via `access_list_id`.
    pub access_lists: Vec<AccessList>,
    /// Managed config dir — where generated files (incl. htpasswd) live; used
    /// to build the absolute `auth_basic_user_file` path.
    pub http_d_dir: std::path::PathBuf,
    /// Redirection hosts (301/302/… to another domain).
    pub redirect_hosts: Vec<RedirectHost>,
    /// 404 (dead) hosts.
    pub dead_hosts: Vec<DeadHost>,
    /// TCP/UDP port forwards (stream context). Emitted with the STREAM_PREFIX
    /// so the apply pipeline routes them to stream.d.
    pub streams: Vec<Stream>,
    /// Blocked IPs/CIDRs — emitted as http-scope `deny` rules (03-bans.conf).
    pub bans: Vec<Ban>,
}

/// An access list reduced to what the generator needs. Usernames carry their
/// bcrypt hashes (written verbatim into the htpasswd file).
#[derive(Debug, Clone)]
pub struct AccessList {
    pub id: i64,
    /// "any" | "all".
    pub satisfy: String,
    /// When false, the client's Authorization header is stripped from upstream.
    pub pass_auth: bool,
    /// (username, bcrypt-hash) pairs for basic auth.
    pub users: Vec<(String, String)>,
    /// (directive, address) pairs — directive is "allow"|"deny".
    pub clients: Vec<(String, String)>,
}

/// Let's Encrypt ACME directory endpoints.
pub const LE_PROD_DIRECTORY: &str = "https://acme-v02.api.letsencrypt.org/directory";
pub const LE_STAGING_DIRECTORY: &str = "https://acme-staging-v02.api.letsencrypt.org/directory";

/// A certificate row, reduced to exactly what the generator needs.
///
/// `ready` encodes the first-issuance state machine from PLAN.md §4/§5: the
/// `$acme_cert_<name>` variable is *empty* until Angie has actually obtained
/// the certificate for the first time. A freshly created HTTPS host would
/// therefore pass `angie -t` but fail every TLS handshake. To avoid serving
/// TLS errors instead of the site, a host whose certificate is not yet `ready`
/// is rendered HTTP-only (no `:443` server, no force-ssl redirect). Once the
/// panel observes `certificate: valid` via `/status/http/acme_clients/<name>`
/// it flips `ready` and re-applies.
#[derive(Debug, Clone)]
pub struct Certificate {
    pub id: i64,
    /// acme_client name; `^[a-z0-9_]{1,32}$` (validated on creation).
    pub name: String,
    /// Authoritative SAN — emitted as the collector block's `server_name`,
    /// so issuance is independent of which hosts attach the cert (PLAN.md §5).
    pub domains: Vec<String>,
    /// "http" | "dns" | "alpn".
    pub challenge: String,
    /// "ecdsa" | "rsa".
    pub key_type: String,
    pub email: Option<String>,
    /// Use the Let's Encrypt staging CA (untrusted, high rate limits).
    pub staging: bool,
    /// Pause renewal without deleting state (acme_client `enabled=off`).
    pub enabled: bool,
    /// True once Angie has issued the certificate at least once.
    pub ready: bool,
}

#[derive(Debug, Clone)]
pub struct EffectiveSettings {
    pub default_site: DefaultSite,
    pub ipv6_enabled: bool,
    /// Nameservers for the `resolver` directive (from resolv.conf or override).
    pub resolvers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefaultSite {
    NotFound,
    Drop444,
    Redirect(String),
    Html,
}

/// filename → file body (WITHOUT the MANAGED-BY header; wrap with
/// `with_header` before writing to disk).
pub type FileSet = BTreeMap<String, String>;

/// Directory (relative to keys_zone) where Angie caches assets for hosts with
/// `cache_assets`. Owned by Angie's workers; created by the root helper.
const CACHE_ASSETS_PATH: &str = "/var/cache/angie-panel-assets";

pub fn generate(input: &GeneratorInput) -> anyhow::Result<FileSet> {
    let mut files = FileSet::new();

    files.insert("00-panel.conf".to_string(), gen_panel(input));
    // Global IP blocklist (http-scope `deny`). Emitted early so the deny rules
    // are inherited by every server that has no access rules of its own.
    if let Some(body) = gen_bans(input) {
        files.insert("03-bans.conf".to_string(), body);
    }
    files.insert("05-default.conf".to_string(), gen_default(input)?);
    files.insert("10-acme.conf".to_string(), gen_acme(input));
    // Rate-limit zones: emitted before 20-host-* so the zones exist in http
    // context before any server block references them.
    if let Some(body) = gen_rate_limits(input) {
        files.insert("15-rate-limits.conf".to_string(), body);
    }

    // Index certificates by id so hosts can resolve name + readiness in O(1).
    let certs: BTreeMap<i64, &Certificate> = input.certificates.iter().map(|c| (c.id, c)).collect();

    // Sort hosts by id for a stable file order (BTreeMap keys the filenames,
    // but sorting also makes any per-run logging deterministic).
    let mut hosts: Vec<&ProxyHost> = input.hosts.iter().collect();
    hosts.sort_by_key(|h| h.id);

    for host in hosts {
        // enabled=false → no file at all; traffic falls through to the
        // default_server (PLAN.md §4).
        if !host.enabled {
            tracing::debug!(host_id = host.id, "host disabled; no config file emitted");
            continue;
        }
        let cert = host.certificate_id.and_then(|cid| certs.get(&cid).copied());
        let (filename, body) = gen_host(host, cert, input)?;
        files.insert(filename, body);
        // Materialize the mTLS client-CA bundle — but only when the host is
        // actually serving HTTPS (a ready cert produced a :443 block that
        // references it). QUIC/TLS-only, like every other 443 feature.
        let https = matches!(cert, Some(c) if c.ready);
        if https && host.mtls.active() {
            if let Some(pem) = &host.mtls.ca_pem {
                files.insert(client_ca_name(host.id), pem.clone());
            }
        }
    }

    // Redirection hosts (30-*) and 404 hosts (40-*). Disabled ones emit no
    // file, matching proxy hosts.
    let mut redirects: Vec<&RedirectHost> = input.redirect_hosts.iter().collect();
    redirects.sort_by_key(|r| r.id);
    for rh in redirects {
        if !rh.enabled {
            continue;
        }
        let cert = rh.certificate_id.and_then(|cid| certs.get(&cid).copied());
        let (filename, body) = gen_redirect(rh, cert, input);
        files.insert(filename, body);
    }
    let mut deads: Vec<&DeadHost> = input.dead_hosts.iter().collect();
    deads.sort_by_key(|d| d.id);
    for dh in deads {
        if !dh.enabled {
            continue;
        }
        let cert = dh.certificate_id.and_then(|cid| certs.get(&cid).copied());
        let (filename, body) = gen_dead(dh, cert, input);
        files.insert(filename, body);
    }

    // Streams (stream.d/stream-<id>.conf). Keyed with STREAM_PREFIX so the
    // apply pipeline syncs them to stream.d, not http.d.
    let mut streams: Vec<&Stream> = input.streams.iter().collect();
    streams.sort_by_key(|s| s.id);
    for s in streams {
        if !s.enabled {
            continue;
        }
        // A TLS-terminating stream needs a real certificate to name in
        // `$acme_cert_<name>`. If the reference dangles (guarded against by
        // validation + the cert-in-use delete check, but be defensive) we skip
        // the stream entirely rather than silently downgrade it to plaintext.
        let terminate_cert = match s.tls {
            StreamTls::Terminate => {
                match s.certificate_id.and_then(|cid| certs.get(&cid).copied()) {
                    Some(cert) => Some(cert),
                    None => {
                        tracing::warn!(
                            stream_id = s.id,
                            "TLS-terminating stream references a missing certificate; skipping"
                        );
                        continue;
                    }
                }
            }
            StreamTls::None => None,
        };

        let mut body = String::new();
        let _ = writeln!(body, "server {{");
        if let Some(cert) = terminate_cert {
            // TCP-only SSL listener (validated: terminate ⇒ tcp, no udp). The
            // `$acme_cert_<name>` variable is lazy, so `angie -t` passes before
            // issuance and Angie hot-loads the cert once the shared http-context
            // ACME collector obtains it — no readiness gate, no reload.
            let _ = writeln!(body, "    listen {} ssl;", s.incoming_port);
            let _ = writeln!(body, "    ssl_certificate     $acme_cert_{};", cert.name);
            let _ = writeln!(
                body,
                "    ssl_certificate_key $acme_cert_key_{};",
                cert.name
            );
        } else {
            if s.tcp {
                let _ = writeln!(body, "    listen {};", s.incoming_port);
            }
            if s.udp {
                let _ = writeln!(body, "    listen {} udp;", s.incoming_port);
            }
        }
        // forward_host/port were validated (bare IP/hostname + u16).
        let _ = writeln!(
            body,
            "    proxy_pass {}:{};",
            s.forward_host, s.forward_port
        );
        let _ = writeln!(body, "}}");
        files.insert(format!("{STREAM_PREFIX}stream-{}.conf", s.id), body);
    }

    // htpasswd files for access lists that have basic-auth users. These are
    // NOT *.conf, so Angie's `include *.conf` never loads them; the apply
    // pipeline manages them via the MANAGED-BY header (nginx auth_basic skips
    // '#' comment lines). Only emit lists that are actually referenced by a
    // host, to avoid stray files.
    let referenced: std::collections::HashSet<i64> = input
        .hosts
        .iter()
        .filter_map(|h| h.access_list_id)
        .collect();
    let mut lists: Vec<&AccessList> = input
        .access_lists
        .iter()
        .filter(|l| referenced.contains(&l.id) && !l.users.is_empty())
        .collect();
    lists.sort_by_key(|l| l.id);
    for list in lists {
        let mut body = String::new();
        for (username, hash) in &list.users {
            let _ = writeln!(body, "{username}:{hash}");
        }
        files.insert(htpasswd_name(list.id), body);
    }

    Ok(files)
}

fn htpasswd_name(list_id: i64) -> String {
    format!("access-{list_id}.htpasswd")
}

// ------------------------------------------------------------- 00-panel.conf

fn gen_panel(input: &GeneratorInput) -> String {
    let mut out = String::new();

    // resolver: required by upstreams that use hostnames and by the ACME
    // module. Skipping it when empty is intentional (an empty `resolver;` is a
    // config error) — but we log so the operator can spot a missing resolv.conf.
    if input.settings.resolvers.is_empty() {
        tracing::warn!("no resolvers configured; omitting the `resolver` directive");
    } else {
        let list = input.settings.resolvers.join(" ");
        let _ = writeln!(out, "resolver {list} valid=300s;");
        out.push('\n');
    }

    // WebSocket upgrade map: hosts with websocket support emit
    // `proxy_set_header Connection $connection_upgrade;`. The base packaged
    // angie.conf does not define this variable, so we declare the standard
    // map here at http scope (the linter allows `map`).
    let _ = writeln!(
        out,
        "map $http_upgrade $connection_upgrade {{\n    \
         default upgrade;\n    '' close;\n}}"
    );
    out.push('\n');

    // Shared cache for hosts with cache_assets. The `assets` keys_zone is
    // referenced by the packaged cache-assets.conf snippet, which is included
    // inside each caching host's `location /`.
    let _ = writeln!(
        out,
        "proxy_cache_path {CACHE_ASSETS_PATH} levels=1:2 keys_zone=assets:10m \
         max_size=1g inactive=60m;"
    );
    out.push('\n');

    // Status/monitoring server, loopback only. `api_config_files on` exposes the
    // actually-loaded config for drift detection (PLAN.md §2.2).
    let _ = writeln!(out, "server {{");
    let _ = writeln!(out, "    listen 127.0.0.1:{};", input.status_port);
    let _ = writeln!(out, "    location /status/ {{");
    let _ = writeln!(out, "        api /status/;");
    let _ = writeln!(out, "        api_config_files on;");
    let _ = writeln!(out, "    }}");
    let _ = writeln!(out, "}}");

    out
}

// ----------------------------------------------------------- 05-default.conf

/// The catch-all `default_server` for :80 and :443. Requests whose Host does
/// not match any managed host (or that hit an IP directly) land here.
fn gen_default(input: &GeneratorInput) -> anyhow::Result<String> {
    let ipv6 = input.settings.ipv6_enabled;
    let mut out = String::new();

    // :80 default — carries the actual default-site behaviour.
    let _ = writeln!(out, "server {{");
    let _ = writeln!(out, "    listen 80 default_server;");
    if ipv6 {
        let _ = writeln!(out, "    listen [::]:80 default_server;");
    }
    let _ = writeln!(out, "    server_name _;");
    default_site_body(&mut out, input)?;
    let _ = writeln!(out, "}}");
    out.push('\n');

    // :443 default — `ssl_reject_handshake on` means we present NO certificate
    // and abort the TLS handshake for unknown SNI, rather than shipping a dummy
    // self-signed cert (PLAN.md §4). There is deliberately no default site on
    // 443: an unknown-SNI client never completes TLS, so there is nothing to
    // serve.
    let _ = writeln!(out, "server {{");
    let _ = writeln!(out, "    listen 443 ssl default_server;");
    if ipv6 {
        let _ = writeln!(out, "    listen [::]:443 ssl default_server;");
    }
    let _ = writeln!(out, "    server_name _;");
    let _ = writeln!(out, "    ssl_reject_handshake on;");
    let _ = writeln!(out, "}}");

    Ok(out)
}

fn default_site_body(out: &mut String, input: &GeneratorInput) -> anyhow::Result<()> {
    match &input.settings.default_site {
        DefaultSite::NotFound => {
            let _ = writeln!(out, "    return 404;");
        }
        DefaultSite::Drop444 => {
            // 444 = close the connection without a response (nginx/Angie
            // special code). Good for hiding the panel from scanners.
            let _ = writeln!(out, "    return 444;");
        }
        DefaultSite::Redirect(url) => {
            // The URL is validated upstream, but this string is interpolated
            // verbatim into a directive, so we defensively re-validate it here
            // too: it must not contain anything that could terminate the
            // `return` directive or inject a new one.
            let url = sanitize_redirect_url(url)?;
            let _ = writeln!(out, "    return 301 {url};");
        }
        DefaultSite::Html => {
            let public = path_str(&input.public_dir)?;
            let _ = writeln!(out, "    root {public};");
            let _ = writeln!(out, "    try_files /index.html =404;");
        }
    }
    Ok(())
}

/// Defence-in-depth validation of a redirect target that gets interpolated into
/// `return 301 <url>;`. Angie treats whitespace, `;`, `{`, `}`, quotes and
/// newlines as token/directive boundaries, so any of them would let the value
/// break out of the directive. We allow only a conservative URL charset.
fn sanitize_redirect_url(url: &str) -> anyhow::Result<String> {
    let u = url.trim();
    if u.is_empty() {
        anyhow::bail!("empty default-site redirect URL");
    }
    if u.len() > 2048 {
        anyhow::bail!("default-site redirect URL too long");
    }
    // Must be an absolute http(s) URL (a relative one is meaningless for a
    // catch-all redirect and easier to smuggle control chars through).
    if !(u.starts_with("http://") || u.starts_with("https://")) {
        anyhow::bail!("default-site redirect URL must start with http:// or https://");
    }
    // Reject any Angie-significant or control character outright.
    let bad = |c: char| {
        c.is_whitespace()
            || c.is_control()
            || matches!(c, ';' | '{' | '}' | '"' | '\'' | '\\' | '$' | '#')
    };
    if let Some(c) = u.chars().find(|&c| bad(c)) {
        anyhow::bail!("default-site redirect URL contains illegal character {c:?}");
    }
    Ok(u.to_string())
}

// -------------------------------------------------------------- 10-acme.conf

/// ACME clients + collector server blocks (PLAN.md §5).
///
/// For each certificate we emit an `acme_client` (the issuance policy) plus a
/// "collector" `server` block that listens on a unix socket and carries
/// `acme <name>` + `server_name <domains>`. Angie documents this pattern for
/// blocks that "only collect domain names": the collector defines the cert's
/// SAN authoritatively and drives issuance, while it never serves real traffic
/// and never clashes with the host's :80/:443 blocks (which reference only
/// `$acme_cert_<name>`). This also breaks the first-issuance deadlock: the
/// collector (with `acme`) is ALWAYS present so Angie can issue, whereas the
/// host's serving 443 block appears only once the cert is `ready`.
fn gen_acme(input: &GeneratorInput) -> String {
    let mut out = String::new();
    out.push_str("# ACME clients and their collector server blocks (managed).\n\n");

    if input.certificates.is_empty() {
        return out;
    }

    // dns-01 needs Angie to answer validation queries on UDP/53 itself.
    if input.certificates.iter().any(|c| c.challenge == "dns") {
        out.push_str("# dns-01 certificates: Angie answers ACME DNS queries itself.\n");
        out.push_str("acme_dns_port 53;\n\n");
    }

    let mut certs: Vec<&Certificate> = input.certificates.iter().collect();
    certs.sort_by_key(|c| c.id);

    for cert in certs {
        let directory = if cert.staging {
            LE_STAGING_DIRECTORY
        } else {
            LE_PROD_DIRECTORY
        };

        // acme_client <name> <uri> [params];
        let mut params = String::new();
        if cert.challenge != "http" {
            params.push_str(&format!(" challenge={}", cert.challenge));
        }
        if cert.key_type != "ecdsa" {
            params.push_str(&format!(" key_type={}", cert.key_type));
        }
        if let Some(email) = &cert.email {
            params.push_str(&format!(" email={email}"));
        }
        if !cert.enabled {
            // enabled=off keeps the cert usable but pauses renewal.
            params.push_str(" enabled=off");
        }
        let _ = writeln!(out, "acme_client {} {}{};", cert.name, directory, params);

        // Collector block — unix socket, never serves traffic.
        let sock = input
            .acme_socket_dir
            .join(format!("acme-{}.sock", cert.name));
        let _ = writeln!(out, "server {{");
        let _ = writeln!(out, "    listen unix:{};", sock.display());
        let _ = writeln!(out, "    server_name {};", cert.domains.join(" "));
        let _ = writeln!(out, "    acme {};", cert.name);
        let _ = writeln!(out, "}}");
        out.push('\n');
    }

    out
}

// ------------------------------------------------------- 15-rate-limits.conf

/// A rate-limit config is "active" only when enabled AND it defines at least
/// one actual limit (a request rate and/or a connection cap). Zeroed configs
/// (e.g. a disabled host, or enabled with all-zero) emit nothing.
fn rate_limit_active(rl: &RateLimit) -> bool {
    rl.enabled && (rl.rps > 0 || rl.conn > 0)
}

/// Per-host `limit_req_zone` / `limit_conn_zone` shared-memory zones. These are
/// http-context directives; emitting them in a low-sorted file guarantees each
/// zone is defined before the `20-host-*` server block that uses it. Returns
/// None when no enabled host has an active rate limit.
fn gen_rate_limits(input: &GeneratorInput) -> Option<String> {
    let mut hosts: Vec<&ProxyHost> = input
        .hosts
        .iter()
        .filter(|h| h.enabled && rate_limit_active(&h.rate_limit))
        .collect();
    hosts.sort_by_key(|h| h.id);
    if hosts.is_empty() {
        return None;
    }
    let mut out = String::new();
    for h in hosts {
        let rl = &h.rate_limit;
        if rl.rps > 0 {
            let _ = writeln!(
                out,
                "limit_req_zone $binary_remote_addr zone=rlreq_host_{}:10m rate={}r/s;",
                h.id, rl.rps
            );
        }
        if rl.conn > 0 {
            let _ = writeln!(
                out,
                "limit_conn_zone $binary_remote_addr zone=rlconn_host_{}:10m;",
                h.id
            );
        }
    }
    Some(out)
}

/// Emit the server-scope `limit_req` / `limit_conn` directives (with a 429
/// status instead of the default 503). Zones come from [`gen_rate_limits`].
fn emit_rate_limit(out: &mut String, host: &ProxyHost) {
    let rl = &host.rate_limit;
    if !rate_limit_active(rl) {
        return;
    }
    if rl.rps > 0 {
        let mut line = format!("    limit_req zone=rlreq_host_{}", host.id);
        if rl.burst > 0 {
            let _ = write!(line, " burst={}", rl.burst);
            if rl.nodelay {
                line.push_str(" nodelay");
            }
        }
        line.push(';');
        let _ = writeln!(out, "{line}");
        let _ = writeln!(out, "    limit_req_status 429;");
    }
    if rl.conn > 0 {
        let _ = writeln!(out, "    limit_conn rlconn_host_{} {};", host.id, rl.conn);
        let _ = writeln!(out, "    limit_conn_status 429;");
    }
}

// -------------------------------------------------------------- 03-bans.conf

/// Global IP blocklist as http-scope `deny` rules. Addresses were validated to
/// a bare IP or IP/CIDR upstream (never free text), so they interpolate safely.
/// Returns None when there are no bans (no file emitted). The `deny` rules are
/// inherited by every server that defines no access rules of its own; hosts
/// with their own allow/deny (access lists) are a documented exception.
fn gen_bans(input: &GeneratorInput) -> Option<String> {
    if input.bans.is_empty() {
        return None;
    }
    let mut bans: Vec<&Ban> = input.bans.iter().collect();
    bans.sort_by_key(|b| b.id);
    let mut out = String::new();
    for b in bans {
        let _ = writeln!(out, "deny {};", b.address);
    }
    Some(out)
}

// ------------------------------------------------------------- upstream pool

/// Emit the `upstream <zone> { ... }` block: shared-memory zone, optional
/// balancing method, the primary server, then any extra pool members. Passive
/// health (`max_fails`/`fail_timeout`) is attached to every peer when tuned off
/// the Angie defaults (1 / 10s). Hosts/ports were SSRF-validated upstream.
fn gen_upstream_block(out: &mut String, zone: &str, host: &ProxyHost) {
    let up = &host.upstream;
    let _ = writeln!(out, "upstream {zone} {{");
    let _ = writeln!(out, "    zone {zone} 64k;");
    if let Some(method) = up.method.directive() {
        let _ = writeln!(out, "    {method};");
    }
    // Passive-health suffix shared by every peer (empty when at defaults).
    let health = if up.max_fails != 1 || up.fail_timeout_secs != 10 {
        format!(
            " max_fails={} fail_timeout={}s",
            up.max_fails, up.fail_timeout_secs
        )
    } else {
        String::new()
    };
    // Primary server (never backup/down — it is the host's main target).
    let mut primary = format!("    server {}:{}", host.forward_host, host.forward_port);
    if up.primary_weight != 1 {
        let _ = write!(primary, " weight={}", up.primary_weight);
    }
    primary.push_str(&health);
    primary.push(';');
    let _ = writeln!(out, "{primary}");
    // Extra pool members.
    for s in &up.servers {
        let mut line = format!("    server {}:{}", s.host, s.port);
        if s.weight != 1 {
            let _ = write!(line, " weight={}", s.weight);
        }
        line.push_str(&health);
        if s.backup {
            line.push_str(" backup");
        }
        if s.down {
            line.push_str(" down");
        }
        line.push(';');
        let _ = writeln!(out, "{line}");
    }
    let _ = writeln!(out, "}}");
}

// -------------------------------------------------- 20-host-<id>-<slug>.conf

/// FileSet key for a host's materialized client-CA bundle (mTLS).
fn client_ca_name(host_id: i64) -> String {
    format!("client-ca-host-{host_id}.pem")
}

/// Render one proxy-host file. Returns (filename, body).
fn gen_host(
    host: &ProxyHost,
    cert: Option<&Certificate>,
    input: &GeneratorInput,
) -> anyhow::Result<(String, String)> {
    let slug = slugify(host.domains.first().map(String::as_str).unwrap_or(""));
    let filename = format!("20-host-{}-{}.conf", host.id, slug);

    // HTTPS is only rendered when a certificate is attached AND already issued
    // (see `Certificate::ready`). Without that, we emit HTTP-only so we never
    // serve TLS errors in place of the site (PLAN.md §4 first-issuance window).
    let https = matches!(cert, Some(c) if c.ready);
    let zone = format!("host_{}", host.id);
    let server_names = host.domains.join(" ");
    let ipv6 = input.settings.ipv6_enabled;

    let mut out = String::new();

    // upstream block: the `zone` gives us per-upstream metrics in /status. The
    // primary server is forward_host:forward_port; extra pool members, the
    // balancing method, and passive health (max_fails/fail_timeout) come from
    // host.upstream (PLAN.md §4).
    gen_upstream_block(&mut out, &zone, host);
    out.push('\n');

    if https {
        // Separate :80 server whose only job is the force-ssl redirect. The
        // redirect is UNCONDITIONAL — no /.well-known exception — because the
        // ACME module intercepts http-01 challenges at the POST_READ phase,
        // before `return` runs (PLAN.md §4).
        let _ = writeln!(out, "server {{");
        let _ = writeln!(out, "    listen 80;");
        if ipv6 {
            let _ = writeln!(out, "    listen [::]:80;");
        }
        let _ = writeln!(out, "    server_name {server_names};");
        let _ = writeln!(out, "    return 301 https://$host$request_uri;");
        let _ = writeln!(out, "}}");
        out.push('\n');

        // The real :443 server.
        let cert = cert.expect("https implies a ready certificate");
        let _ = writeln!(out, "server {{");
        let _ = writeln!(out, "    listen 443 ssl;");
        if ipv6 {
            let _ = writeln!(out, "    listen [::]:443 ssl;");
        }
        // HTTP/3 (QUIC over UDP 443) alongside the TLS listener. No `reuseport`
        // — it may appear at most once per address across all servers, and a
        // plain `listen 443 quic;` shares the socket fine (verified on Angie).
        if host.http3 {
            let _ = writeln!(out, "    listen 443 quic;");
            if ipv6 {
                let _ = writeln!(out, "    listen [::]:443 quic;");
            }
        }
        if host.http2 {
            let _ = writeln!(out, "    http2 on;");
        }
        if host.http3 {
            let _ = writeln!(out, "    http3 on;");
        }
        let _ = writeln!(out, "    server_name {server_names};");
        let _ = writeln!(out, "    status_zone {zone};");
        // Only the $acme_cert_<name> variable form is allowed here (never a
        // filesystem path — the linter enforces that).
        let _ = writeln!(out, "    ssl_certificate     $acme_cert_{};", cert.name);
        let _ = writeln!(out, "    ssl_certificate_key $acme_cert_key_{};", cert.name);
        // Mutual TLS: verify client certs against the host's CA. The CA bundle
        // is a managed http.d file (see the generate() loop). Referenced
        // RELATIVE to the main config dir (`http.d/…`) on purpose:
        // ssl_client_certificate is loaded EAGERLY at `angie -t` (unlike the
        // lazy auth_basic_user_file), so it must exist at validation time. A
        // relative path resolves to the staged CA (test conf lives beside the
        // staged http.d) during validation and the live CA after sync — one
        // string, correct in both. Assumes the standard /etc/angie/http.d layout.
        if host.mtls.active() {
            let _ = writeln!(
                out,
                "    ssl_client_certificate http.d/{};",
                client_ca_name(host.id)
            );
            let verify = if host.mtls.optional { "optional" } else { "on" };
            let _ = writeln!(out, "    ssl_verify_client {verify};");
        }
        // Advertise h3 so clients upgrade to QUIC on their next connection.
        if host.http3 {
            let _ = writeln!(
                out,
                "    add_header Alt-Svc 'h3=\":443\"; ma=86400' always;"
            );
        }
        host_features(&mut out, host, input, /* tls */ true)?;
        let _ = writeln!(out, "}}");
    } else {
        // Plain-HTTP host: no 443, no force-ssl redirect.
        let _ = writeln!(out, "server {{");
        let _ = writeln!(out, "    listen 80;");
        if ipv6 {
            let _ = writeln!(out, "    listen [::]:80;");
        }
        let _ = writeln!(out, "    server_name {server_names};");
        let _ = writeln!(out, "    status_zone {zone};");
        host_features(&mut out, host, input, /* tls */ false)?;
        let _ = writeln!(out, "}}");
    }

    Ok((filename, out))
}

/// Body shared between the HTTP-only and HTTPS server blocks: HSTS, shared
/// snippets, the main `location /`, custom locations, and the advanced snippet.
fn host_features(
    out: &mut String,
    host: &ProxyHost,
    input: &GeneratorInput,
    tls: bool,
) -> anyhow::Result<()> {
    // HSTS only makes sense over TLS.
    if tls && host.hsts {
        let mut value = String::from("max-age=63072000");
        if host.hsts_subdomains {
            value.push_str("; includeSubDomains");
        }
        // No `preload` — deliberately (PLAN.md §4): preload is a one-way,
        // hard-to-undo commitment we don't make on the user's behalf.
        let _ = writeln!(
            out,
            "    add_header Strict-Transport-Security \"{value}\" always;"
        );
    }

    // block-exploits.conf is a package-owned snippet of server-level
    // `if (...) { return 444; }` rules; it is included at SERVER scope. The
    // linter verifies the path stays under snippets_dir.
    if host.block_exploits {
        let p = snippet_path(&input.snippets_dir, "block-exploits.conf")?;
        let _ = writeln!(out, "    include {p};");
    }

    // Access control (basic auth + IP allow/deny), server scope so it covers
    // every location. Values were strictly validated upstream.
    if let Some(list) = host
        .access_list_id
        .and_then(|id| input.access_lists.iter().find(|l| l.id == id))
    {
        let has_auth = !list.users.is_empty();
        let has_ip = !list.clients.is_empty();
        // `satisfy` only matters when BOTH mechanisms are present.
        if has_auth && has_ip {
            let _ = writeln!(out, "    satisfy {};", list.satisfy);
        }
        if has_auth {
            let file = input.http_d_dir.join(htpasswd_name(list.id));
            let _ = writeln!(out, "    auth_basic \"Restricted\";");
            let _ = writeln!(out, "    auth_basic_user_file {};", file.display());
        }
        for (directive, address) in &list.clients {
            let _ = writeln!(out, "    {directive} {address};");
        }
        // NPM parity: once any IP rule exists, everything else is denied.
        if has_ip {
            let _ = writeln!(out, "    deny all;");
        }
    }

    // Rate limiting (server scope → applies to every location below).
    emit_rate_limit(out, host);

    // Main location. cache-assets.conf is directives-only (proxy_cache*, no
    // location/proxy_pass) and MUST be included inside this location, next to
    // proxy_pass — so it is emitted here rather than at server scope.
    let _ = writeln!(out, "    location / {{");
    proxy_body(
        out,
        host.forward_scheme,
        &host_upstream_ref(host),
        host,
        input,
    );
    if host.cache_assets {
        let p = snippet_path(&input.snippets_dir, "cache-assets.conf")?;
        let _ = writeln!(out, "        include {p};");
    }
    let _ = writeln!(out, "    }}");

    // Custom locations. Each is a self-contained location with a direct
    // proxy_pass to its own upstream target (consistent style across all of
    // them). The path was validated to a strict charset upstream.
    for loc in &host.locations {
        let _ = writeln!(out, "    location {} {{", loc.path);
        if let Some(rewrite) = &loc.rewrite {
            // `break` = stop rewrite processing and use the rewritten URI.
            let _ = writeln!(out, "        rewrite ^ {rewrite} break;");
        }
        proxy_body(
            out,
            loc.forward_scheme,
            &location_upstream_ref(loc),
            host,
            input,
        );
        // Per-location snippet (validated + gated upstream; re-linted on output).
        if let Some(snip) = &loc.snippet {
            emit_snippet(out, snip, "        ");
        }
        let _ = writeln!(out, "    }}");
    }

    // Host-wide advanced snippet, inserted verbatim (gated by
    // allow_advanced_snippets upstream; the linter re-checks the output).
    if let Some(snip) = &host.advanced_snippet {
        emit_snippet(out, snip, "    ");
    }

    Ok(())
}

// ----------------------------------------------------- redirect / 404 hosts

/// Emit the HSTS header if enabled (TLS only). Shared by all host types.
fn emit_hsts(out: &mut String, hsts: bool, subdomains: bool) {
    if hsts {
        let mut value = String::from("max-age=63072000");
        if subdomains {
            value.push_str("; includeSubDomains");
        }
        let _ = writeln!(
            out,
            "    add_header Strict-Transport-Security \"{value}\" always;"
        );
    }
}

/// Render one redirection host. Returns (filename, body).
fn gen_redirect(
    rh: &RedirectHost,
    cert: Option<&Certificate>,
    input: &GeneratorInput,
) -> (String, String) {
    let slug = slugify(rh.domains.first().map(String::as_str).unwrap_or(""));
    let filename = format!("30-redirect-{}-{}.conf", rh.id, slug);
    let https = matches!(cert, Some(c) if c.ready);
    let server_names = rh.domains.join(" ");
    let ipv6 = input.settings.ipv6_enabled;

    // The redirect statement: `return <code> <scheme>://<domain>[<path>];`.
    // scheme/domain/code were all validated; path is $request_uri or nothing.
    let path = if rh.preserve_path { "$request_uri" } else { "" };
    let redirect_stmt = format!(
        "    return {} {}://{}{};",
        rh.forward_http_code,
        rh.forward_scheme.as_target(),
        rh.forward_domain,
        path
    );
    let block_exploits = |out: &mut String| {
        if rh.block_exploits {
            if let Ok(p) = snippet_path(&input.snippets_dir, "block-exploits.conf") {
                let _ = writeln!(out, "    include {p};");
            }
        }
    };

    let mut out = String::new();
    if https {
        let cert = cert.expect("https implies a ready cert");
        // :80 — force-ssl sends the client to https first; otherwise redirect.
        let _ = writeln!(out, "server {{");
        let _ = writeln!(out, "    listen 80;");
        if ipv6 {
            let _ = writeln!(out, "    listen [::]:80;");
        }
        let _ = writeln!(out, "    server_name {server_names};");
        block_exploits(&mut out);
        if rh.force_ssl {
            let _ = writeln!(out, "    return 301 https://$host$request_uri;");
        } else {
            let _ = writeln!(out, "{redirect_stmt}");
        }
        let _ = writeln!(out, "}}");
        out.push('\n');
        // :443 — the real redirect.
        let _ = writeln!(out, "server {{");
        let _ = writeln!(out, "    listen 443 ssl;");
        if ipv6 {
            let _ = writeln!(out, "    listen [::]:443 ssl;");
        }
        if rh.http2 {
            let _ = writeln!(out, "    http2 on;");
        }
        let _ = writeln!(out, "    server_name {server_names};");
        let _ = writeln!(out, "    ssl_certificate     $acme_cert_{};", cert.name);
        let _ = writeln!(out, "    ssl_certificate_key $acme_cert_key_{};", cert.name);
        emit_hsts(&mut out, rh.hsts, rh.hsts_subdomains);
        block_exploits(&mut out);
        let _ = writeln!(out, "{redirect_stmt}");
        if let Some(snip) = &rh.advanced_snippet {
            emit_snippet(&mut out, snip, "    ");
        }
        let _ = writeln!(out, "}}");
    } else {
        let _ = writeln!(out, "server {{");
        let _ = writeln!(out, "    listen 80;");
        if ipv6 {
            let _ = writeln!(out, "    listen [::]:80;");
        }
        let _ = writeln!(out, "    server_name {server_names};");
        block_exploits(&mut out);
        let _ = writeln!(out, "{redirect_stmt}");
        if let Some(snip) = &rh.advanced_snippet {
            emit_snippet(&mut out, snip, "    ");
        }
        let _ = writeln!(out, "}}");
    }
    (filename, out)
}

/// Render one 404 (dead) host. Returns (filename, body).
fn gen_dead(dh: &DeadHost, cert: Option<&Certificate>, input: &GeneratorInput) -> (String, String) {
    let slug = slugify(dh.domains.first().map(String::as_str).unwrap_or(""));
    let filename = format!("40-dead-{}-{}.conf", dh.id, slug);
    let https = matches!(cert, Some(c) if c.ready);
    let server_names = dh.domains.join(" ");
    let ipv6 = input.settings.ipv6_enabled;

    let mut out = String::new();
    if https {
        let cert = cert.expect("https implies a ready cert");
        let _ = writeln!(out, "server {{");
        let _ = writeln!(out, "    listen 80;");
        if ipv6 {
            let _ = writeln!(out, "    listen [::]:80;");
        }
        let _ = writeln!(out, "    server_name {server_names};");
        if dh.force_ssl {
            let _ = writeln!(out, "    return 301 https://$host$request_uri;");
        } else {
            let _ = writeln!(out, "    return 404;");
        }
        let _ = writeln!(out, "}}");
        out.push('\n');
        let _ = writeln!(out, "server {{");
        let _ = writeln!(out, "    listen 443 ssl;");
        if ipv6 {
            let _ = writeln!(out, "    listen [::]:443 ssl;");
        }
        if dh.http2 {
            let _ = writeln!(out, "    http2 on;");
        }
        let _ = writeln!(out, "    server_name {server_names};");
        let _ = writeln!(out, "    ssl_certificate     $acme_cert_{};", cert.name);
        let _ = writeln!(out, "    ssl_certificate_key $acme_cert_key_{};", cert.name);
        emit_hsts(&mut out, dh.hsts, dh.hsts_subdomains);
        let _ = writeln!(out, "    return 404;");
        if let Some(snip) = &dh.advanced_snippet {
            emit_snippet(&mut out, snip, "    ");
        }
        let _ = writeln!(out, "}}");
    } else {
        let _ = writeln!(out, "server {{");
        let _ = writeln!(out, "    listen 80;");
        if ipv6 {
            let _ = writeln!(out, "    listen [::]:80;");
        }
        let _ = writeln!(out, "    server_name {server_names};");
        let _ = writeln!(out, "    return 404;");
        if let Some(snip) = &dh.advanced_snippet {
            emit_snippet(&mut out, snip, "    ");
        }
        let _ = writeln!(out, "}}");
    }
    (filename, out)
}

/// The upstream reference used by the main `location /` proxy_pass.
///
/// The main location always proxies to the named `upstream host_<id>` block
/// (which carries the metrics zone). The scheme comes from `forward_scheme`.
fn host_upstream_ref(host: &ProxyHost) -> String {
    format!("host_{}", host.id)
}

/// Custom locations proxy directly to `host:port` rather than a named upstream
/// (they don't get their own metrics zone in v1). The host/port were validated
/// to a safe charset upstream.
fn location_upstream_ref(loc: &CustomLocation) -> String {
    format!("{}:{}", loc.forward_host, loc.forward_port)
}

/// Emit the standard proxy_pass + proxy_set_header set into an open `location`
/// block. `target` is either a named upstream (`host_<id>`) or `host:port`.
fn proxy_body(
    out: &mut String,
    scheme: Scheme,
    target: &str,
    host: &ProxyHost,
    input: &GeneratorInput,
) {
    let _ = writeln!(out, "        proxy_pass {}://{target};", scheme.as_str());
    let _ = writeln!(out, "        proxy_set_header Host $host;");
    let _ = writeln!(
        out,
        "        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;"
    );
    // X-Forwarded-Proto: when we trust an inbound proxy we forward its value
    // (`$scheme` reflects the connection to *us*; to honour an upstream L7 LB we
    // pass the incoming header through). Otherwise we assert our own scheme.
    if host.trust_forwarded_proto {
        let _ = writeln!(
            out,
            "        proxy_set_header X-Forwarded-Proto $http_x_forwarded_proto;"
        );
    } else {
        let _ = writeln!(out, "        proxy_set_header X-Forwarded-Proto $scheme;");
    }

    // WebSocket upgrade support.
    if host.websockets_upgrade {
        let _ = writeln!(out, "        proxy_http_version 1.1;");
        let _ = writeln!(out, "        proxy_set_header Upgrade $http_upgrade;");
        let _ = writeln!(
            out,
            "        proxy_set_header Connection $connection_upgrade;"
        );
    }

    // When the host's access list has basic auth and "pass auth to upstream" is
    // OFF, strip the Authorization header so the upstream never sees the
    // gate's credentials. Emitted per-location because a location that sets any
    // proxy_set_header does not inherit server-scope ones.
    if let Some(list) = host
        .access_list_id
        .and_then(|id| input.access_lists.iter().find(|l| l.id == id))
    {
        if !list.pass_auth && !list.users.is_empty() {
            let _ = writeln!(out, "        proxy_set_header Authorization \"\";");
        }
    }
}

/// Insert a validated snippet verbatim, re-indented so the output stays tidy.
/// We do NOT attempt to rewrite the snippet's contents — it was allow-listed on
/// input and the level-2 linter re-checks the generated bytes.
fn emit_snippet(out: &mut String, snippet: &str, indent: &str) {
    for line in snippet.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            out.push('\n');
        } else {
            let _ = writeln!(out, "{indent}{trimmed}");
        }
    }
}

// --------------------------------------------------------------- helpers

/// Build an absolute `<snippets_dir>/<name>` path string for an `include`,
/// refusing anything that isn't a plain filename (defence in depth — `name`
/// is a compile-time constant today, but this keeps the invariant local).
fn snippet_path(dir: &Path, name: &str) -> anyhow::Result<String> {
    if name.contains('/') || name.contains("..") {
        anyhow::bail!("snippet name {name:?} is not a plain filename");
    }
    path_str(&dir.join(name))
}

/// Render a path as a UTF-8 string for the config, rejecting non-UTF-8 and any
/// character that could break out of a directive.
fn path_str(p: &Path) -> anyhow::Result<String> {
    let s = p
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("non-UTF-8 path {}", p.display()))?;
    if s.chars()
        .any(|c| c.is_whitespace() || c.is_control() || matches!(c, ';' | '{' | '}'))
    {
        anyhow::bail!("path {s:?} contains a character illegal in an Angie directive");
    }
    Ok(s.to_string())
}

/// Derive a filesystem-safe slug from the first domain into the `[a-z0-9-]`
/// charset (§4). Alphanumerics are lowercased and kept; an existing hyphen is
/// kept verbatim (so punycode labels like `xn--80a1acny` survive intact); any
/// other character (`.`, `*`, whitespace, …) becomes a single separating `-`.
/// Leading/trailing dashes are trimmed. A wildcard `*.example.com` becomes
/// `example-com`. Falls back to `host` when the result is empty so the filename
/// is always well-formed.
fn slugify(domain: &str) -> String {
    let mut slug = String::with_capacity(domain.len());
    // Tracks whether the last char we pushed came from a *separator* run, so we
    // collapse runs of separators without collapsing genuine input hyphens.
    let mut pending_sep = false;
    for c in domain.chars() {
        if c.is_ascii_alphanumeric() {
            if pending_sep && !slug.is_empty() {
                slug.push('-');
            }
            pending_sep = false;
            slug.push(c.to_ascii_lowercase());
        } else if c == '-' {
            // A literal hyphen from the input (e.g. inside a punycode label).
            if pending_sep && !slug.is_empty() {
                slug.push('-');
                pending_sep = false;
            }
            slug.push('-');
        } else {
            // Any other character starts/continues a separator run.
            pending_sep = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "host".to_string()
    } else {
        slug
    }
}

// ------------------------------------------------------- MANAGED-BY header

/// Header marker prefix. Full line:
/// `# MANAGED BY angie-panel <version> sha256:<hex>`
const HEADER_PREFIX: &str = "# MANAGED BY angie-panel ";

/// Prepend the MANAGED-BY header. The hash covers the body *after* the header
/// line, so re-wrapping the same body always yields the same header — that is
/// what makes drift detection stable across generator upgrades (PLAN.md §2.2).
pub fn with_header(body: &str) -> String {
    let hash = body_hash(body);
    let version = env!("CARGO_PKG_VERSION");
    format!("{HEADER_PREFIX}{version} sha256:{hash}\n{body}")
}

/// Parsed MANAGED-BY header + whether the declared hash matches the actual body.
pub struct ManagedMeta {
    pub generator_version: String,
    pub declared_hash: String,
    pub hash_matches: bool,
}

/// Parse a managed file's header and verify its hash. Returns `None` for files
/// without our header (e.g. a foreign file dropped into http.d — the panel
/// lists those but never rewrites them, PLAN.md §2.2).
pub fn managed_meta(content: &str) -> Option<ManagedMeta> {
    // Split off the first line (the header) from the rest (the body). We keep
    // the body exactly as it appeared after the first '\n'.
    let (header, body) = match content.split_once('\n') {
        Some((h, b)) => (h, b),
        None => (content, ""),
    };
    let rest = header.strip_prefix(HEADER_PREFIX)?;
    // rest = "<version> sha256:<hex>"
    let (version, hash_part) = rest.split_once(' ')?;
    let declared_hash = hash_part.strip_prefix("sha256:")?.trim().to_string();
    let actual = body_hash(body);
    Some(ManagedMeta {
        generator_version: version.to_string(),
        hash_matches: actual == declared_hash,
        declared_hash,
    })
}

/// sha256 of the body (hex, lowercase).
fn body_hash(body: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests;
