//! Persistence for proxy hosts, settings, and apply history. Runtime sqlx
//! queries (no compile-time DB needed). JSON columns (domains, locations) are
//! stored as TEXT and parsed at the boundary; booleans as INTEGER 0/1.

use anyhow::Context;
use sqlx::SqlitePool;

use crate::db::now_epoch;
use crate::model::{CustomLocation, ProxyHost, ProxyHostInput, Scheme};

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
    force_ssl: i64,
    hsts: i64,
    hsts_subdomains: i64,
    trust_forwarded_proto: i64,
    certificate_id: Option<i64>,
    locations: String,
    advanced_snippet: Option<String>,
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
            force_ssl: self.force_ssl != 0,
            hsts: self.hsts != 0,
            hsts_subdomains: self.hsts_subdomains != 0,
            trust_forwarded_proto: self.trust_forwarded_proto != 0,
            certificate_id: self.certificate_id,
            locations: serde_json::from_str(&self.locations).context("locations json")?,
            advanced_snippet: self.advanced_snippet,
            enabled: self.enabled != 0,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

const HOST_COLUMNS: &str = "id, domains, forward_scheme, forward_host, forward_port, \
     websockets_upgrade, block_exploits, cache_assets, http2, force_ssl, hsts, \
     hsts_subdomains, trust_forwarded_proto, certificate_id, locations, \
     advanced_snippet, enabled, created_at, updated_at";

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
         websockets_upgrade, block_exploits, cache_assets, http2, force_ssl, hsts, \
         hsts_subdomains, trust_forwarded_proto, certificate_id, locations, \
         advanced_snippet, enabled, created_at, updated_at) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) RETURNING id",
    )
    .bind(domains_json(&input.domains))
    .bind(input.forward_scheme.as_str())
    .bind(&input.forward_host)
    .bind(input.forward_port as i64)
    .bind(input.websockets_upgrade as i64)
    .bind(input.block_exploits as i64)
    .bind(input.cache_assets as i64)
    .bind(input.http2 as i64)
    .bind(input.force_ssl as i64)
    .bind(input.hsts as i64)
    .bind(input.hsts_subdomains as i64)
    .bind(input.trust_forwarded_proto as i64)
    .bind(input.certificate_id)
    .bind(locations_json(&input.locations))
    .bind(input.advanced_snippet.as_deref())
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
         websockets_upgrade=?, block_exploits=?, cache_assets=?, http2=?, force_ssl=?, hsts=?, \
         hsts_subdomains=?, trust_forwarded_proto=?, certificate_id=?, locations=?, \
         advanced_snippet=?, enabled=?, updated_at=? WHERE id=?",
    )
    .bind(domains_json(&input.domains))
    .bind(input.forward_scheme.as_str())
    .bind(&input.forward_host)
    .bind(input.forward_port as i64)
    .bind(input.websockets_upgrade as i64)
    .bind(input.block_exploits as i64)
    .bind(input.cache_assets as i64)
    .bind(input.http2 as i64)
    .bind(input.force_ssl as i64)
    .bind(input.hsts as i64)
    .bind(input.hsts_subdomains as i64)
    .bind(input.trust_forwarded_proto as i64)
    .bind(input.certificate_id)
    .bind(locations_json(&input.locations))
    .bind(input.advanced_snippet.as_deref())
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

/// DB revision = max(updated_at) across hosts (used to reject a stale apply
/// against a preview computed from an older state — PLAN.md §2.2).
pub async fn hosts_revision(db: &SqlitePool) -> anyhow::Result<i64> {
    let rev: Option<i64> = sqlx::query_scalar("SELECT MAX(updated_at) FROM proxy_hosts")
        .fetch_one(db)
        .await?;
    Ok(rev.unwrap_or(0))
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
    Ok(())
}

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
