//! `apctl` — the operator CLI, a thin client over the panel's own REST API.
//!
//! It deliberately does **not** touch the database. Everything the panel does
//! on a write — validation, the staged diff, `angie -t`, the atomic swap with
//! rollback, the audit row — lives behind the API, and a CLI that went around
//! it would be a second, weaker implementation of the apply pipeline.
//!
//! This is its own crate, depending on nothing of the server's. That is what
//! lets it build for macOS and Windows — the panel is Linux-only by nature
//! (systemd, polkit, D-Bus) — and ship as a few MB rather than ~15 with an
//! embedded frontend and SQLite that a CLI user has no use for.
//!
//! Reached two ways: the `apctl` binary, or `angie-panel ctl <cmd>` on the
//! server, which links this crate and calls the same [`run`].
//!
//! Credentials, in order: `--token`, `$ANGIE_PANEL_TOKEN`, then the local token
//! file in the data dir — which is what makes it zero-config on the box itself.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use clap::Subcommand;
use serde_json::{json, Value};

/// File in the panel's data dir holding the machine-local token. Must match
/// `auth::CLI_TOKEN_FILE` in the panel; the panel has a test that pins it.
pub const CLI_TOKEN_FILE: &str = "cli-token";

/// Where to reach the panel and how to authenticate. The panel builds this from
/// its own `PanelConfig`; the standalone binary builds it from the config file
/// or from flags alone. Keeping it a plain struct is what stops this crate from
/// having to know the server's configuration schema.
pub struct Endpoint {
    /// `--url`, else `$ANGIE_PANEL_URL`, else derived from bind_addr/port.
    pub url: Option<String>,
    /// `--token`, else `$ANGIE_PANEL_TOKEN`, else the local token file.
    pub token: Option<String>,
    pub bind_addr: String,
    pub port: u16,
    pub data_dir: PathBuf,
}

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
    /// Print a shell completion script (eval it, or drop it in the shell's
    /// completion directory)
    Completions {
        /// bash, zsh, fish, elvish or powershell
        shell: clap_complete::Shell,
    },
    /// Write this command's man page into DIR. Packaging-time only.
    #[command(hide = true)]
    Man { dir: PathBuf },
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
fn base_url(ep: &Endpoint) -> String {
    if let Some(u) = ep
        .url
        .clone()
        .or_else(|| std::env::var("ANGIE_PANEL_URL").ok())
    {
        return u.trim_end_matches('/').to_string();
    }
    let host = match ep.bind_addr.as_str() {
        "0.0.0.0" | "::" | "" => "127.0.0.1",
        h => h,
    };
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]") // bare IPv6 literal
    } else {
        host.to_string()
    };
    format!("http://{host}:{}", ep.port)
}

fn resolve_token(ep: &Endpoint) -> anyhow::Result<String> {
    if let Some(t) = ep.token.clone() {
        return Ok(t);
    }
    if let Ok(t) = std::env::var("ANGIE_PANEL_TOKEN") {
        if !t.trim().is_empty() {
            return Ok(t.trim().to_string());
        }
    }
    let path: PathBuf = ep.data_dir.join(CLI_TOKEN_FILE);
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

/// Write a completion script for `cmd` to stdout. Dispatched from each binary's
/// `main`, which is where its clap `Command` is defined — and it must run
/// before any token lookup, since it talks to nothing.
pub fn print_completions(shell: clap_complete::Shell, cmd: &mut clap::Command) {
    let name = cmd.get_name().to_string();
    clap_complete::generate(shell, cmd, name, &mut std::io::stdout());
}

/// Render `cmd`'s man page into `dir` as `<name>.1`.
///
/// Generated rather than committed so the page cannot drift from the clap
/// definitions it documents. Lives in this crate so the panel can render both
/// its own page and the CLI's from one place.
pub fn write_man_page(dir: &Path, cmd: clap::Command) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = dir.join(format!("{}.1", cmd.get_name()));
    let mut buf = Vec::new();
    clap_mangen::Man::new(cmd)
        .render(&mut buf)
        .with_context(|| format!("rendering {}", path.display()))?;
    std::fs::write(&path, buf).with_context(|| format!("writing {}", path.display()))?;
    println!("{}", path.display());
    Ok(())
}

/// The slice of the panel's `angie-panel.toml` the CLI cares about.
///
/// Deliberately not the panel's `PanelConfig`: serde ignores unknown keys, so
/// this reads the same file without this crate having to track the server's
/// configuration schema (or depend on the server at all).
#[derive(Debug, serde::Deserialize)]
pub struct CliConfig {
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
}

// Kept in step with the panel's config.rs defaults; `cli_config_defaults_match`
// in the panel's test suite fails if they drift.
fn default_bind_addr() -> String {
    "127.0.0.1".into()
}
fn default_port() -> u16 {
    8080
}
fn default_data_dir() -> PathBuf {
    PathBuf::from("/var/lib/angie-panel")
}

impl CliConfig {
    /// Load the panel's config, falling back to defaults when there is none —
    /// with `--url` and `--token` the CLI needs no config file at all.
    pub fn load(explicit: Option<PathBuf>) -> Self {
        let path = explicit
            .or_else(|| std::env::var("ANGIE_PANEL_CONFIG").ok().map(PathBuf::from))
            .or_else(|| {
                ["/etc/angie-panel.toml", "./angie-panel.toml"]
                    .iter()
                    .map(PathBuf::from)
                    .find(|p| p.exists())
            });
        path.and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|raw| toml::from_str(&raw).ok())
            .unwrap_or_else(|| toml::from_str("").expect("CliConfig defaults"))
    }
}

// ----------------------------------------------------------------- commands

pub async fn run(cmd: CtlCommand, ep: Endpoint, json_out: bool) -> anyhow::Result<()> {
    let client = Client {
        http: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?,
        base: base_url(&ep),
        token: resolve_token(&ep)?,
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
        CtlCommand::Completions { .. } | CtlCommand::Man { .. } => {
            unreachable!("handled in main")
        }
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

    fn endpoint(bind: &str, port: u16, url: Option<&str>) -> Endpoint {
        Endpoint {
            url: url.map(str::to_string),
            token: Some("ap_test".into()),
            bind_addr: bind.into(),
            port,
            data_dir: PathBuf::from("/var/lib/angie-panel"),
        }
    }

    #[test]
    fn base_url_falls_back_to_loopback_on_wildcard_binds() {
        assert_eq!(
            base_url(&endpoint("0.0.0.0", 8080, None)),
            "http://127.0.0.1:8080"
        );
        assert_eq!(
            base_url(&endpoint("192.168.1.5", 9000, None)),
            "http://192.168.1.5:9000"
        );
        // A bare IPv6 literal has to be bracketed to be a valid authority.
        assert_eq!(base_url(&endpoint("::1", 8080, None)), "http://[::1]:8080");
        // An explicit override wins and loses any trailing slash.
        assert_eq!(
            base_url(&endpoint(
                "192.168.1.5",
                9000,
                Some("http://panel.lan:1234/")
            )),
            "http://panel.lan:1234"
        );
    }

    /// The config file is the panel's, so only the fields the CLI actually
    /// reads may be required — anything else in it must be ignored, not error.
    #[test]
    fn cli_config_ignores_the_rest_of_the_panels_file() {
        let cfg: CliConfig = toml::from_str(
            "bind_addr = \"10.0.0.5\"\nport = 9999\ndata_dir = \"/srv/ap\"\n\
             allow_advanced_snippets = true\n[angie]\nbin = \"/usr/sbin/angie\"",
        )
        .expect("unknown panel keys must not break the CLI");
        assert_eq!(cfg.bind_addr, "10.0.0.5");
        assert_eq!(cfg.port, 9999);
        assert_eq!(cfg.data_dir, PathBuf::from("/srv/ap"));

        // And an absent file is equivalent to defaults.
        let empty: CliConfig = toml::from_str("").unwrap();
        assert_eq!(empty.bind_addr, "127.0.0.1");
        assert_eq!(empty.port, 8080);
    }
}
