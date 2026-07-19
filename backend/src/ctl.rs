//! `apctl` — the operator CLI, a thin client over the panel's own REST API.
//!
//! It deliberately does **not** touch the database. Everything the panel does
//! on a write — validation, the staged diff, `angie -t`, the atomic swap with
//! rollback, the audit row — lives behind the API, and a CLI that went around
//! it would be a second, weaker implementation of the apply pipeline.
//!
//! Reached two ways: `angie-panel ctl <cmd>`, or `apctl <cmd>` via a symlink
//! that `main` dispatches on argv[0]. Same enum, same behaviour.
//!
//! Credentials, in order: `--token`, `$ANGIE_PANEL_TOKEN`, then the local token
//! file in the data dir — which is what makes it zero-config on the box itself.

use std::path::PathBuf;

use anyhow::{bail, Context};
use clap::Subcommand;
use serde_json::{json, Value};

use crate::auth::CLI_TOKEN_FILE;
use crate::config::PanelConfig;

#[derive(Subcommand)]
pub enum CtlCommand {
    /// Panel, Angie and D-Bus health at a glance
    Status,
    /// Show what an apply would change, without touching anything
    Diff,
    /// Generate, validate and reload the Angie configuration
    Apply {
        /// Print the diff and exit without applying
        #[arg(long)]
        dry_run: bool,
    },
    /// Proxy hosts
    #[command(subcommand)]
    Host(HostCommand),
    /// Certificates
    #[command(subcommand)]
    Cert(CertCommand),
    /// Dump the full configuration as JSON (stdout)
    Export,
    /// Load a configuration dump produced by `export`
    Import {
        /// File to read, or `-` for stdin
        file: String,
    },
}

#[derive(Subcommand)]
pub enum HostCommand {
    /// List proxy hosts
    Ls,
    /// Add a proxy host
    Add {
        /// Domain(s) served by this host
        #[arg(required = true)]
        domains: Vec<String>,
        /// Upstream, e.g. http://127.0.0.1:3000
        #[arg(long, value_name = "URL")]
        to: String,
        /// Bind an existing certificate by id (see `apctl cert ls`)
        #[arg(long, value_name = "ID")]
        cert: Option<i64>,
        /// Proxy WebSocket upgrades
        #[arg(long)]
        websockets: bool,
    },
    /// Enable a host
    Enable { id: i64 },
    /// Disable a host
    Disable { id: i64 },
    /// Delete a host
    Rm { id: i64 },
}

#[derive(Subcommand)]
pub enum CertCommand {
    /// List certificates and their issuance state
    Ls,
}

// ------------------------------------------------------------------- client

struct Client {
    http: reqwest::Client,
    base: String,
    token: String,
    json_out: bool,
}

impl Client {
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    async fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> anyhow::Result<Value> {
        let mut req = self
            .http
            .request(method, self.url(path))
            .bearer_auth(&self.token)
            // The panel requires this on every mutation. A token is not an
            // ambient credential, but the marker costs nothing and keeps the
            // CLI on exactly the same path as the browser.
            .header("x-ap-request", "1");
        if let Some(b) = body {
            req = req.json(&b);
        }
        let res = req
            .send()
            .await
            .with_context(|| format!("cannot reach the panel at {}", self.base))?;

        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        let parsed: Value = serde_json::from_str(&text).unwrap_or(Value::Null);

        if !status.is_success() {
            // The API's error shape: { error: { code, message } }.
            let msg = parsed
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or_else(|| {
                    if text.is_empty() {
                        status.as_str()
                    } else {
                        &text
                    }
                });
            if status == reqwest::StatusCode::UNAUTHORIZED {
                bail!("{msg}\nThe token was rejected. Check $ANGIE_PANEL_TOKEN, or run as root so the local token file can be read.");
            }
            bail!("{msg}");
        }
        Ok(parsed)
    }

    async fn get(&self, path: &str) -> anyhow::Result<Value> {
        self.send(reqwest::Method::GET, path, None).await
    }
    async fn post(&self, path: &str, body: Value) -> anyhow::Result<Value> {
        self.send(reqwest::Method::POST, path, Some(body)).await
    }
    async fn delete(&self, path: &str) -> anyhow::Result<Value> {
        self.send(reqwest::Method::DELETE, path, None).await
    }

    /// Print raw JSON when `--json` was passed. Returns true when it did, so
    /// callers can skip their human-readable rendering.
    fn dumped(&self, v: &Value) -> bool {
        if self.json_out {
            println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
        }
        self.json_out
    }
}

/// Resolve the panel's base URL. `bind_addr` may be a wildcard, which is not a
/// usable destination — loopback is, and the panel always allows it in the Host
/// allowlist.
fn base_url(cfg: &PanelConfig, override_url: Option<String>) -> String {
    if let Some(u) = override_url.or_else(|| std::env::var("ANGIE_PANEL_URL").ok()) {
        return u.trim_end_matches('/').to_string();
    }
    let host = match cfg.bind_addr.as_str() {
        "0.0.0.0" | "::" | "" => "127.0.0.1",
        h => h,
    };
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]") // bare IPv6 literal
    } else {
        host.to_string()
    };
    format!("http://{host}:{}", cfg.port)
}

fn resolve_token(cfg: &PanelConfig, override_token: Option<String>) -> anyhow::Result<String> {
    if let Some(t) = override_token {
        return Ok(t);
    }
    if let Ok(t) = std::env::var("ANGIE_PANEL_TOKEN") {
        if !t.trim().is_empty() {
            return Ok(t.trim().to_string());
        }
    }
    let path: PathBuf = cfg.data_dir.join(CLI_TOKEN_FILE);
    match std::fs::read_to_string(&path) {
        Ok(t) => Ok(t.trim().to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => bail!(
            "cannot read {} (permission denied).\n\
             Run as root, or pass a token from the panel's Tokens page via --token / $ANGIE_PANEL_TOKEN.",
            path.display()
        ),
        Err(_) => bail!(
            "no API token available.\n\
             Expected {} (written by the service on startup), or pass --token / $ANGIE_PANEL_TOKEN.",
            path.display()
        ),
    }
}

// ------------------------------------------------------------------ helpers

fn yes_no(v: Option<bool>) -> &'static str {
    match v {
        Some(true) => "yes",
        Some(false) => "no",
        None => "?",
    }
}

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("-")
        .to_string()
}

/// Parse `--to http://host:port` into the three fields the API wants.
fn parse_upstream(raw: &str) -> anyhow::Result<(String, String, u16)> {
    let (scheme, rest) = raw
        .split_once("://")
        .ok_or_else(|| anyhow::anyhow!("--to must look like http://host:port (got {raw:?})"))?;
    if scheme != "http" && scheme != "https" {
        bail!("--to scheme must be http or https (got {scheme:?})");
    }
    let rest = rest.trim_end_matches('/');
    let (host, port) = match rest.rsplit_once(':') {
        Some((h, p)) => (
            h.to_string(),
            p.parse::<u16>()
                .with_context(|| format!("bad port in --to: {p:?}"))?,
        ),
        None => (rest.to_string(), if scheme == "https" { 443 } else { 80 }),
    };
    if host.is_empty() {
        bail!("--to has no host: {raw:?}");
    }
    Ok((scheme.to_string(), host, port))
}

// ----------------------------------------------------------------- commands

pub async fn run(
    cmd: CtlCommand,
    cfg: PanelConfig,
    url: Option<String>,
    token: Option<String>,
    json_out: bool,
) -> anyhow::Result<()> {
    let client = Client {
        http: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?,
        base: base_url(&cfg, url),
        token: resolve_token(&cfg, token)?,
        json_out,
    };

    match cmd {
        CtlCommand::Status => status(&client).await,
        CtlCommand::Diff => diff(&client).await,
        CtlCommand::Apply { dry_run } => {
            if dry_run {
                diff(&client).await
            } else {
                apply(&client).await
            }
        }
        CtlCommand::Host(c) => host(&client, c).await,
        CtlCommand::Cert(CertCommand::Ls) => cert_ls(&client).await,
        CtlCommand::Export => export(&client).await,
        CtlCommand::Import { file } => import(&client, &file).await,
    }
}

async fn status(c: &Client) -> anyhow::Result<()> {
    let v = c.get("/api/system/status").await?;
    if c.dumped(&v) {
        return Ok(());
    }
    let angie = &v["angie"];
    let installed = angie["installed"].as_bool().unwrap_or(false);
    println!("panel    {}", s(&v["panel"], "version"));
    println!(
        "angie    {}{}",
        if installed {
            "installed"
        } else {
            "NOT INSTALLED"
        },
        angie["version"]
            .as_str()
            .map(|x| format!(" {x}"))
            .unwrap_or_default()
    );
    println!("  acme module   {}", yes_no(angie["acme_module"].as_bool()));
    println!("  unit active   {}", yes_no(angie["unit_active"].as_bool()));
    println!(
        "dbus     available {}, polkit {}",
        yes_no(v["dbus"]["available"].as_bool()),
        yes_no(v["dbus"]["polkit_ok"].as_bool())
    );

    // A pending diff is the thing an operator most often wants to know about.
    if let Ok(p) = c.get("/api/apply/preview").await {
        let d = &p["diff"];
        let (a, m, r) = (
            d["added"].as_u64().unwrap_or(0),
            d["modified"].as_u64().unwrap_or(0),
            d["removed"].as_u64().unwrap_or(0),
        );
        if a + m + r == 0 {
            println!("config   up to date");
        } else {
            println!("config   PENDING — +{a} ~{m} -{r} (run `apctl apply`)");
        }
        if d["has_drift"].as_bool().unwrap_or(false) {
            println!("         WARNING: a managed file was hand-edited on disk");
        }
    }
    Ok(())
}

async fn diff(c: &Client) -> anyhow::Result<()> {
    let v = c.get("/api/apply/preview").await?;
    if c.dumped(&v) {
        return Ok(());
    }
    let d = &v["diff"];
    let empty = vec![];
    let files = d["files"].as_array().unwrap_or(&empty);
    let mut changed = 0;
    for f in files {
        let status = s(f, "status");
        if status == "unchanged" {
            continue;
        }
        changed += 1;
        let mark = match status.as_str() {
            "added" => "+",
            "removed" => "-",
            _ => "~",
        };
        let drift = if f["drift"].as_bool().unwrap_or(false) {
            "  (hand-edited on disk — this apply would overwrite it)"
        } else {
            ""
        };
        println!("{mark} {}{drift}", s(f, "name"));
        if let Some(u) = f["unified"].as_str() {
            for line in u.lines() {
                println!("    {line}");
            }
        }
    }
    if changed == 0 {
        println!("No changes — the live configuration matches the database.");
    }
    for f in d["foreign"].as_array().unwrap_or(&empty) {
        println!("(unmanaged, left alone) {}", s(f, "name"));
    }
    Ok(())
}

async fn apply(c: &Client) -> anyhow::Result<()> {
    let v = c.post("/api/apply", json!({})).await?;
    if c.dumped(&v) {
        return Ok(());
    }
    let result = s(&v, "result");
    if result == "ok" {
        println!("Applied and reloaded.");
        return Ok(());
    }

    // The apply pipeline already rolled back; surface why it refused.
    eprintln!("Apply failed: {result}");
    for l in v["lint_violations"].as_array().unwrap_or(&vec![]) {
        eprintln!("  lint: {}", s(l, "message"));
    }
    for e in v["file_errors"].as_array().unwrap_or(&vec![]) {
        eprintln!("  {}: {}", s(e, "file"), s(e, "message"));
    }
    for (label, key) in [("angie -t", "stderr"), ("error.log", "error_log_tail")] {
        if let Some(t) = v[key].as_str().filter(|t| !t.trim().is_empty()) {
            eprintln!("--- {label} ---");
            eprintln!("{}", t.trim_end());
        }
    }
    if let Some(rb) = v.get("rollback").filter(|r| !r.is_null()) {
        eprintln!("rollback: {}", s(rb, "result"));
    }
    bail!("configuration was not applied")
}

async fn host(c: &Client, cmd: HostCommand) -> anyhow::Result<()> {
    match cmd {
        HostCommand::Ls => {
            let v = c.get("/api/hosts").await?;
            if c.dumped(&v) {
                return Ok(());
            }
            let empty = vec![];
            let hosts = v["hosts"].as_array().unwrap_or(&empty);
            if hosts.is_empty() {
                println!("No proxy hosts yet.");
                return Ok(());
            }
            println!("{:>4}  {:<40} {:<28} STATE", "ID", "DOMAINS", "UPSTREAM");
            for h in hosts {
                let domains = h["domains"]
                    .as_array()
                    .map(|d| {
                        d.iter()
                            .filter_map(|x| x.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                let upstream = format!(
                    "{}://{}:{}",
                    s(h, "forward_scheme"),
                    s(h, "forward_host"),
                    h["forward_port"].as_u64().unwrap_or(0)
                );
                let mut state = if h["enabled"].as_bool().unwrap_or(true) {
                    "enabled".to_string()
                } else {
                    "disabled".to_string()
                };
                if !h["certificate_id"].is_null() {
                    state.push_str(", ssl");
                }
                println!(
                    "{:>4}  {:<40} {:<28} {}",
                    h["id"].as_i64().unwrap_or(0),
                    domains,
                    upstream,
                    state
                );
            }
        }
        HostCommand::Add {
            domains,
            to,
            cert,
            websockets,
        } => {
            let (scheme, fwd_host, port) = parse_upstream(&to)?;
            let body = json!({
                "domains": domains,
                "forward_scheme": scheme,
                "forward_host": fwd_host,
                "forward_port": port,
                "websockets_upgrade": websockets,
                "certificate_id": cert,
            });
            let v = c.post("/api/hosts", body).await?;
            if c.dumped(&v) {
                return Ok(());
            }
            println!(
                "Created host #{} for {}",
                v["id"].as_i64().unwrap_or(0),
                domains.join(", ")
            );
            println!("Run `apctl apply` to push it to Angie.");
        }
        HostCommand::Enable { id } => {
            let v = c
                .post(&format!("/api/hosts/{id}/enable"), json!({}))
                .await?;
            if !c.dumped(&v) {
                println!("Host #{id} enabled. Run `apctl apply` to push it.");
            }
        }
        HostCommand::Disable { id } => {
            let v = c
                .post(&format!("/api/hosts/{id}/disable"), json!({}))
                .await?;
            if !c.dumped(&v) {
                println!("Host #{id} disabled. Run `apctl apply` to push it.");
            }
        }
        HostCommand::Rm { id } => {
            let v = c.delete(&format!("/api/hosts/{id}")).await?;
            if !c.dumped(&v) {
                println!("Host #{id} deleted. Run `apctl apply` to push it.");
            }
        }
    }
    Ok(())
}

async fn cert_ls(c: &Client) -> anyhow::Result<()> {
    let v = c.get("/api/certificates").await?;
    if c.dumped(&v) {
        return Ok(());
    }
    let empty = vec![];
    let certs = v["certificates"].as_array().unwrap_or(&empty);
    if certs.is_empty() {
        println!("No certificates yet.");
        return Ok(());
    }
    println!("{:>4}  {:<20} {:<40} STATE", "ID", "NAME", "DOMAINS");
    for c in certs {
        let domains = c["domains"]
            .as_array()
            .map(|d| {
                d.iter()
                    .filter_map(|x| x.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        println!(
            "{:>4}  {:<20} {:<40} {}",
            c["id"].as_i64().unwrap_or(0),
            s(c, "name"),
            domains,
            s(c, "state")
        );
    }
    Ok(())
}

async fn export(c: &Client) -> anyhow::Result<()> {
    let v = c.get("/api/export").await?;
    // Always raw JSON: the point is to redirect it into a file or into git.
    println!("{}", serde_json::to_string_pretty(&v)?);
    Ok(())
}

async fn import(c: &Client, file: &str) -> anyhow::Result<()> {
    let raw = if file == "-" {
        std::io::read_to_string(std::io::stdin()).context("reading stdin")?
    } else {
        std::fs::read_to_string(file).with_context(|| format!("reading {file}"))?
    };
    let body: Value = serde_json::from_str(&raw).with_context(|| format!("{file} is not JSON"))?;
    let v = c.post("/api/import", body).await?;
    if c.dumped(&v) {
        return Ok(());
    }
    println!("Imported. Run `apctl apply` to push the new configuration to Angie.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_parsing() {
        assert_eq!(
            parse_upstream("http://127.0.0.1:3000").unwrap(),
            ("http".into(), "127.0.0.1".into(), 3000)
        );
        // Scheme defaults, trailing slash, and IPv6-free hostnames.
        assert_eq!(
            parse_upstream("https://backend.internal").unwrap(),
            ("https".into(), "backend.internal".into(), 443)
        );
        assert_eq!(
            parse_upstream("http://app/").unwrap(),
            ("http".into(), "app".into(), 80)
        );
        for bad in ["127.0.0.1:3000", "ftp://x:1", "http://:80", "http://x:abc"] {
            assert!(parse_upstream(bad).is_err(), "{bad} must be rejected");
        }
    }

    #[test]
    fn base_url_falls_back_to_loopback_on_wildcard_binds() {
        let wildcard: PanelConfig = toml::from_str("bind_addr = \"0.0.0.0\"\nport = 8080").unwrap();
        assert_eq!(base_url(&wildcard, None), "http://127.0.0.1:8080");

        let specific: PanelConfig =
            toml::from_str("bind_addr = \"192.168.1.5\"\nport = 9000").unwrap();
        assert_eq!(base_url(&specific, None), "http://192.168.1.5:9000");

        // An explicit override wins and loses any trailing slash.
        assert_eq!(
            base_url(&specific, Some("http://panel.lan:1234/".into())),
            "http://panel.lan:1234"
        );
    }
}
