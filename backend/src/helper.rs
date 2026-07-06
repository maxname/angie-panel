//! The privileged helper. Invoked ONLY via the root oneshot units
//! (angie-panel-configtest.service / angie-panel-apply.service) with a fixed
//! argv, or directly in dev environments. It never takes user input from the
//! command line beyond the config path.

use std::path::Path;

use anyhow::Context;
use tokio::process::Command;

use crate::config::PanelConfig;
use crate::db::now_epoch;
use crate::system::{ConfigtestReport, REPORT_FILE};

/// Run `angie -t -e stderr` against the live configuration and write the
/// report for the panel to pick up. Always exits 0 when the report was
/// written — an invalid Angie config is a *result*, not a helper failure.
pub async fn configtest(cfg: &PanelConfig) -> anyhow::Result<()> {
    let ran_via = std::env::var("ANGIE_PANEL_RAN_VIA").unwrap_or_else(|_| "systemd".into());
    let out = Command::new(&cfg.angie.bin)
        .args(["-t", "-e", "stderr"])
        .output()
        .await;

    let report = match out {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stderr).into_owned();
            let stdout = String::from_utf8_lossy(&out.stdout);
            if !stdout.trim().is_empty() {
                text.push_str(&stdout);
            }
            ConfigtestReport {
                timestamp: now_epoch(),
                ok: out.status.success(),
                exit_code: out.status.code(),
                output: text,
                ran_via,
            }
        }
        Err(e) => ConfigtestReport {
            timestamp: now_epoch(),
            ok: false,
            exit_code: None,
            output: format!("failed to execute {}: {e}", cfg.angie.bin.display()),
            ran_via,
        },
    };

    write_report(&cfg.data_dir, &report)?;
    if report.ok {
        println!("angie -t: OK");
    } else {
        println!("angie -t: FAILED (see report)");
    }
    Ok(())
}

/// Atomic same-directory write (see PLAN.md §2.2 — no cross-directory
/// renames, they break under ProtectSystem=strict and on power loss).
fn write_report(data_dir: &Path, report: &ConfigtestReport) -> anyhow::Result<()> {
    let tmp = data_dir.join(".configtest-report.tmp");
    let dst = data_dir.join(REPORT_FILE);
    let json = serde_json::to_string_pretty(report)?;
    std::fs::write(&tmp, &json).with_context(|| format!("writing {}", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Root writes into the panel-owned data dir; the panel must be able
        // to read the report back.
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644))?;
    }
    std::fs::rename(&tmp, &dst).with_context(|| format!("renaming to {}", dst.display()))?;
    Ok(())
}

/// M1 will implement the full apply pipeline (lint → validate staging →
/// snapshot → sync → reload → verify). Shipping the unit now keeps the
/// packaging stable.
pub async fn apply(_cfg: &PanelConfig) -> anyhow::Result<()> {
    anyhow::bail!("helper apply is not implemented yet (planned for M1)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn configtest_reports_missing_binary_gracefully() {
        let dir = tempfile::tempdir().unwrap();
        let cfg: PanelConfig = toml::from_str(&format!(
            "data_dir = \"{}\"\n[angie]\nbin = \"/nonexistent/angie\"",
            dir.path().display()
        ))
        .unwrap();
        configtest(&cfg).await.unwrap();
        let report = crate::system::read_report(dir.path()).unwrap();
        assert!(!report.ok);
        assert!(report.exit_code.is_none());
        assert!(report.output.contains("/nonexistent/angie"));
    }
}
