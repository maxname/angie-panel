//! Persistence for proxy hosts, settings, and apply history. Runtime sqlx
//! queries (no compile-time DB needed). JSON columns (domains, locations) are
//! stored as TEXT and parsed at the boundary; booleans as INTEGER 0/1.

use anyhow::Context;
use sqlx::SqlitePool;

use crate::db::now_epoch;
use crate::model::{
    Certificate, CertificateInput, Challenge, CustomHeader, CustomLocation, DnsCredential,
    DnsCredentialInput, ErrorPages, ForwardAuth, Gzip, KeyType, Maintenance, Mtls, ProxyHost,
    ProxyHostInput, ProxyTuning, RateLimit, Scheme, Upstream,
};

// ------------------------------------------------------------------- rows

#[derive(sqlx::FromRow)]
struct HostRow {
    id: i64,
    domains: String,
    forward_scheme: String,
    forward_host: String,
    forward_port: i64,
    websockets_upgrade: i64,
    block_exploits: i64,
    cache_assets: i64,
    http2: i64,
    http3: i64,
    force_ssl: i64,
    hsts: i64,
    hsts_subdomains: i64,
    trust_forwarded_proto: i64,
    certificate_id: Option<i64>,
    access_list_id: Option<i64>,
    locations: String,
    advanced_snippet: Option<String>,
    rate_limit: Option<String>,
    upstream: Option<String>,
    mtls: Option<String>,
    forward_auth: Option<String>,
    custom_headers: Option<String>,
    maintenance: Option<String>,
    gzip: Option<String>,
    error_pages: Option<String>,
    proxy_tuning: Option<String>,
    enabled: i64,
    created_at: i64,
    updated_at: i64,
}

fn scheme_from_str(s: &str) -> Scheme {
    match s {
        "https" => Scheme::Https,
        _ => Scheme::Http,
    }
}

impl HostRow {
    fn into_model(self) -> anyhow::Result<ProxyHost> {
        Ok(ProxyHost {
            id: self.id,
            domains: serde_json::from_str(&self.domains).context("domains json")?,
            forward_scheme: scheme_from_str(&self.forward_scheme),
            forward_host: self.forward_host,
            forward_port: self.forward_port as u16,
            websockets_upgrade: self.websockets_upgrade != 0,
            block_exploits: self.block_exploits != 0,
            cache_assets: self.cache_assets != 0,
            http2: self.http2 != 0,
            http3: self.http3 != 0,
            force_ssl: self.force_ssl != 0,
            hsts: self.hsts != 0,
            hsts_subdomains: self.hsts_subdomains != 0,
            trust_forwarded_proto: self.trust_forwarded_proto != 0,
            certificate_id: self.certificate_id,
            access_list_id: self.access_list_id,
            locations: serde_json::from_str(&self.locations).context("locations json")?,
            advanced_snippet: self.advanced_snippet,
            rate_limit: rate_limit_from_json(self.rate_limit.as_deref())?,
            upstream: upstream_from_json(self.upstream.as_deref())?,
            mtls: mtls_from_json(self.mtls.as_deref())?,
            forward_auth: forward_auth_from_json(self.forward_auth.as_deref())?,
            custom_headers: custom_headers_from_json(self.custom_headers.as_deref())?,
            maintenance: maintenance_from_json(self.maintenance.as_deref())?,
            gzip: gzip_from_json(self.gzip.as_deref())?,
            error_pages: error_pages_from_json(self.error_pages.as_deref())?,
            proxy_tuning: proxy_tuning_from_json(self.proxy_tuning.as_deref())?,
            enabled: self.enabled != 0,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

fn rate_limit_json(rl: &RateLimit) -> String {
    serde_json::to_string(rl).unwrap_or_else(|_| "{}".into())
}

/// Parse the stored rate_limit JSON (NULL / absent = disabled default).
fn rate_limit_from_json(raw: Option<&str>) -> anyhow::Result<RateLimit> {
    match raw {
        Some(s) if !s.trim().is_empty() => Ok(serde_json::from_str(s).context("rate_limit json")?),
        _ => Ok(RateLimit::default()),
    }
}

fn upstream_json(up: &Upstream) -> String {
    serde_json::to_string(up).unwrap_or_else(|_| "{}".into())
}

/// Parse the stored upstream JSON (NULL / absent = plain single-server host).
fn upstream_from_json(raw: Option<&str>) -> anyhow::Result<Upstream> {
    match raw {
        Some(s) if !s.trim().is_empty() => Ok(serde_json::from_str(s).context("upstream json")?),
        _ => Ok(Upstream::default()),
    }
}

fn mtls_json(m: &Mtls) -> String {
    serde_json::to_string(m).unwrap_or_else(|_| "{}".into())
}

/// Parse the stored mtls JSON (NULL / absent = no client-cert requirement).
fn mtls_from_json(raw: Option<&str>) -> anyhow::Result<Mtls> {
    match raw {
        Some(s) if !s.trim().is_empty() => Ok(serde_json::from_str(s).context("mtls json")?),
        _ => Ok(Mtls::default()),
    }
}

fn forward_auth_json(fa: &ForwardAuth) -> String {
    serde_json::to_string(fa).unwrap_or_else(|_| "{}".into())
}

/// Parse the stored forward_auth JSON (NULL / absent = no forward auth).
fn forward_auth_from_json(raw: Option<&str>) -> anyhow::Result<ForwardAuth> {
    match raw {
        Some(s) if !s.trim().is_empty() => {
            Ok(serde_json::from_str(s).context("forward_auth json")?)
        }
        _ => Ok(ForwardAuth::default()),
    }
}

fn custom_headers_json(h: &[CustomHeader]) -> String {
    serde_json::to_string(h).unwrap_or_else(|_| "[]".into())
}

/// Parse the stored custom_headers JSON (NULL / absent = no custom headers).
fn custom_headers_from_json(raw: Option<&str>) -> anyhow::Result<Vec<CustomHeader>> {
    match raw {
        Some(s) if !s.trim().is_empty() => {
            Ok(serde_json::from_str(s).context("custom_headers json")?)
        }
        _ => Ok(Vec::new()),
    }
}

fn maintenance_json(m: &Maintenance) -> String {
    serde_json::to_string(m).unwrap_or_else(|_| "{}".into())
}

/// Parse the stored maintenance JSON (NULL / absent = maintenance off).
fn maintenance_from_json(raw: Option<&str>) -> anyhow::Result<Maintenance> {
    match raw {
        Some(s) if !s.trim().is_empty() => Ok(serde_json::from_str(s).context("maintenance json")?),
        _ => Ok(Maintenance::default()),
    }
}

fn gzip_json(g: &Gzip) -> String {
    serde_json::to_string(g).unwrap_or_else(|_| "{}".into())
}

/// Parse the stored gzip JSON (NULL / absent = gzip off).
fn gzip_from_json(raw: Option<&str>) -> anyhow::Result<Gzip> {
    match raw {
        Some(s) if !s.trim().is_empty() => Ok(serde_json::from_str(s).context("gzip json")?),
        _ => Ok(Gzip::default()),
    }
}

fn proxy_tuning_json(p: &ProxyTuning) -> String {
    serde_json::to_string(p).unwrap_or_else(|_| "{}".into())
}

/// Parse the stored proxy-tuning JSON (NULL / absent = Angie defaults).
fn proxy_tuning_from_json(raw: Option<&str>) -> anyhow::Result<ProxyTuning> {
    match raw {
        Some(s) if !s.trim().is_empty() => {
            Ok(serde_json::from_str(s).context("proxy_tuning json")?)
        }
        _ => Ok(ProxyTuning::default()),
    }
}

fn error_pages_json(e: &ErrorPages) -> String {
    serde_json::to_string(e).unwrap_or_else(|_| "{}".into())
}

/// Parse the stored error-pages JSON (NULL / absent = no custom pages).
fn error_pages_from_json(raw: Option<&str>) -> anyhow::Result<ErrorPages> {
    match raw {
        Some(s) if !s.trim().is_empty() => Ok(serde_json::from_str(s).context("error_pages json")?),
        _ => Ok(ErrorPages::default()),
    }
}

const HOST_COLUMNS: &str = "id, domains, forward_scheme, forward_host, forward_port, \
     websockets_upgrade, block_exploits, cache_assets, http2, http3, force_ssl, hsts, \
     hsts_subdomains, trust_forwarded_proto, certificate_id, access_list_id, locations, \
     advanced_snippet, rate_limit, upstream, mtls, forward_auth, custom_headers, maintenance, \
     gzip, error_pages, proxy_tuning, enabled, created_at, updated_at";

// -------------------------------------------------------------- host CRUD

pub async fn list_hosts(db: &SqlitePool) -> anyhow::Result<Vec<ProxyHost>> {
    let rows: Vec<HostRow> = sqlx::query_as(&format!(
        "SELECT {HOST_COLUMNS} FROM proxy_hosts ORDER BY id"
    ))
    .fetch_all(db)
    .await?;
    rows.into_iter().map(HostRow::into_model).collect()
}

pub async fn get_host(db: &SqlitePool, id: i64) -> anyhow::Result<Option<ProxyHost>> {
    let row: Option<HostRow> = sqlx::query_as(&format!(
        "SELECT {HOST_COLUMNS} FROM proxy_hosts WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(db)
    .await?;
    row.map(HostRow::into_model).transpose()
}

fn locations_json(locs: &[CustomLocation]) -> String {
    serde_json::to_string(locs).unwrap_or_else(|_| "[]".into())
}

fn domains_json(domains: &[String]) -> String {
    serde_json::to_string(domains).unwrap_or_else(|_| "[]".into())
}

/// Insert a validated host; returns the new id.
pub async fn insert_host(db: &SqlitePool, input: &ProxyHostInput) -> anyhow::Result<i64> {
    let now = now_epoch();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO proxy_hosts (domains, forward_scheme, forward_host, forward_port, \
         websockets_upgrade, block_exploits, cache_assets, http2, http3, force_ssl, hsts, \
         hsts_subdomains, trust_forwarded_proto, certificate_id, access_list_id, locations, \
         advanced_snippet, rate_limit, upstream, mtls, forward_auth, custom_headers, maintenance, \
         gzip, error_pages, proxy_tuning, enabled, created_at, updated_at) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) RETURNING id",
    )
    .bind(domains_json(&input.domains))
    .bind(input.forward_scheme.as_str())
    .bind(&input.forward_host)
    .bind(input.forward_port as i64)
    .bind(input.websockets_upgrade as i64)
    .bind(input.block_exploits as i64)
    .bind(input.cache_assets as i64)
    .bind(input.http2 as i64)
    .bind(input.http3 as i64)
    .bind(input.force_ssl as i64)
    .bind(input.hsts as i64)
    .bind(input.hsts_subdomains as i64)
    .bind(input.trust_forwarded_proto as i64)
    .bind(input.certificate_id)
    .bind(input.access_list_id)
    .bind(locations_json(&input.locations))
    .bind(input.advanced_snippet.as_deref())
    .bind(rate_limit_json(&input.rate_limit))
    .bind(upstream_json(&input.upstream))
    .bind(mtls_json(&input.mtls))
    .bind(forward_auth_json(&input.forward_auth))
    .bind(custom_headers_json(&input.custom_headers))
    .bind(maintenance_json(&input.maintenance))
    .bind(gzip_json(&input.gzip))
    .bind(error_pages_json(&input.error_pages))
    .bind(proxy_tuning_json(&input.proxy_tuning))
    .bind(input.enabled as i64)
    .bind(now)
    .bind(now)
    .fetch_one(db)
    .await?;
    Ok(id)
}

/// Update all mutable fields of an existing host. Returns false if not found.
pub async fn update_host(db: &SqlitePool, id: i64, input: &ProxyHostInput) -> anyhow::Result<bool> {
    let rows = sqlx::query(
        "UPDATE proxy_hosts SET domains=?, forward_scheme=?, forward_host=?, forward_port=?, \
         websockets_upgrade=?, block_exploits=?, cache_assets=?, http2=?, http3=?, force_ssl=?, hsts=?, \
         hsts_subdomains=?, trust_forwarded_proto=?, certificate_id=?, access_list_id=?, \
         locations=?, advanced_snippet=?, rate_limit=?, upstream=?, mtls=?, forward_auth=?, \
         custom_headers=?, maintenance=?, gzip=?, error_pages=?, proxy_tuning=?, enabled=?, \
         updated_at=? WHERE id=?",
    )
    .bind(domains_json(&input.domains))
    .bind(input.forward_scheme.as_str())
    .bind(&input.forward_host)
    .bind(input.forward_port as i64)
    .bind(input.websockets_upgrade as i64)
    .bind(input.block_exploits as i64)
    .bind(input.cache_assets as i64)
    .bind(input.http2 as i64)
    .bind(input.http3 as i64)
    .bind(input.force_ssl as i64)
    .bind(input.hsts as i64)
    .bind(input.hsts_subdomains as i64)
    .bind(input.trust_forwarded_proto as i64)
    .bind(input.certificate_id)
    .bind(input.access_list_id)
    .bind(locations_json(&input.locations))
    .bind(input.advanced_snippet.as_deref())
    .bind(rate_limit_json(&input.rate_limit))
    .bind(upstream_json(&input.upstream))
    .bind(mtls_json(&input.mtls))
    .bind(forward_auth_json(&input.forward_auth))
    .bind(custom_headers_json(&input.custom_headers))
    .bind(maintenance_json(&input.maintenance))
    .bind(gzip_json(&input.gzip))
    .bind(error_pages_json(&input.error_pages))
    .bind(proxy_tuning_json(&input.proxy_tuning))
    .bind(input.enabled as i64)
    .bind(now_epoch())
    .bind(id)
    .execute(db)
    .await?;
    Ok(rows.rows_affected() > 0)
}

pub async fn delete_host(db: &SqlitePool, id: i64) -> anyhow::Result<bool> {
    let rows = sqlx::query("DELETE FROM proxy_hosts WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?;
    Ok(rows.rows_affected() > 0)
}

pub async fn set_enabled(db: &SqlitePool, id: i64, enabled: bool) -> anyhow::Result<bool> {
    let rows = sqlx::query("UPDATE proxy_hosts SET enabled = ?, updated_at = ? WHERE id = ?")
        .bind(enabled as i64)
        .bind(now_epoch())
        .bind(id)
        .execute(db)
        .await?;
    Ok(rows.rows_affected() > 0)
}

/// DB revision = max(updated_at) across ALL host types (used to reject a stale
/// apply against a preview computed from an older state — PLAN.md §2.2).
pub async fn hosts_revision(db: &SqlitePool) -> anyhow::Result<i64> {
    let rev: Option<i64> = sqlx::query_scalar(
        "SELECT MAX(m) FROM (SELECT MAX(updated_at) m FROM proxy_hosts \
         UNION ALL SELECT MAX(updated_at) FROM redirect_hosts \
         UNION ALL SELECT MAX(updated_at) FROM dead_hosts \
         UNION ALL SELECT MAX(updated_at) FROM streams \
         UNION ALL SELECT MAX(updated_at) FROM sni_routers \
         UNION ALL SELECT MAX(created_at) FROM bans)",
    )
    .fetch_one(db)
    .await?;
    Ok(rev.unwrap_or(0))
}

// ------------------------------------------------- redirect / dead hosts

use crate::model::{DeadHost, DeadHostInput, RedirectHost, RedirectHostInput, RedirectScheme};

fn redirect_scheme_from_str(s: &str) -> RedirectScheme {
    match s {
        "http" => RedirectScheme::Http,
        "https" => RedirectScheme::Https,
        _ => RedirectScheme::Auto,
    }
}

fn redirect_scheme_str(s: RedirectScheme) -> &'static str {
    match s {
        RedirectScheme::Auto => "auto",
        RedirectScheme::Http => "http",
        RedirectScheme::Https => "https",
    }
}

#[derive(sqlx::FromRow)]
struct RedirectRow {
    id: i64,
    domains: String,
    forward_scheme: String,
    forward_domain: String,
    forward_http_code: i64,
    preserve_path: i64,
    certificate_id: Option<i64>,
    force_ssl: i64,
    hsts: i64,
    hsts_subdomains: i64,
    http2: i64,
    block_exploits: i64,
    advanced_snippet: Option<String>,
    enabled: i64,
    created_at: i64,
    updated_at: i64,
}

impl RedirectRow {
    fn into_model(self) -> anyhow::Result<RedirectHost> {
        Ok(RedirectHost {
            id: self.id,
            domains: serde_json::from_str(&self.domains).context("domains json")?,
            forward_scheme: redirect_scheme_from_str(&self.forward_scheme),
            forward_domain: self.forward_domain,
            forward_http_code: self.forward_http_code as u16,
            preserve_path: self.preserve_path != 0,
            certificate_id: self.certificate_id,
            force_ssl: self.force_ssl != 0,
            hsts: self.hsts != 0,
            hsts_subdomains: self.hsts_subdomains != 0,
            http2: self.http2 != 0,
            block_exploits: self.block_exploits != 0,
            advanced_snippet: self.advanced_snippet,
            enabled: self.enabled != 0,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

const REDIRECT_COLUMNS: &str = "id, domains, forward_scheme, forward_domain, forward_http_code, \
     preserve_path, certificate_id, force_ssl, hsts, hsts_subdomains, http2, block_exploits, \
     advanced_snippet, enabled, created_at, updated_at";

pub async fn list_redirects(db: &SqlitePool) -> anyhow::Result<Vec<RedirectHost>> {
    let rows: Vec<RedirectRow> = sqlx::query_as(&format!(
        "SELECT {REDIRECT_COLUMNS} FROM redirect_hosts ORDER BY id"
    ))
    .fetch_all(db)
    .await?;
    rows.into_iter().map(RedirectRow::into_model).collect()
}

pub async fn get_redirect(db: &SqlitePool, id: i64) -> anyhow::Result<Option<RedirectHost>> {
    let row: Option<RedirectRow> = sqlx::query_as(&format!(
        "SELECT {REDIRECT_COLUMNS} FROM redirect_hosts WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(db)
    .await?;
    row.map(RedirectRow::into_model).transpose()
}

pub async fn insert_redirect(db: &SqlitePool, i: &RedirectHostInput) -> anyhow::Result<i64> {
    let now = now_epoch();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO redirect_hosts (domains, forward_scheme, forward_domain, forward_http_code, \
         preserve_path, certificate_id, force_ssl, hsts, hsts_subdomains, http2, block_exploits, \
         advanced_snippet, enabled, created_at, updated_at) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) RETURNING id",
    )
    .bind(domains_json(&i.domains))
    .bind(redirect_scheme_str(i.forward_scheme))
    .bind(&i.forward_domain)
    .bind(i.forward_http_code as i64)
    .bind(i.preserve_path as i64)
    .bind(i.certificate_id)
    .bind(i.force_ssl as i64)
    .bind(i.hsts as i64)
    .bind(i.hsts_subdomains as i64)
    .bind(i.http2 as i64)
    .bind(i.block_exploits as i64)
    .bind(i.advanced_snippet.as_deref())
    .bind(i.enabled as i64)
    .bind(now)
    .bind(now)
    .fetch_one(db)
    .await?;
    Ok(id)
}

pub async fn update_redirect(
    db: &SqlitePool,
    id: i64,
    i: &RedirectHostInput,
) -> anyhow::Result<bool> {
    let rows = sqlx::query(
        "UPDATE redirect_hosts SET domains=?, forward_scheme=?, forward_domain=?, \
         forward_http_code=?, preserve_path=?, certificate_id=?, force_ssl=?, hsts=?, \
         hsts_subdomains=?, http2=?, block_exploits=?, advanced_snippet=?, enabled=?, \
         updated_at=? WHERE id=?",
    )
    .bind(domains_json(&i.domains))
    .bind(redirect_scheme_str(i.forward_scheme))
    .bind(&i.forward_domain)
    .bind(i.forward_http_code as i64)
    .bind(i.preserve_path as i64)
    .bind(i.certificate_id)
    .bind(i.force_ssl as i64)
    .bind(i.hsts as i64)
    .bind(i.hsts_subdomains as i64)
    .bind(i.http2 as i64)
    .bind(i.block_exploits as i64)
    .bind(i.advanced_snippet.as_deref())
    .bind(i.enabled as i64)
    .bind(now_epoch())
    .bind(id)
    .execute(db)
    .await?;
    Ok(rows.rows_affected() > 0)
}

pub async fn delete_redirect(db: &SqlitePool, id: i64) -> anyhow::Result<bool> {
    let rows = sqlx::query("DELETE FROM redirect_hosts WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?;
    Ok(rows.rows_affected() > 0)
}

pub async fn set_redirect_enabled(db: &SqlitePool, id: i64, enabled: bool) -> anyhow::Result<bool> {
    let rows = sqlx::query("UPDATE redirect_hosts SET enabled=?, updated_at=? WHERE id=?")
        .bind(enabled as i64)
        .bind(now_epoch())
        .bind(id)
        .execute(db)
        .await?;
    Ok(rows.rows_affected() > 0)
}

#[derive(sqlx::FromRow)]
struct DeadRow {
    id: i64,
    domains: String,
    certificate_id: Option<i64>,
    force_ssl: i64,
    hsts: i64,
    hsts_subdomains: i64,
    http2: i64,
    advanced_snippet: Option<String>,
    enabled: i64,
    created_at: i64,
    updated_at: i64,
}

impl DeadRow {
    fn into_model(self) -> anyhow::Result<DeadHost> {
        Ok(DeadHost {
            id: self.id,
            domains: serde_json::from_str(&self.domains).context("domains json")?,
            certificate_id: self.certificate_id,
            force_ssl: self.force_ssl != 0,
            hsts: self.hsts != 0,
            hsts_subdomains: self.hsts_subdomains != 0,
            http2: self.http2 != 0,
            advanced_snippet: self.advanced_snippet,
            enabled: self.enabled != 0,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

const DEAD_COLUMNS: &str = "id, domains, certificate_id, force_ssl, hsts, hsts_subdomains, http2, \
     advanced_snippet, enabled, created_at, updated_at";

pub async fn list_dead(db: &SqlitePool) -> anyhow::Result<Vec<DeadHost>> {
    let rows: Vec<DeadRow> = sqlx::query_as(&format!(
        "SELECT {DEAD_COLUMNS} FROM dead_hosts ORDER BY id"
    ))
    .fetch_all(db)
    .await?;
    rows.into_iter().map(DeadRow::into_model).collect()
}

pub async fn get_dead(db: &SqlitePool, id: i64) -> anyhow::Result<Option<DeadHost>> {
    let row: Option<DeadRow> = sqlx::query_as(&format!(
        "SELECT {DEAD_COLUMNS} FROM dead_hosts WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(db)
    .await?;
    row.map(DeadRow::into_model).transpose()
}

pub async fn insert_dead(db: &SqlitePool, i: &DeadHostInput) -> anyhow::Result<i64> {
    let now = now_epoch();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO dead_hosts (domains, certificate_id, force_ssl, hsts, hsts_subdomains, \
         http2, advanced_snippet, enabled, created_at, updated_at) \
         VALUES (?,?,?,?,?,?,?,?,?,?) RETURNING id",
    )
    .bind(domains_json(&i.domains))
    .bind(i.certificate_id)
    .bind(i.force_ssl as i64)
    .bind(i.hsts as i64)
    .bind(i.hsts_subdomains as i64)
    .bind(i.http2 as i64)
    .bind(i.advanced_snippet.as_deref())
    .bind(i.enabled as i64)
    .bind(now)
    .bind(now)
    .fetch_one(db)
    .await?;
    Ok(id)
}

pub async fn update_dead(db: &SqlitePool, id: i64, i: &DeadHostInput) -> anyhow::Result<bool> {
    let rows = sqlx::query(
        "UPDATE dead_hosts SET domains=?, certificate_id=?, force_ssl=?, hsts=?, \
         hsts_subdomains=?, http2=?, advanced_snippet=?, enabled=?, updated_at=? WHERE id=?",
    )
    .bind(domains_json(&i.domains))
    .bind(i.certificate_id)
    .bind(i.force_ssl as i64)
    .bind(i.hsts as i64)
    .bind(i.hsts_subdomains as i64)
    .bind(i.http2 as i64)
    .bind(i.advanced_snippet.as_deref())
    .bind(i.enabled as i64)
    .bind(now_epoch())
    .bind(id)
    .execute(db)
    .await?;
    Ok(rows.rows_affected() > 0)
}

pub async fn delete_dead(db: &SqlitePool, id: i64) -> anyhow::Result<bool> {
    let rows = sqlx::query("DELETE FROM dead_hosts WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?;
    Ok(rows.rows_affected() > 0)
}

pub async fn set_dead_enabled(db: &SqlitePool, id: i64, enabled: bool) -> anyhow::Result<bool> {
    let rows = sqlx::query("UPDATE dead_hosts SET enabled=?, updated_at=? WHERE id=?")
        .bind(enabled as i64)
        .bind(now_epoch())
        .bind(id)
        .execute(db)
        .await?;
    Ok(rows.rows_affected() > 0)
}

// ---------------------------------------------------------------- streams

use crate::model::{Stream, StreamInput, StreamTls};

#[derive(sqlx::FromRow)]
struct StreamRow {
    id: i64,
    incoming_port: i64,
    forward_host: String,
    forward_port: i64,
    tcp: i64,
    udp: i64,
    tls: String,
    certificate_id: Option<i64>,
    enabled: i64,
    created_at: i64,
    updated_at: i64,
}

impl From<StreamRow> for Stream {
    fn from(r: StreamRow) -> Self {
        Stream {
            id: r.id,
            incoming_port: r.incoming_port as u16,
            forward_host: r.forward_host,
            forward_port: r.forward_port as u16,
            tcp: r.tcp != 0,
            udp: r.udp != 0,
            tls: StreamTls::from_stored(&r.tls),
            certificate_id: r.certificate_id,
            enabled: r.enabled != 0,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

const STREAM_COLUMNS: &str = "id, incoming_port, forward_host, forward_port, tcp, udp, tls, \
     certificate_id, enabled, created_at, updated_at";

pub async fn list_streams(db: &SqlitePool) -> anyhow::Result<Vec<Stream>> {
    let rows: Vec<StreamRow> =
        sqlx::query_as(&format!("SELECT {STREAM_COLUMNS} FROM streams ORDER BY id"))
            .fetch_all(db)
            .await?;
    Ok(rows.into_iter().map(Stream::from).collect())
}

pub async fn get_stream(db: &SqlitePool, id: i64) -> anyhow::Result<Option<Stream>> {
    let row: Option<StreamRow> = sqlx::query_as(&format!(
        "SELECT {STREAM_COLUMNS} FROM streams WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(Stream::from))
}

pub async fn insert_stream(db: &SqlitePool, i: &StreamInput) -> anyhow::Result<i64> {
    let now = now_epoch();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO streams (incoming_port, forward_host, forward_port, tcp, udp, tls, \
         certificate_id, enabled, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?) RETURNING id",
    )
    .bind(i.incoming_port as i64)
    .bind(&i.forward_host)
    .bind(i.forward_port as i64)
    .bind(i.tcp as i64)
    .bind(i.udp as i64)
    .bind(i.tls.as_str())
    .bind(i.certificate_id)
    .bind(i.enabled as i64)
    .bind(now)
    .bind(now)
    .fetch_one(db)
    .await?;
    Ok(id)
}

pub async fn update_stream(db: &SqlitePool, id: i64, i: &StreamInput) -> anyhow::Result<bool> {
    let rows = sqlx::query(
        "UPDATE streams SET incoming_port=?, forward_host=?, forward_port=?, tcp=?, udp=?, \
         tls=?, certificate_id=?, enabled=?, updated_at=? WHERE id=?",
    )
    .bind(i.incoming_port as i64)
    .bind(&i.forward_host)
    .bind(i.forward_port as i64)
    .bind(i.tcp as i64)
    .bind(i.udp as i64)
    .bind(i.tls.as_str())
    .bind(i.certificate_id)
    .bind(i.enabled as i64)
    .bind(now_epoch())
    .bind(id)
    .execute(db)
    .await?;
    Ok(rows.rows_affected() > 0)
}

pub async fn delete_stream(db: &SqlitePool, id: i64) -> anyhow::Result<bool> {
    let rows = sqlx::query("DELETE FROM streams WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?;
    Ok(rows.rows_affected() > 0)
}

pub async fn set_stream_enabled(db: &SqlitePool, id: i64, enabled: bool) -> anyhow::Result<bool> {
    let rows = sqlx::query("UPDATE streams SET enabled=?, updated_at=? WHERE id=?")
        .bind(enabled as i64)
        .bind(now_epoch())
        .bind(id)
        .execute(db)
        .await?;
    Ok(rows.rows_affected() > 0)
}

// ----------------------------------------------------------- SNI routers

use crate::model::{SniRoute, SniRouter, SniRouterInput};

#[derive(sqlx::FromRow)]
struct SniRouterRow {
    id: i64,
    name: String,
    incoming_port: i64,
    routes: String,
    default_host: String,
    default_port: i64,
    enabled: i64,
    created_at: i64,
    updated_at: i64,
}

fn sni_routes_json(routes: &[SniRoute]) -> String {
    serde_json::to_string(routes).unwrap_or_else(|_| "[]".into())
}

impl TryFrom<SniRouterRow> for SniRouter {
    type Error = anyhow::Error;
    fn try_from(r: SniRouterRow) -> anyhow::Result<Self> {
        Ok(SniRouter {
            id: r.id,
            name: r.name,
            incoming_port: r.incoming_port as u16,
            routes: serde_json::from_str(&r.routes).context("sni routes json")?,
            default_host: r.default_host,
            default_port: r.default_port as u16,
            enabled: r.enabled != 0,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
    }
}

const SNI_ROUTER_COLUMNS: &str = "id, name, incoming_port, routes, default_host, default_port, \
     enabled, created_at, updated_at";

pub async fn list_sni_routers(db: &SqlitePool) -> anyhow::Result<Vec<SniRouter>> {
    let rows: Vec<SniRouterRow> = sqlx::query_as(&format!(
        "SELECT {SNI_ROUTER_COLUMNS} FROM sni_routers ORDER BY id"
    ))
    .fetch_all(db)
    .await?;
    rows.into_iter().map(SniRouter::try_from).collect()
}

pub async fn get_sni_router(db: &SqlitePool, id: i64) -> anyhow::Result<Option<SniRouter>> {
    let row: Option<SniRouterRow> = sqlx::query_as(&format!(
        "SELECT {SNI_ROUTER_COLUMNS} FROM sni_routers WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(db)
    .await?;
    row.map(SniRouter::try_from).transpose()
}

pub async fn insert_sni_router(db: &SqlitePool, i: &SniRouterInput) -> anyhow::Result<i64> {
    let now = now_epoch();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO sni_routers (name, incoming_port, routes, default_host, default_port, \
         enabled, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?) RETURNING id",
    )
    .bind(&i.name)
    .bind(i.incoming_port as i64)
    .bind(sni_routes_json(&i.routes))
    .bind(&i.default_host)
    .bind(i.default_port as i64)
    .bind(i.enabled as i64)
    .bind(now)
    .bind(now)
    .fetch_one(db)
    .await?;
    Ok(id)
}

pub async fn update_sni_router(
    db: &SqlitePool,
    id: i64,
    i: &SniRouterInput,
) -> anyhow::Result<bool> {
    let rows = sqlx::query(
        "UPDATE sni_routers SET name=?, incoming_port=?, routes=?, default_host=?, \
         default_port=?, enabled=?, updated_at=? WHERE id=?",
    )
    .bind(&i.name)
    .bind(i.incoming_port as i64)
    .bind(sni_routes_json(&i.routes))
    .bind(&i.default_host)
    .bind(i.default_port as i64)
    .bind(i.enabled as i64)
    .bind(now_epoch())
    .bind(id)
    .execute(db)
    .await?;
    Ok(rows.rows_affected() > 0)
}

pub async fn delete_sni_router(db: &SqlitePool, id: i64) -> anyhow::Result<bool> {
    let rows = sqlx::query("DELETE FROM sni_routers WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?;
    Ok(rows.rows_affected() > 0)
}

pub async fn set_sni_router_enabled(
    db: &SqlitePool,
    id: i64,
    enabled: bool,
) -> anyhow::Result<bool> {
    let rows = sqlx::query("UPDATE sni_routers SET enabled=?, updated_at=? WHERE id=?")
        .bind(enabled as i64)
        .bind(now_epoch())
        .bind(id)
        .execute(db)
        .await?;
    Ok(rows.rows_affected() > 0)
}

/// Which host type owns a domain (for cross-type uniqueness messages).
#[derive(Debug, Clone, Copy)]
pub enum HostKind {
    Proxy,
    Redirect,
    Dead,
}

impl HostKind {
    pub fn label(self) -> &'static str {
        match self {
            HostKind::Proxy => "proxy host",
            HostKind::Redirect => "redirect host",
            HostKind::Dead => "404 host",
        }
    }
}

/// Every domain claimed by an ENABLED host of any type, with its owner. Used
/// to enforce domain uniqueness ACROSS proxy/redirect/dead hosts. `exclude`
/// skips the host being edited (its own kind + id).
pub async fn all_enabled_domains(
    db: &SqlitePool,
    exclude: Option<(HostKind, i64)>,
) -> anyhow::Result<std::collections::HashMap<String, (HostKind, i64)>> {
    let mut map = std::collections::HashMap::new();
    let skip = |k: HostKind, id: i64| matches!(exclude, Some((ek, eid)) if std::mem::discriminant(&ek) == std::mem::discriminant(&k) && eid == id);
    for h in list_hosts(db).await? {
        if h.enabled && !skip(HostKind::Proxy, h.id) {
            for d in h.domains {
                map.insert(d, (HostKind::Proxy, h.id));
            }
        }
    }
    for h in list_redirects(db).await? {
        if h.enabled && !skip(HostKind::Redirect, h.id) {
            for d in h.domains {
                map.insert(d, (HostKind::Redirect, h.id));
            }
        }
    }
    for h in list_dead(db).await? {
        if h.enabled && !skip(HostKind::Dead, h.id) {
            for d in h.domains {
                map.insert(d, (HostKind::Dead, h.id));
            }
        }
    }
    Ok(map)
}

// ----------------------------------------------------------- certificates

#[derive(sqlx::FromRow)]
struct CertRow {
    id: i64,
    name: String,
    domains: String,
    challenge: String,
    key_type: String,
    email: Option<String>,
    staging: i64,
    dns_provider: Option<String>,
    created_at: i64,
}

fn challenge_from_str(s: &str) -> Challenge {
    match s {
        "dns" => Challenge::Dns,
        "alpn" => Challenge::Alpn,
        _ => Challenge::Http,
    }
}

fn key_type_from_str(s: &str) -> KeyType {
    match s {
        "rsa" => KeyType::Rsa,
        _ => KeyType::Ecdsa,
    }
}

impl CertRow {
    fn into_model(self) -> anyhow::Result<Certificate> {
        Ok(Certificate {
            id: self.id,
            name: self.name,
            domains: serde_json::from_str(&self.domains).context("cert domains json")?,
            challenge: challenge_from_str(&self.challenge),
            key_type: key_type_from_str(&self.key_type),
            email: self.email,
            staging: self.staging != 0,
            dns_provider: self.dns_provider,
            created_at: self.created_at,
        })
    }
}

const CERT_COLUMNS: &str =
    "id, name, domains, challenge, key_type, email, staging, dns_provider, created_at";

pub async fn list_certs(db: &SqlitePool) -> anyhow::Result<Vec<Certificate>> {
    let rows: Vec<CertRow> = sqlx::query_as(&format!(
        "SELECT {CERT_COLUMNS} FROM certificates ORDER BY id"
    ))
    .fetch_all(db)
    .await?;
    rows.into_iter().map(CertRow::into_model).collect()
}

pub async fn get_cert(db: &SqlitePool, id: i64) -> anyhow::Result<Option<Certificate>> {
    let row: Option<CertRow> = sqlx::query_as(&format!(
        "SELECT {CERT_COLUMNS} FROM certificates WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(db)
    .await?;
    row.map(CertRow::into_model).transpose()
}

pub async fn cert_name_exists(db: &SqlitePool, name: &str) -> anyhow::Result<bool> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM certificates WHERE name = ?")
        .bind(name)
        .fetch_one(db)
        .await?;
    Ok(n > 0)
}

/// Like [`cert_name_exists`] but ignores one row — used when updating a
/// certificate, so keeping its own name doesn't count as a clash.
pub async fn cert_name_exists_except(
    db: &SqlitePool,
    name: &str,
    except_id: i64,
) -> anyhow::Result<bool> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM certificates WHERE name = ? AND id <> ?")
        .bind(name)
        .bind(except_id)
        .fetch_one(db)
        .await?;
    Ok(n > 0)
}

pub async fn insert_cert(db: &SqlitePool, input: &CertificateInput) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO certificates (name, domains, challenge, key_type, email, staging, \
         dns_provider, created_at) VALUES (?,?,?,?,?,?,?,?) RETURNING id",
    )
    .bind(&input.name)
    .bind(domains_json(&input.domains))
    .bind(input.challenge.as_str())
    .bind(input.key_type.as_str())
    .bind(input.email.as_deref())
    .bind(input.staging as i64)
    .bind(input.dns_provider.as_deref())
    .bind(now_epoch())
    .fetch_one(db)
    .await?;
    Ok(id)
}

/// Update a certificate in place, keeping its `id` (and therefore every host's
/// `certificate_id` binding) and `created_at`. Changing name/domains/challenge
/// makes Angie re-issue on the next apply — the row is the source of truth, the
/// generator reads it fresh. Returns false if no row had that id.
pub async fn update_cert(
    db: &SqlitePool,
    id: i64,
    input: &CertificateInput,
) -> anyhow::Result<bool> {
    let rows = sqlx::query(
        "UPDATE certificates SET name = ?, domains = ?, challenge = ?, key_type = ?, \
         email = ?, staging = ?, dns_provider = ? WHERE id = ?",
    )
    .bind(&input.name)
    .bind(domains_json(&input.domains))
    .bind(input.challenge.as_str())
    .bind(input.key_type.as_str())
    .bind(input.email.as_deref())
    .bind(input.staging as i64)
    .bind(input.dns_provider.as_deref())
    .bind(id)
    .execute(db)
    .await?;
    Ok(rows.rows_affected() > 0)
}

pub async fn delete_cert(db: &SqlitePool, id: i64) -> anyhow::Result<bool> {
    let rows = sqlx::query("DELETE FROM certificates WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?;
    Ok(rows.rows_affected() > 0)
}

// ------------------------------------------------- DNS credential profiles

#[derive(sqlx::FromRow)]
struct DnsCredentialRow {
    id: i64,
    provider: String,
    name: String,
    created_at: i64,
}

impl From<DnsCredentialRow> for DnsCredential {
    fn from(r: DnsCredentialRow) -> Self {
        DnsCredential {
            id: r.id,
            provider: r.provider,
            name: r.name,
            created_at: r.created_at,
        }
    }
}

pub async fn list_dns_credentials(db: &SqlitePool) -> anyhow::Result<Vec<DnsCredential>> {
    let rows: Vec<DnsCredentialRow> =
        sqlx::query_as("SELECT id, provider, name, created_at FROM dns_credentials ORDER BY id")
            .fetch_all(db)
            .await?;
    Ok(rows.into_iter().map(DnsCredential::from).collect())
}

pub async fn get_dns_credential(db: &SqlitePool, id: i64) -> anyhow::Result<Option<DnsCredential>> {
    let row: Option<DnsCredentialRow> =
        sqlx::query_as("SELECT id, provider, name, created_at FROM dns_credentials WHERE id = ?")
            .bind(id)
            .fetch_optional(db)
            .await?;
    Ok(row.map(DnsCredential::from))
}

pub async fn insert_dns_credential(
    db: &SqlitePool,
    input: &DnsCredentialInput,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO dns_credentials (provider, name, created_at) VALUES (?,?,?) RETURNING id",
    )
    .bind(&input.provider)
    .bind(&input.name)
    .bind(now_epoch())
    .fetch_one(db)
    .await?;
    Ok(id)
}

/// Rename a profile (the provider type is immutable). Returns false if missing.
pub async fn update_dns_credential_name(
    db: &SqlitePool,
    id: i64,
    name: &str,
) -> anyhow::Result<bool> {
    let rows = sqlx::query("UPDATE dns_credentials SET name = ? WHERE id = ?")
        .bind(name)
        .bind(id)
        .execute(db)
        .await?;
    Ok(rows.rows_affected() > 0)
}

/// Delete a profile and all its stored credential settings (`dns_cred:<id>:*`).
pub async fn delete_dns_credential(db: &SqlitePool, id: i64) -> anyhow::Result<bool> {
    let mut tx = db.begin().await?;
    let rows = sqlx::query("DELETE FROM dns_credentials WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM settings WHERE key LIKE ?")
        .bind(format!("dns_cred:{id}:%"))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(rows.rows_affected() > 0)
}

/// Whether any certificate references this credential profile (by its id).
pub async fn dns_credential_in_use(db: &SqlitePool, id: i64) -> anyhow::Result<bool> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM certificates WHERE dns_provider = ?")
        .bind(id.to_string())
        .fetch_one(db)
        .await?;
    Ok(n > 0)
}

/// Transactionally REPLACE all hosts + certificates with an imported set
/// (config import). Every input is already validated by the caller. Explicit
/// ids are preserved so hosts keep their certificate_id references. The admin
/// user, sessions, and apply history are untouched; settings are upserted.
#[allow(clippy::too_many_arguments)]
pub async fn import_replace(
    db: &SqlitePool,
    certs: &[(i64, CertificateInput)],
    access_lists: &[AclImportRow],
    hosts: &[(i64, ProxyHostInput)],
    redirects: &[(i64, RedirectHostInput)],
    deads: &[(i64, DeadHostInput)],
    streams: &[(i64, StreamInput)],
    sni_routers: &[(i64, SniRouterInput)],
    dns_credentials: &[(i64, DnsCredentialInput)],
    bans: &[(i64, BanInput)],
    settings: &std::collections::HashMap<String, String>,
) -> anyhow::Result<()> {
    let now = now_epoch();
    let mut tx = db.begin().await?;

    // Clear children (which reference certs/access lists) before parents, so a
    // full replace works whether or not FK enforcement is on.
    for table in [
        "proxy_hosts",
        "redirect_hosts",
        "dead_hosts",
        "streams",
        "sni_routers",
        "dns_credentials",
        "bans",
        "access_list_users",
        "access_list_clients",
        "access_lists",
        "certificates",
    ] {
        sqlx::query(&format!("DELETE FROM {table}"))
            .execute(&mut *tx)
            .await?;
    }

    // Imported credential profiles keep their ids but arrive WITHOUT secrets
    // (creds are never exported). Purge stale `dns_cred:*` values so a restored
    // profile can't silently inherit the previous install's credentials for the
    // same id. `delete_dns_credential` does this per-profile; the bulk import
    // must too.
    sqlx::query("DELETE FROM settings WHERE key LIKE 'dns_cred:%'")
        .execute(&mut *tx)
        .await?;

    for (id, c) in certs {
        sqlx::query(
            "INSERT INTO certificates (id, name, domains, challenge, key_type, email, staging, \
             dns_provider, created_at) VALUES (?,?,?,?,?,?,?,?,?)",
        )
        .bind(id)
        .bind(&c.name)
        .bind(domains_json(&c.domains))
        .bind(c.challenge.as_str())
        .bind(c.key_type.as_str())
        .bind(c.email.as_deref())
        .bind(c.staging as i64)
        .bind(c.dns_provider.as_deref())
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    for (id, dc) in dns_credentials {
        sqlx::query(
            "INSERT INTO dns_credentials (id, provider, name, created_at) VALUES (?,?,?,?)",
        )
        .bind(id)
        .bind(&dc.provider)
        .bind(&dc.name)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    for l in access_lists {
        sqlx::query(
            "INSERT INTO access_lists (id, name, satisfy, pass_auth, created_at) VALUES (?,?,?,?,?)",
        )
        .bind(l.id)
        .bind(&l.name)
        .bind(l.satisfy.as_str())
        .bind(l.pass_auth as i64)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        for u in &l.users {
            sqlx::query(
                "INSERT INTO access_list_users (access_list_id, username, password_hash) VALUES (?,?,?)",
            )
            .bind(l.id)
            .bind(&u.username)
            .bind(&u.password_hash)
            .execute(&mut *tx)
            .await?;
        }
        for (directive, address) in &l.clients {
            sqlx::query(
                "INSERT INTO access_list_clients (access_list_id, directive, address) VALUES (?,?,?)",
            )
            .bind(l.id)
            .bind(directive.as_str())
            .bind(address)
            .execute(&mut *tx)
            .await?;
        }
    }

    for (id, h) in hosts {
        sqlx::query(
            "INSERT INTO proxy_hosts (id, domains, forward_scheme, forward_host, forward_port, \
             websockets_upgrade, block_exploits, cache_assets, http2, http3, force_ssl, hsts, \
             hsts_subdomains, trust_forwarded_proto, certificate_id, access_list_id, locations, \
             advanced_snippet, rate_limit, upstream, mtls, forward_auth, custom_headers, \
             maintenance, gzip, error_pages, proxy_tuning, enabled, created_at, updated_at) \
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(id)
        .bind(domains_json(&h.domains))
        .bind(h.forward_scheme.as_str())
        .bind(&h.forward_host)
        .bind(h.forward_port as i64)
        .bind(h.websockets_upgrade as i64)
        .bind(h.block_exploits as i64)
        .bind(h.cache_assets as i64)
        .bind(h.http2 as i64)
        .bind(h.http3 as i64)
        .bind(h.force_ssl as i64)
        .bind(h.hsts as i64)
        .bind(h.hsts_subdomains as i64)
        .bind(h.trust_forwarded_proto as i64)
        .bind(h.certificate_id)
        .bind(h.access_list_id)
        .bind(locations_json(&h.locations))
        .bind(h.advanced_snippet.as_deref())
        .bind(rate_limit_json(&h.rate_limit))
        .bind(upstream_json(&h.upstream))
        .bind(mtls_json(&h.mtls))
        .bind(forward_auth_json(&h.forward_auth))
        .bind(custom_headers_json(&h.custom_headers))
        .bind(maintenance_json(&h.maintenance))
        .bind(gzip_json(&h.gzip))
        .bind(error_pages_json(&h.error_pages))
        .bind(proxy_tuning_json(&h.proxy_tuning))
        .bind(h.enabled as i64)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    for (id, r) in redirects {
        sqlx::query(
            "INSERT INTO redirect_hosts (id, domains, forward_scheme, forward_domain, \
             forward_http_code, preserve_path, certificate_id, force_ssl, hsts, hsts_subdomains, \
             http2, block_exploits, advanced_snippet, enabled, created_at, updated_at) \
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(id)
        .bind(domains_json(&r.domains))
        .bind(redirect_scheme_str(r.forward_scheme))
        .bind(&r.forward_domain)
        .bind(r.forward_http_code as i64)
        .bind(r.preserve_path as i64)
        .bind(r.certificate_id)
        .bind(r.force_ssl as i64)
        .bind(r.hsts as i64)
        .bind(r.hsts_subdomains as i64)
        .bind(r.http2 as i64)
        .bind(r.block_exploits as i64)
        .bind(r.advanced_snippet.as_deref())
        .bind(r.enabled as i64)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    for (id, d) in deads {
        sqlx::query(
            "INSERT INTO dead_hosts (id, domains, certificate_id, force_ssl, hsts, hsts_subdomains, \
             http2, advanced_snippet, enabled, created_at, updated_at) \
             VALUES (?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(id)
        .bind(domains_json(&d.domains))
        .bind(d.certificate_id)
        .bind(d.force_ssl as i64)
        .bind(d.hsts as i64)
        .bind(d.hsts_subdomains as i64)
        .bind(d.http2 as i64)
        .bind(d.advanced_snippet.as_deref())
        .bind(d.enabled as i64)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    for (id, s) in streams {
        sqlx::query(
            "INSERT INTO streams (id, incoming_port, forward_host, forward_port, tcp, udp, \
             tls, certificate_id, enabled, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(id)
        .bind(s.incoming_port as i64)
        .bind(&s.forward_host)
        .bind(s.forward_port as i64)
        .bind(s.tcp as i64)
        .bind(s.udp as i64)
        .bind(s.tls.as_str())
        .bind(s.certificate_id)
        .bind(s.enabled as i64)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    for (id, r) in sni_routers {
        sqlx::query(
            "INSERT INTO sni_routers (id, name, incoming_port, routes, default_host, \
             default_port, enabled, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?)",
        )
        .bind(id)
        .bind(&r.name)
        .bind(r.incoming_port as i64)
        .bind(sni_routes_json(&r.routes))
        .bind(&r.default_host)
        .bind(r.default_port as i64)
        .bind(r.enabled as i64)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    for (id, b) in bans {
        sqlx::query("INSERT INTO bans (id, address, reason, created_at) VALUES (?,?,?,?)")
            .bind(id)
            .bind(&b.address)
            .bind(b.reason.as_deref())
            .bind(now)
            .execute(&mut *tx)
            .await?;
    }

    for (k, v) in settings {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(k)
        .bind(v)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// Hosts of any type (label + first domain) that reference this certificate.
pub async fn hosts_using_cert(db: &SqlitePool, cert_id: i64) -> anyhow::Result<Vec<String>> {
    let mut out = Vec::new();
    let first = |d: &[String]| d.first().cloned().unwrap_or_default();
    for h in list_hosts(db).await? {
        if h.certificate_id == Some(cert_id) {
            out.push(format!("proxy #{} ({})", h.id, first(&h.domains)));
        }
    }
    for h in list_redirects(db).await? {
        if h.certificate_id == Some(cert_id) {
            out.push(format!("redirect #{} ({})", h.id, first(&h.domains)));
        }
    }
    for h in list_dead(db).await? {
        if h.certificate_id == Some(cert_id) {
            out.push(format!("404 #{} ({})", h.id, first(&h.domains)));
        }
    }
    for s in list_streams(db).await? {
        if s.certificate_id == Some(cert_id) {
            out.push(format!("stream #{} (:{})", s.id, s.incoming_port));
        }
    }
    Ok(out)
}

// ----------------------------------------------------------- access lists

use crate::model::{
    AccessList, AccessListClient, AccessListInput, AccessListUser, Directive, Satisfy,
};

fn satisfy_from_str(s: &str) -> Satisfy {
    match s {
        "any" => Satisfy::Any,
        _ => Satisfy::All,
    }
}
fn directive_from_str(s: &str) -> Directive {
    match s {
        "allow" => Directive::Allow,
        _ => Directive::Deny,
    }
}

/// A user with the bcrypt hash (internal — the API never exposes the hash).
pub struct AclUserHash {
    pub username: String,
    pub password_hash: String,
}

/// One access list from an import document: explicit id + already-validated
/// user hashes and clients. Used only by [`import_replace`].
pub struct AclImportRow {
    pub id: i64,
    pub name: String,
    pub satisfy: Satisfy,
    pub pass_auth: bool,
    pub users: Vec<AclUserHash>,
    pub clients: Vec<(Directive, String)>,
}

pub async fn list_access_lists(db: &SqlitePool) -> anyhow::Result<Vec<AccessList>> {
    let rows: Vec<(i64, String, String, i64, i64)> = sqlx::query_as(
        "SELECT id, name, satisfy, pass_auth, created_at FROM access_lists ORDER BY id",
    )
    .fetch_all(db)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for (id, name, satisfy, pass_auth, created_at) in rows {
        out.push(AccessList {
            id,
            name,
            satisfy: satisfy_from_str(&satisfy),
            pass_auth: pass_auth != 0,
            users: acl_users(db, id).await?,
            clients: acl_clients(db, id).await?,
            created_at,
        });
    }
    Ok(out)
}

pub async fn get_access_list(db: &SqlitePool, id: i64) -> anyhow::Result<Option<AccessList>> {
    let row: Option<(i64, String, String, i64, i64)> = sqlx::query_as(
        "SELECT id, name, satisfy, pass_auth, created_at FROM access_lists WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(db)
    .await?;
    let Some((id, name, satisfy, pass_auth, created_at)) = row else {
        return Ok(None);
    };
    Ok(Some(AccessList {
        id,
        name,
        satisfy: satisfy_from_str(&satisfy),
        pass_auth: pass_auth != 0,
        users: acl_users(db, id).await?,
        clients: acl_clients(db, id).await?,
        created_at,
    }))
}

async fn acl_users(db: &SqlitePool, list_id: i64) -> anyhow::Result<Vec<AccessListUser>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT username FROM access_list_users WHERE access_list_id = ? ORDER BY id",
    )
    .bind(list_id)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(username,)| AccessListUser {
            username,
            has_password: true,
        })
        .collect())
}

/// Full users with hashes — used when regenerating the htpasswd file and when
/// preserving existing passwords on update.
pub async fn acl_user_hashes(db: &SqlitePool, list_id: i64) -> anyhow::Result<Vec<AclUserHash>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT username, password_hash FROM access_list_users WHERE access_list_id = ? ORDER BY id",
    )
    .bind(list_id)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(username, password_hash)| AclUserHash {
            username,
            password_hash,
        })
        .collect())
}

async fn acl_clients(db: &SqlitePool, list_id: i64) -> anyhow::Result<Vec<AccessListClient>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT directive, address FROM access_list_clients WHERE access_list_id = ? ORDER BY id",
    )
    .bind(list_id)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(directive, address)| AccessListClient {
            directive: directive_from_str(&directive),
            address,
        })
        .collect())
}

/// Insert or update an access list and its users/clients in one transaction.
/// `user_hashes` is the resolved final user set (passwords already hashed /
/// preserved by the caller). Pass `id = None` to insert; returns the id.
pub async fn upsert_access_list(
    db: &SqlitePool,
    id: Option<i64>,
    input: &AccessListInput,
    user_hashes: &[AclUserHash],
) -> anyhow::Result<i64> {
    let mut tx = db.begin().await?;
    let list_id = match id {
        Some(id) => {
            sqlx::query(
                "UPDATE access_lists SET name = ?, satisfy = ?, pass_auth = ? WHERE id = ?",
            )
            .bind(&input.name)
            .bind(input.satisfy.as_str())
            .bind(input.pass_auth as i64)
            .bind(id)
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM access_list_users WHERE access_list_id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM access_list_clients WHERE access_list_id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            id
        }
        None => {
            sqlx::query_scalar(
                "INSERT INTO access_lists (name, satisfy, pass_auth, created_at) \
             VALUES (?,?,?,?) RETURNING id",
            )
            .bind(&input.name)
            .bind(input.satisfy.as_str())
            .bind(input.pass_auth as i64)
            .bind(now_epoch())
            .fetch_one(&mut *tx)
            .await?
        }
    };

    for u in user_hashes {
        sqlx::query(
            "INSERT INTO access_list_users (access_list_id, username, password_hash) VALUES (?,?,?)",
        )
        .bind(list_id)
        .bind(&u.username)
        .bind(&u.password_hash)
        .execute(&mut *tx)
        .await?;
    }
    for c in &input.clients {
        sqlx::query(
            "INSERT INTO access_list_clients (access_list_id, directive, address) VALUES (?,?,?)",
        )
        .bind(list_id)
        .bind(c.directive.as_str())
        .bind(&c.address)
        .execute(&mut *tx)
        .await?;
    }
    // Bump hosts referencing this list so the apply preview picks up the change.
    sqlx::query("UPDATE proxy_hosts SET updated_at = ? WHERE access_list_id = ?")
        .bind(now_epoch())
        .bind(list_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(list_id)
}

pub async fn delete_access_list(db: &SqlitePool, id: i64) -> anyhow::Result<bool> {
    let rows = sqlx::query("DELETE FROM access_lists WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?;
    Ok(rows.rows_affected() > 0)
}

/// Hosts (id + first domain) referencing this access list.
pub async fn hosts_using_access_list(
    db: &SqlitePool,
    list_id: i64,
) -> anyhow::Result<Vec<(i64, String)>> {
    let hosts = list_hosts(db).await?;
    Ok(hosts
        .into_iter()
        .filter(|h| h.access_list_id == Some(list_id))
        .map(|h| (h.id, h.domains.first().cloned().unwrap_or_default()))
        .collect())
}

// -------------------------------------------------------------- settings

pub async fn set_setting(db: &SqlitePool, key: &str, value: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn get_setting(db: &SqlitePool, key: &str) -> anyhow::Result<Option<String>> {
    let v: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(db)
        .await?;
    Ok(v)
}

pub async fn all_settings(
    db: &SqlitePool,
) -> anyhow::Result<std::collections::HashMap<String, String>> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM settings")
        .fetch_all(db)
        .await?;
    Ok(rows.into_iter().collect())
}

// ---------------------------------------------------------- apply history

pub async fn record_apply(
    db: &SqlitePool,
    db_revision: i64,
    result: &str,
    report_json: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO apply_history (timestamp, db_revision, result, report) VALUES (?,?,?,?)",
    )
    .bind(now_epoch())
    .bind(db_revision)
    .bind(result)
    .bind(report_json)
    .execute(db)
    .await?;
    // Prune to the newest rows — each report embeds the full diff, so on a
    // long-lived device (the reconciler applies whenever a cert becomes ready)
    // this table would otherwise grow unbounded (audit finding). Mirrors the
    // audit_log cap.
    sqlx::query(
        "DELETE FROM apply_history WHERE id NOT IN \
         (SELECT id FROM apply_history ORDER BY id DESC LIMIT ?)",
    )
    .bind(APPLY_HISTORY_KEEP)
    .execute(db)
    .await?;
    Ok(())
}

const APPLY_HISTORY_KEEP: i64 = 500;

pub struct ApplyHistoryEntry {
    pub id: i64,
    pub timestamp: i64,
    pub result: String,
    pub report: String,
}

pub async fn list_apply_history(
    db: &SqlitePool,
    limit: i64,
) -> anyhow::Result<Vec<ApplyHistoryEntry>> {
    let rows: Vec<(i64, i64, String, String)> = sqlx::query_as(
        "SELECT id, timestamp, result, report FROM apply_history ORDER BY id DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, timestamp, result, report)| ApplyHistoryEntry {
            id,
            timestamp,
            result,
            report,
        })
        .collect())
}

// ------------------------------------------------------------------- users

/// A panel operator as returned to the admin UI (never includes the hash).
pub struct UserRow {
    pub id: i64,
    pub email: String,
    pub role: String,
    pub created_at: i64,
}

pub async fn list_users(db: &SqlitePool) -> anyhow::Result<Vec<UserRow>> {
    let rows: Vec<(i64, String, String, i64)> =
        sqlx::query_as("SELECT id, email, role, created_at FROM users ORDER BY id")
            .fetch_all(db)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(id, email, role, created_at)| UserRow {
            id,
            email,
            role,
            created_at,
        })
        .collect())
}

pub async fn get_user(db: &SqlitePool, id: i64) -> anyhow::Result<Option<UserRow>> {
    let row: Option<(i64, String, String, i64)> =
        sqlx::query_as("SELECT id, email, role, created_at FROM users WHERE id = ?")
            .bind(id)
            .fetch_optional(db)
            .await?;
    Ok(row.map(|(id, email, role, created_at)| UserRow {
        id,
        email,
        role,
        created_at,
    }))
}

pub async fn user_email_exists(db: &SqlitePool, email: &str) -> anyhow::Result<bool> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email = ?")
        .bind(email)
        .fetch_one(db)
        .await?;
    Ok(n > 0)
}

/// Insert a user with an already-hashed password. Returns the new id.
pub async fn insert_user(
    db: &SqlitePool,
    email: &str,
    password_hash: &str,
    role: &str,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO users (email, password_hash, role, created_at) VALUES (?,?,?,?) RETURNING id",
    )
    .bind(email)
    .bind(password_hash)
    .bind(role)
    .bind(now_epoch())
    .fetch_one(db)
    .await?;
    Ok(id)
}

pub async fn set_user_role(db: &SqlitePool, id: i64, role: &str) -> anyhow::Result<bool> {
    let rows = sqlx::query("UPDATE users SET role = ? WHERE id = ?")
        .bind(role)
        .bind(id)
        .execute(db)
        .await?;
    Ok(rows.rows_affected() > 0)
}

pub async fn set_user_password(db: &SqlitePool, id: i64, hash: &str) -> anyhow::Result<bool> {
    let rows = sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(hash)
        .bind(id)
        .execute(db)
        .await?;
    Ok(rows.rows_affected() > 0)
}

pub async fn user_password_hash(db: &SqlitePool, id: i64) -> anyhow::Result<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
            .bind(id)
            .fetch_optional(db)
            .await?,
    )
}

/// Delete a user and revoke all their sessions in one transaction.
pub async fn delete_user(db: &SqlitePool, id: i64) -> anyhow::Result<bool> {
    let mut tx = db.begin().await?;
    sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let rows = sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(rows.rows_affected() > 0)
}

/// Count admins (for the "never remove the last admin" guard).
pub async fn count_admins(db: &SqlitePool) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin'")
            .fetch_one(db)
            .await?,
    )
}

// --------------------------------------------------------------- ip blocklist

use crate::model::{Ban, BanInput};

pub async fn list_bans(db: &SqlitePool) -> anyhow::Result<Vec<Ban>> {
    let rows: Vec<(i64, String, Option<String>, i64)> =
        sqlx::query_as("SELECT id, address, reason, created_at FROM bans ORDER BY id")
            .fetch_all(db)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(id, address, reason, created_at)| Ban {
            id,
            address,
            reason,
            created_at,
        })
        .collect())
}

pub async fn ban_address_exists(db: &SqlitePool, address: &str) -> anyhow::Result<bool> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bans WHERE address = ?")
        .bind(address)
        .fetch_one(db)
        .await?;
    Ok(n > 0)
}

pub async fn insert_ban(db: &SqlitePool, input: &BanInput) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO bans (address, reason, created_at) VALUES (?,?,?) RETURNING id",
    )
    .bind(&input.address)
    .bind(input.reason.as_deref())
    .bind(now_epoch())
    .fetch_one(db)
    .await?;
    Ok(id)
}

pub async fn get_ban(db: &SqlitePool, id: i64) -> anyhow::Result<Option<Ban>> {
    let row: Option<(i64, String, Option<String>, i64)> =
        sqlx::query_as("SELECT id, address, reason, created_at FROM bans WHERE id = ?")
            .bind(id)
            .fetch_optional(db)
            .await?;
    Ok(row.map(|(id, address, reason, created_at)| Ban {
        id,
        address,
        reason,
        created_at,
    }))
}

pub async fn delete_ban(db: &SqlitePool, id: i64) -> anyhow::Result<bool> {
    let rows = sqlx::query("DELETE FROM bans WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?;
    Ok(rows.rows_affected() > 0)
}

// ------------------------------------------------------------- audit log

/// Keep at most this many audit rows — the table is pruned to the newest on
/// each insert so a long-running panel can't grow it without bound.
const AUDIT_KEEP: i64 = 2000;

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct AuditEntry {
    pub id: i64,
    pub user_email: Option<String>,
    pub method: String,
    pub path: String,
    pub status: i64,
    pub created_at: i64,
}

/// Record one audited action, then prune to the newest [`AUDIT_KEEP`] rows.
pub async fn insert_audit(
    db: &SqlitePool,
    user_email: Option<&str>,
    method: &str,
    path: &str,
    status: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO audit_log (user_email, method, path, status, created_at) \
         VALUES (?,?,?,?,?)",
    )
    .bind(user_email)
    .bind(method)
    .bind(path)
    .bind(status)
    .bind(now_epoch())
    .execute(db)
    .await?;
    sqlx::query(
        "DELETE FROM audit_log WHERE id NOT IN \
         (SELECT id FROM audit_log ORDER BY id DESC LIMIT ?)",
    )
    .bind(AUDIT_KEEP)
    .execute(db)
    .await?;
    Ok(())
}

/// The most recent audit entries, newest first (bounded by `limit`).
pub async fn list_audit(db: &SqlitePool, limit: i64) -> anyhow::Result<Vec<AuditEntry>> {
    let rows: Vec<AuditEntry> = sqlx::query_as(
        "SELECT id, user_email, method, path, status, created_at \
         FROM audit_log ORDER BY id DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(db)
    .await?;
    Ok(rows)
}
