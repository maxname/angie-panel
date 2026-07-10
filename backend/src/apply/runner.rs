//! Production [`Runner`](super::pipeline::Runner): drives the real `angie`
//! binary, `systemctl reload angie`, the status API, and the Angie error log.
//! All external commands use argv arrays (never a shell, never user input on
//! the command line). Tests substitute a fake runner instead.

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use tokio::process::Command;

use super::pipeline::Runner;
use crate::config::PanelConfig;

/// Reload command. On a real box the helper is already root, so
/// `systemctl reload angie` is simplest (PLAN.md §2.2 step 7 / §2.1 — the
/// helper reloads Angie itself; the panel needs no polkit right for it).
const SYSTEMCTL: &str = "/usr/bin/systemctl";
const RELOAD_UNIT: &str = "angie.service";
const ERROR_LOG: &str = "/var/log/angie/error.log";
const ERROR_LOG_TAIL_BYTES: u64 = 8 * 1024;

pub struct RealRunner {
    angie_bin: PathBuf,
    status_api_url: String,
    error_log: PathBuf,
    http: reqwest::Client,
}

impl RealRunner {
    pub fn new(cfg: &PanelConfig) -> Self {
        Self {
            angie_bin: cfg.angie.bin.clone(),
            status_api_url: cfg.angie.status_api_url.clone(),
            error_log: PathBuf::from(ERROR_LOG),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(3))
                .build()
                .expect("http client"),
        }
    }
}

#[async_trait]
impl Runner for RealRunner {
    async fn angie_test(&self, conf: Option<&Path>) -> (bool, String) {
        let mut cmd = Command::new(&self.angie_bin);
        cmd.arg("-t");
        if let Some(conf) = conf {
            cmd.arg("-c").arg(conf);
        }
        cmd.args(["-e", "stderr"]);
        match cmd.output().await {
            Ok(out) => {
                let mut text = String::from_utf8_lossy(&out.stderr).into_owned();
                let stdout = String::from_utf8_lossy(&out.stdout);
                if !stdout.trim().is_empty() {
                    text.push_str(&stdout);
                }
                (out.status.success(), text)
            }
            Err(e) => (
                false,
                format!("failed to execute {}: {e}", self.angie_bin.display()),
            ),
        }
    }

    async fn reload(&self) -> (bool, String) {
        match Command::new(SYSTEMCTL)
            .args(["reload", RELOAD_UNIT])
            .output()
            .await
        {
            Ok(out) if out.status.success() => (true, "reloaded".into()),
            Ok(out) => (
                false,
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            ),
            Err(e) => (false, format!("failed to execute {SYSTEMCTL}: {e}")),
        }
    }

    async fn status_generation(&self) -> Option<u64> {
        let url = format!("{}/angie", self.status_api_url.trim_end_matches('/'));
        let resp = self.http.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let v: serde_json::Value = resp.json().await.ok()?;
        v.get("generation").and_then(|g| g.as_u64())
    }

    async fn error_log_tail(&self) -> String {
        tail_file(&self.error_log, ERROR_LOG_TAIL_BYTES)
    }
}

/// Read the last `max_bytes` of a file (best-effort; empty on any error).
fn tail_file(path: &Path, max_bytes: u64) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(path) else {
        return String::new();
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(max_bytes);
    if f.seek(SeekFrom::Start(start)).is_err() {
        return String::new();
    }
    let mut buf = String::new();
    let _ = f.read_to_string(&mut buf);
    buf
}
