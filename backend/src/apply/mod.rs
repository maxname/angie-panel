//! Apply pipeline (PLAN.md §2.2 — the anti-nginx-proxy-manager safe-apply
//! flow): staging, diff preview, manifest snapshots, atomic sync, helper-side
//! apply with rollback and crash recovery.
//!
//! # Trust boundary
//! The **panel** (unprivileged) only writes under `data_dir`: it stages a
//! header-wrapped, already-linted FileSet ([`write_staging`]) and computes the
//! [`preview`] diff. The **helper** (root, oneshot unit) runs the full
//! transaction ([`helper_apply`]) that touches `/etc/angie/http.d`:
//! re-lint → validate-staged → snapshot → atomic sync → post-swap validate →
//! reload → verify, with rollback on any failure.
//!
//! # Module layout
//! - [`atomic`]   — the one crash-safe write/remove helper (temp+fsync+rename+dir fsync).
//! - [`header`]   — parser for the generator's MANAGED-BY header (read-only here).
//! - [`manifest`] — snapshot [`Manifest`] + rollback (`restore_into`).
//! - [`diff`]     — [`DiffReport`] for the "Apply preview" UI, incl. drift detection.
//! - [`stage`]    — panel-side staging + `angie-test.conf` (validate-before-swap).
//! - [`report`]   — the [`ApplyReport`] JSON the helper writes for the panel.
//! - [`runner`]   — production [`pipeline::Runner`] over `angie`/`systemctl`/status.
//! - [`pipeline`] — the helper transaction + [`recover_if_interrupted`].
//!
//! The public re-exports and the panel-side `preview`/`write_staging`/`recover`
//! entry points below are the API surface the future API layer (api.rs) and the
//! panel bootstrap will consume; until they are wired they read as dead code, so
//! this module tolerates that the same way config.rs marks apply-pipeline fields
//! it feeds ("consumed by the apply pipeline from M1").
#![allow(dead_code, unused_imports)]

pub mod atomic;
pub mod diff;
pub mod header;
pub mod manifest;
pub mod pipeline;
pub mod report;
pub mod runner;
pub mod stage;

#[cfg(test)]
pub mod testutil;

use std::path::Path;

pub use diff::{DiffReport, FileStatus};
pub use manifest::Manifest;
pub use pipeline::{
    apply_in_progress, read_report, recover_if_interrupted, ApplyCtx, Linter, RecoveryOutcome,
    Runner,
};
pub use report::{ApplyReport, ApplyResult};
pub use runner::RealRunner;
pub use stage::{stage as write_staging, StageResult};

use crate::config::PanelConfig;
use crate::generator::lint::{check_fileset, LintPolicy};
use crate::generator::FileSet;

// ----------------------------------------------------------- panel-side API
// (unprivileged; writes only under data_dir)

/// Preview what applying `staged` would do to the live dirs: per-file
/// Added/Modified/Removed/Unchanged, unified diffs, foreign-file report, and
/// drift flags. Splits `staged` into http.d and stream.d and diffs both. Pure
/// read of the live directories — safe for the panel user.
pub fn preview(cfg: &PanelConfig, staged: &FileSet) -> anyhow::Result<DiffReport> {
    let (http, stream) = stage::split_fileset(staged);
    let mut report = diff::diff(&cfg.angie.http_d_dir, &http)?;
    if !stream.is_empty() {
        if let Ok(sd) = diff::diff(&cfg.angie.stream_d_dir, &stream) {
            pipeline::merge_stream_diff(&mut report, sd);
        }
    }
    Ok(report)
}

/// Build the [`LintPolicy`] from panel config (shared by the panel pre-check and
/// the helper's defense-in-depth re-lint).
pub fn lint_policy(cfg: &PanelConfig) -> LintPolicy {
    LintPolicy {
        snippets_dir: cfg.angie.snippets_dir.clone(),
        public_dir: cfg.public_dir(),
        allow_advanced_snippets: cfg.allow_advanced_snippets,
    }
}

/// Production linter closure wiring [`crate::generator::lint::check_fileset`].
/// Kept behind a [`Linter`] indirection so tests inject a stub while the
/// generator's real implementation lands.
pub fn real_linter() -> Linter {
    Box::new(check_fileset)
}

// ----------------------------------------------------------- helper-side API
// (root; called from helper::apply)

/// Run the full helper transaction against the config's live `http_d_dir`,
/// using the real `angie`/`systemctl`/status runner. Writes
/// `<data_dir>/apply-result.json` and returns the report.
pub async fn helper_apply(cfg: &PanelConfig) -> anyhow::Result<ApplyReport> {
    let runner = RealRunner::new(cfg);
    let lint = real_linter();
    let ctx = ApplyCtx {
        cfg,
        runner: &runner,
        lint: &lint,
        lint_policy: lint_policy(cfg),
    };
    pipeline::run_apply(&ctx).await
}

/// Panel-startup crash recovery entry point with the production runner
/// (PLAN.md §2.2 step 2). The panel calls this once on boot.
pub async fn recover(cfg: &PanelConfig) -> anyhow::Result<RecoveryOutcome> {
    let runner = RealRunner::new(cfg);
    recover_if_interrupted(cfg, &runner).await
}

/// Convenience: the staging directory root for a given data dir.
pub fn staging_root(data_dir: &Path) -> std::path::PathBuf {
    stage::StagingPaths::new(data_dir).root
}
