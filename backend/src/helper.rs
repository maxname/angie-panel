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

/// The full apply pipeline (lint → validate staging → snapshot → sync →
/// reload → verify, with rollback on failure). Reads the staged set the panel
/// wrote under `<data_dir>/staging` and writes an ApplyReport to
/// `<data_dir>/apply-result.json`. Like `configtest`, an invalid/rejected
/// config is a *result* written to the report, not a helper failure — the
/// helper only returns `Err` when it cannot write the report at all.
pub async fn apply(cfg: &PanelConfig) -> anyhow::Result<()> {
    let report = crate::apply::helper_apply(cfg).await?;
    if report.result.is_ok() {
        println!("apply: OK ({})", report.summary);
    } else {
        println!("apply: {:?} ({})", report.result, report.summary);
    }
    Ok(())
}

/// One-time activation of Angie's `stream {}` context. Idempotently edits the
/// live `angie.conf` so it loads `<stream_d>/*.conf`, validates the result
/// with `angie -t`, and reloads. On any failure the original `angie.conf` is
/// restored, so a bad edit is reported, never left live. Like the other
/// helpers, the outcome round-trips to the panel via a result file.
pub async fn enable_streams(cfg: &PanelConfig) -> anyhow::Result<()> {
    use crate::apply::pipeline::StreamEnable;
    use crate::streams::{EnableReport, ENABLE_REPORT_FILE};

    let angie_conf = &cfg.angie.angie_conf;
    let (report, log) = match run_enable_streams(cfg).await {
        Ok(StreamEnable::AlreadyActive) => (
            EnableReport {
                timestamp: now_epoch(),
                ok: true,
                message: "the stream context was already enabled".into(),
            },
            "enable-streams: already active".to_string(),
        ),
        Ok(how) => {
            let verb = match how {
                StreamEnable::Injected => "activated the existing stream block",
                _ => "added a stream block",
            };
            (
                EnableReport {
                    timestamp: now_epoch(),
                    ok: true,
                    message: format!(
                        "stream context enabled ({verb} in {})",
                        angie_conf.display()
                    ),
                },
                format!("enable-streams: OK ({verb})"),
            )
        }
        Err(e) => (
            EnableReport {
                timestamp: now_epoch(),
                ok: false,
                message: e.to_string(),
            },
            format!("enable-streams: FAILED ({e})"),
        ),
    };

    write_enable_report(&cfg.data_dir, ENABLE_REPORT_FILE, &report)?;
    println!("{log}");
    Ok(())
}

/// The privileged edit+validate+reload transaction. Returns the edit made, or
/// an error after restoring the original `angie.conf`.
async fn run_enable_streams(
    cfg: &PanelConfig,
) -> anyhow::Result<crate::apply::pipeline::StreamEnable> {
    use crate::apply::atomic;
    use crate::apply::pipeline::{enable_stream_context_text, StreamEnable};

    let angie_conf = &cfg.angie.angie_conf;
    let original = std::fs::read_to_string(angie_conf)
        .with_context(|| format!("reading {}", angie_conf.display()))?;
    let (new_text, how) = enable_stream_context_text(&original, &cfg.angie.stream_d_dir);
    if how == StreamEnable::AlreadyActive {
        return Ok(how);
    }

    let dir = angie_conf
        .parent()
        .ok_or_else(|| anyhow::anyhow!("angie.conf has no parent directory"))?;
    let name = angie_conf
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("angie.conf has no file name"))?;

    atomic::write_in_dir(dir, name, new_text.as_bytes(), 0o644)
        .with_context(|| format!("writing {}", angie_conf.display()))?;

    // Validate the edited live config. On failure, put the original back.
    let out = Command::new(&cfg.angie.bin)
        .args(["-t", "-e", "stderr"])
        .output()
        .await;
    let (ok, detail) = match out {
        Ok(o) => (
            o.status.success(),
            String::from_utf8_lossy(&o.stderr).trim().to_string(),
        ),
        Err(e) => (
            false,
            format!("failed to execute {}: {e}", cfg.angie.bin.display()),
        ),
    };
    if !ok {
        atomic::write_in_dir(dir, name, original.as_bytes(), 0o644)
            .with_context(|| format!("restoring {}", angie_conf.display()))?;
        anyhow::bail!("angie -t rejected the edited config, reverted: {detail}");
    }

    // Reload so the newly-active context takes effect immediately.
    let reload = Command::new("systemctl")
        .args(["reload", "angie.service"])
        .output()
        .await;
    if let Ok(o) = &reload {
        if !o.status.success() {
            // Config is valid and staged; reload will happen on next apply.
            tracing::warn!(
                stderr = %String::from_utf8_lossy(&o.stderr),
                "enable-streams: reload failed (config is valid; will take effect on next reload)"
            );
        }
    }
    Ok(how)
}

fn write_enable_report(
    data_dir: &Path,
    file: &str,
    report: &crate::streams::EnableReport,
) -> anyhow::Result<()> {
    let tmp = data_dir.join(".enable-streams-result.tmp");
    let dst = data_dir.join(file);
    let json = serde_json::to_string_pretty(report)?;
    std::fs::write(&tmp, &json).with_context(|| format!("writing {}", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644))?;
    }
    std::fs::rename(&tmp, &dst).with_context(|| format!("renaming to {}", dst.display()))?;
    Ok(())
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
