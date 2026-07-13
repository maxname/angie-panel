use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PanelConfig {
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    /// Extra hostnames accepted in the Host header (the bind address and
    /// localhost are always accepted). Empty + unspecified bind (0.0.0.0)
    /// disables the check with a warning.
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    /// Root-owned opt-in for raw config snippets (see PLAN.md §7).
    #[serde(default)]
    pub allow_advanced_snippets: bool,
    /// Permit proxying to loopback/link-local upstreams (SSRF guard opt-out).
    #[serde(default)]
    pub allow_loopback_upstreams: bool,
    #[serde(default)]
    pub angie: AngieConfig,
}

impl PanelConfig {
    /// Directory served for the custom-HTML default site (world-readable,
    /// unlike the rest of data_dir).
    pub fn public_dir(&self) -> std::path::PathBuf {
        self.data_dir.join("public")
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AngieConfig {
    #[serde(default = "default_angie_bin")]
    pub bin: PathBuf,
    /// Managed config directory; consumed by the apply pipeline from M1.
    #[serde(default = "default_http_d_dir")]
    #[allow(dead_code)]
    pub http_d_dir: PathBuf,
    /// Managed stream (TCP/UDP) config directory. Loaded by Angie only when the
    /// `stream {}` block is active in angie.conf (see the enable-streams flow).
    #[serde(default = "default_stream_d_dir")]
    pub stream_d_dir: PathBuf,
    /// Base config used to build the staging validation conf from M1.
    #[serde(default = "default_angie_conf")]
    #[allow(dead_code)]
    pub angie_conf: PathBuf,
    #[serde(default = "default_status_api_url")]
    pub status_api_url: String,
    /// Read-only shared snippet files shipped by the package.
    #[serde(default = "default_snippets_dir")]
    pub snippets_dir: PathBuf,
    /// Directory for the ACME collector unix sockets (created by the root
    /// helper before reload). A runtime tmpfs path is ideal.
    #[serde(default = "default_acme_socket_dir")]
    pub acme_socket_dir: PathBuf,
    /// Country → CIDR dataset (CSV `country_code,cidr`) backing geo blocking.
    /// Shipped by the package; (re)built by scripts/build-geoip-data.sh.
    #[serde(default = "default_geoip_data")]
    pub geoip_data: PathBuf,
    /// Directory holding the vendored acme.sh core (`acme.sh`) + its `dnsapi/`
    /// plugins, used by the DNS-01 provider hook. Shipped by the package.
    #[serde(default = "default_acme_sh_dir")]
    pub acme_sh_dir: PathBuf,
}

impl Default for AngieConfig {
    fn default() -> Self {
        Self {
            bin: default_angie_bin(),
            http_d_dir: default_http_d_dir(),
            stream_d_dir: default_stream_d_dir(),
            angie_conf: default_angie_conf(),
            status_api_url: default_status_api_url(),
            snippets_dir: default_snippets_dir(),
            acme_socket_dir: default_acme_socket_dir(),
            geoip_data: default_geoip_data(),
            acme_sh_dir: default_acme_sh_dir(),
        }
    }
}

fn default_acme_sh_dir() -> PathBuf {
    "/usr/share/angie-panel/acme.sh".into()
}

fn default_snippets_dir() -> PathBuf {
    "/usr/share/angie-panel/snippets".into()
}
fn default_geoip_data() -> PathBuf {
    "/usr/share/angie-panel/geoip-countries.csv".into()
}
fn default_acme_socket_dir() -> PathBuf {
    "/run/angie-panel".into()
}

fn default_bind_addr() -> String {
    "127.0.0.1".into()
}
fn default_port() -> u16 {
    8080
}
fn default_data_dir() -> PathBuf {
    "/var/lib/angie-panel".into()
}
fn default_angie_bin() -> PathBuf {
    "/usr/sbin/angie".into()
}
fn default_http_d_dir() -> PathBuf {
    "/etc/angie/http.d".into()
}
fn default_stream_d_dir() -> PathBuf {
    "/etc/angie/stream.d".into()
}
fn default_angie_conf() -> PathBuf {
    "/etc/angie/angie.conf".into()
}
fn default_status_api_url() -> String {
    "http://127.0.0.1:8100/status".into()
}

/// Resolve the config path: explicit flag, then $ANGIE_PANEL_CONFIG,
/// then /etc/angie-panel.toml, then ./angie-panel.toml.
pub fn resolve_path(explicit: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    if let Ok(p) = std::env::var("ANGIE_PANEL_CONFIG") {
        return Ok(p.into());
    }
    for candidate in ["/etc/angie-panel.toml", "./angie-panel.toml"] {
        let p = Path::new(candidate);
        if p.exists() {
            return Ok(p.to_path_buf());
        }
    }
    anyhow::bail!(
        "no config file found: pass --config, set ANGIE_PANEL_CONFIG, \
         or create /etc/angie-panel.toml"
    )
}

pub fn load(path: &Path) -> anyhow::Result<PanelConfig> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading config {}", path.display()))?;
    let cfg: PanelConfig =
        toml::from_str(&raw).with_context(|| format!("parsing config {}", path.display()))?;
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_config() {
        let cfg: PanelConfig = toml::from_str(
            r#"
            bind_addr = "192.168.1.2"
            port = 8081
            data_dir = "/tmp/ap"
            [angie]
            bin = "/opt/angie/sbin/angie"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.bind_addr, "192.168.1.2");
        assert_eq!(cfg.port, 8081);
        assert_eq!(cfg.angie.bin, PathBuf::from("/opt/angie/sbin/angie"));
        // Unspecified nested fields fall back to defaults.
        assert_eq!(cfg.angie.status_api_url, "http://127.0.0.1:8100/status");
        assert!(!cfg.allow_advanced_snippets);
    }

    #[test]
    fn parses_empty_config_with_defaults() {
        let cfg: PanelConfig = toml::from_str("").unwrap();
        assert_eq!(cfg.bind_addr, "127.0.0.1");
        assert_eq!(cfg.port, 8080);
        assert_eq!(cfg.data_dir, PathBuf::from("/var/lib/angie-panel"));
    }
}
