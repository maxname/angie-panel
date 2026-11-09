//! Background reconciler (M3): auto-activates HTTPS once a certificate is
//! issued. The panel's first-issuance state machine (PLAN.md §4/§5) renders a
//! host HTTP-only until its certificate is `ready`; without this task the user
//! would have to click Apply again after issuance. Here we do it for them —
//! but ONLY when it is safe.
//!
//! Safety: we auto-apply only when the DB revision is unchanged since the last
//! successful apply (so the panel never pushes unreviewed *user* edits) and no
//! managed file has drifted on disk. Under those conditions any difference
//! between the generated config and the live config can only come from
//! external state — a certificate flipping to `valid` in Angie's /status — so
//! applying it is exactly "activate the HTTPS the user already asked for".

use std::sync::Arc;
use std::time::Duration;

use crate::apply_api::{perform_apply, ApplyTrigger};
use crate::state::AppState;
use crate::{apply, repo, settings};

const INTERVAL: Duration = Duration::from_secs(30);

pub fn spawn(state: Arc<AppState>) {
    tokio::spawn(async move {
        // Small initial delay so startup finishes first.
        tokio::time::sleep(Duration::from_secs(10)).await;
        loop {
            if let Err(e) = tick(&state).await {
                tracing::debug!(error = %e, "reconcile tick failed");
            }
            tokio::time::sleep(INTERVAL).await;
        }
    });
}

async fn tick(state: &Arc<AppState>) -> anyhow::Result<()> {
    // Never applied yet → the user must do the first apply; do nothing.
    let last_applied: Option<i64> = settings_i64(state, settings::KEY_LAST_APPLIED_REVISION).await;
    let Some(last_applied) = last_applied else {
        return Ok(());
    };

    // Pending user edits → don't auto-apply; the dashboard surfaces them.
    let current = repo::hosts_revision(&state.db).await?;
    if current != last_applied {
        return Ok(());
    }

    // Compute the diff between the freshly generated config (which folds in the
    // live ACME readiness from /status) and what's on disk.
    let fileset = match settings::build_fileset(state).await {
        Ok(fs) => fs,
        Err(_) => return Ok(()), // status API down / lint issue — try later
    };
    let diff = match apply::preview(&state.cfg, &fileset) {
        Ok(d) => d,
        Err(_) => return Ok(()),
    };

    // Only act on a real, non-drift change. Drift (hand-edited managed files)
    // is surfaced as an alert, never silently overwritten by the reconciler.
    if !diff.has_changes() || diff.has_drift {
        return Ok(());
    }

    tracing::info!(
        "reconciler: config changed with no pending user edits (a certificate \
         likely became ready) — auto-applying to activate HTTPS"
    );
    match perform_apply(
        state,
        ApplyTrigger::AutoCertReady {
            expected_revision: current,
        },
    )
    .await
    {
        Ok(report) if report.result.is_ok() => {
            tracing::info!("reconciler: auto-apply succeeded ({})", report.summary);
        }
        Ok(report) => {
            tracing::warn!("reconciler: auto-apply did not succeed: {}", report.summary);
        }
        Err(e) => {
            // e.g. another apply is in progress (409) — harmless, retry next tick.
            tracing::debug!(error = %e.message, "reconciler: auto-apply skipped");
        }
    }
    Ok(())
}

async fn settings_i64(state: &AppState, key: &str) -> Option<i64> {
    repo::all_settings(&state.db)
        .await
        .ok()
        .and_then(|m| m.get(key).and_then(|v| v.parse().ok()))
}
