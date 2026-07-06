//! Helper-side apply transaction (root, oneshot) — PLAN.md §2.2 steps 3-9.
//!
//! Runs ONLY from the privileged helper (`helper apply`). Reads the staged set
//! from `<data_dir>/staging`, then, in order:
//!   a. re-lint the staged fileset (defense in depth — never trust the panel);
//!   b. `angie -t -c <staging test conf>` — validate BEFORE touching /etc;
//!   c. snapshot the live http.d managed files into a rotated backup manifest;
//!   d. atomic same-dir sync into http_d_dir (no cross-dir renames — EXDEV);
//!   e. post-swap `angie -t`;
//!   f. `systemctl reload angie`, then poll the status API for generation++;
//!   g. on ANY failure after (d): rollback from the snapshot, reload, report.
//! The outcome is written to `<data_dir>/apply-result.json` (mode 0644).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;

use super::atomic;
use super::diff::{diff, DiffReport};
use super::manifest::{scan_dir, Manifest, CONF_MODE};
use super::report::{ApplyReport, ApplyResult, FileError, RollbackOutcome, APPLY_RESULT_FILE};
use super::stage::StagingPaths;
use crate::config::PanelConfig;
use crate::db::now_epoch;
use crate::generator::lint::{LintPolicy, LintViolation};
use crate::generator::FileSet;

/// Keep this many backup snapshots (PLAN.md §2.2 step 5).
const BACKUP_KEEP: usize = 10;
/// Marker file signalling an apply is mid-flight (crash recovery, step 2).
const IN_PROGRESS_MARKER: &str = ".apply-in-progress";
/// How long to wait for the status-API generation to increment after reload.
const RELOAD_VERIFY_TIMEOUT: Duration = Duration::from_secs(10);

/// Abstraction over the two external commands the transaction shells out to, so
/// tests can inject a fake `angie` script and skip the real `systemctl`. All
/// invocations use argv arrays — never a shell, never user input on the line.
#[async_trait::async_trait]
pub trait Runner: Send + Sync {
    /// Run `angie -t -c <conf> -e stderr` (or the live test with `conf = None`).
    /// Returns `(success, combined_stderr_stdout)`.
    async fn angie_test(&self, conf: Option<&Path>) -> (bool, String);
    /// Reload Angie (`systemctl reload angie` on a real box). Returns
    /// `(success, message)`.
    async fn reload(&self) -> (bool, String);
    /// Current status-API `generation` counter, if reachable.
    async fn status_generation(&self) -> Option<u64>;
    /// Tail of the Angie error log for the report (best-effort, may be empty).
    async fn error_log_tail(&self) -> String;
}

/// The linter step (step a). Boxed so tests inject a no-op and production wires
/// `generator::lint::check_fileset` (a `todo!()` stub on this branch until the
/// generator work package lands).
pub type Linter = Box<dyn Fn(&FileSet, &LintPolicy) -> Vec<LintViolation> + Send + Sync>;

/// Everything the transaction needs, resolved from config + injectables.
pub struct ApplyCtx<'a> {
    pub cfg: &'a PanelConfig,
    pub runner: &'a dyn Runner,
    pub lint: &'a Linter,
    pub lint_policy: LintPolicy,
}

// --------------------------------------------------------------- entry points

/// Full transaction. Loads the staged fileset from `<data_dir>/staging/http.d`,
/// runs the pipeline, writes `apply-result.json`, and returns the report.
pub async fn run_apply(ctx: &ApplyCtx<'_>) -> anyhow::Result<ApplyReport> {
    let data_dir = &ctx.cfg.data_dir;
    let http_d = ctx.cfg.angie.http_d_dir.clone();
    let staging = StagingPaths::new(data_dir);

    // Load the staged, header-wrapped fileset the panel produced.
    let staged = load_staged_fileset(&staging.http_d)?;
    let synthetic_base = !ctx.cfg.angie.angie_conf.exists();

    // Preview diff (drives the report + apply_history).
    let diff_report = diff(&http_d, &staged).unwrap_or_else(|_| empty_diff());

    let mut report = base_report(synthetic_base).with_diff(&diff_report);

    // (a) Re-lint the staged fileset — abort on any violation.
    let violations = (ctx.lint)(&staged, &ctx.lint_policy);
    if !violations.is_empty() {
        report.result = ApplyResult::LintFailed;
        report.summary = format!("lint rejected {} file(s)", violations.len());
        report.lint_violations = violations.into_iter().map(Into::into).collect();
        write_report(data_dir, &report)?;
        return Ok(report);
    }

    // (b) Validate the STAGED config before touching /etc.
    let (ok, stderr) = ctx.runner.angie_test(Some(&staging.test_conf)).await;
    if !ok {
        report.result = ApplyResult::ValidationFailed;
        report.file_errors = map_stderr_to_files(&stderr, &staging.http_d, &http_d);
        report.stderr = stderr;
        report.summary = "staged config failed angie -t (nothing changed)".into();
        write_report(data_dir, &report)?;
        return Ok(report);
    }

    // From here on we mutate /etc — mark in-progress for crash recovery, and
    // snapshot first so any later failure can roll back.
    set_in_progress(data_dir, true)?;
    let snapshot = snapshot_now(data_dir, &http_d)?;

    // Run the swap+reload, rolling back on any failure after the swap.
    let outcome = swap_and_reload(ctx, &staged, &http_d, &snapshot).await;
    match outcome {
        Ok(()) => {
            report.result = ApplyResult::Ok;
            report.summary = format!(
                "applied {} change(s); reload confirmed",
                diff_report.added + diff_report.modified + diff_report.removed
            );
        }
        Err(fail) => {
            report.result = fail.result;
            report.stderr = fail.stderr;
            report.error_log_tail = fail.error_log_tail;
            report.file_errors = fail.file_errors;
            report.rollback = Some(fail.rollback);
            report.summary = fail.summary;
        }
    }

    set_in_progress(data_dir, false)?;
    write_report(data_dir, &report)?;
    Ok(report)
}

/// Steps d-g. On any failure after the swap starts, rolls back from `snapshot`,
/// reloads, and returns a populated `Failure`.
async fn swap_and_reload(
    ctx: &ApplyCtx<'_>,
    staged: &FileSet,
    http_d: &Path,
    snapshot: &Manifest,
) -> Result<(), Failure> {
    // (d) Atomic sync into the live http.d.
    if let Err(e) = sync_into_live(staged, http_d) {
        return Err(rollback(
            ctx,
            http_d,
            snapshot,
            ApplyResult::Error,
            format!("sync failed: {e:#}"),
            String::new(),
            Vec::new(),
        )
        .await);
    }

    // (e) Post-swap validation on the live tree.
    let (ok, stderr) = ctx.runner.angie_test(None).await;
    if !ok {
        let fe = map_stderr_to_files(&stderr, http_d, http_d);
        return Err(rollback(
            ctx,
            http_d,
            snapshot,
            ApplyResult::ValidationFailed,
            "post-swap angie -t failed; rolled back".into(),
            stderr,
            fe,
        )
        .await);
    }

    // (f) Reload + verify the generation counter incremented.
    let gen_before = ctx.runner.status_generation().await;
    let (reloaded, msg) = ctx.runner.reload().await;
    if !reloaded {
        let tail = ctx.runner.error_log_tail().await;
        return Err(rollback_with_log(
            ctx,
            http_d,
            snapshot,
            ApplyResult::ReloadFailed,
            format!("reload failed: {msg}; rolled back"),
            tail,
        )
        .await);
    }
    if !verify_reload(ctx.runner, gen_before).await {
        let tail = ctx.runner.error_log_tail().await;
        return Err(rollback_with_log(
            ctx,
            http_d,
            snapshot,
            ApplyResult::ReloadFailed,
            "reload did not take effect within timeout (generation did not \
             advance); rolled back"
                .into(),
            tail,
        )
        .await);
    }
    Ok(())
}

/// Poll the status API until `generation` advances past `before` (SIGHUP is
/// async). Returns true on confirmation. If the status API is unreachable
/// (no baseline), we cannot verify negatively — treat as confirmed so an
/// installation without the status endpoint still applies.
async fn verify_reload(runner: &dyn Runner, before: Option<u64>) -> bool {
    let Some(before) = before else {
        return true;
    };
    let deadline = tokio::time::Instant::now() + RELOAD_VERIFY_TIMEOUT;
    loop {
        if let Some(now) = runner.status_generation().await {
            if now > before {
                return true;
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

// ------------------------------------------------------------------- rollback

/// Collected failure state to fold into the report.
struct Failure {
    result: ApplyResult,
    summary: String,
    stderr: String,
    error_log_tail: String,
    file_errors: Vec<FileError>,
    rollback: RollbackOutcome,
}

#[allow(clippy::too_many_arguments)]
async fn rollback(
    ctx: &ApplyCtx<'_>,
    http_d: &Path,
    snapshot: &Manifest,
    result: ApplyResult,
    summary: String,
    stderr: String,
    file_errors: Vec<FileError>,
) -> Failure {
    let rb = do_rollback(ctx, http_d, snapshot).await;
    Failure {
        result,
        summary,
        stderr,
        error_log_tail: String::new(),
        file_errors,
        rollback: rb,
    }
}

async fn rollback_with_log(
    ctx: &ApplyCtx<'_>,
    http_d: &Path,
    snapshot: &Manifest,
    result: ApplyResult,
    summary: String,
    error_log_tail: String,
) -> Failure {
    let rb = do_rollback(ctx, http_d, snapshot).await;
    Failure {
        result,
        summary,
        stderr: String::new(),
        error_log_tail,
        file_errors: Vec::new(),
        rollback: rb,
    }
}

/// Restore the live http.d to `snapshot`, then reload so Angie serves the known
/// good config again.
async fn do_rollback(ctx: &ApplyCtx<'_>, http_d: &Path, snapshot: &Manifest) -> RollbackOutcome {
    match snapshot.restore_into(http_d) {
        Ok(actions) => {
            let (reloaded, msg) = ctx.runner.reload().await;
            RollbackOutcome {
                attempted: true,
                ok: reloaded,
                detail: if reloaded {
                    format!("restored {} file action(s), reloaded", actions.len())
                } else {
                    format!("restored files but reload failed: {msg}")
                },
            }
        }
        Err(e) => RollbackOutcome {
            attempted: true,
            ok: false,
            detail: format!("rollback FAILED: {e:#}"),
        },
    }
}

// ----------------------------------------------------------- atomic live sync

/// (d) Sync `staged` into the live `http_d`: atomic same-dir write of each
/// staged file, then delete managed files no longer in the set. Foreign files
/// are never touched. NO cross-directory renames — under
/// `ProtectSystem=strict`, `/etc` and `/var/lib` are distinct bind-mounts, so a
/// rename between them returns EXDEV (explicit review finding, PLAN.md §2.2/§11).
fn sync_into_live(staged: &FileSet, http_d: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(http_d)
        .with_context(|| format!("ensuring http.d {}", http_d.display()))?;

    // Delete managed files that are no longer wanted (preserve foreign files).
    for f in scan_dir(http_d)? {
        if f.managed && !staged.contains_key(&f.name) {
            atomic::remove_in_dir(http_d, &f.name)
                .with_context(|| format!("removing stale {}", f.name))?;
        }
    }
    // Write the wanted set (temp file lives INSIDE http_d — same-dir rename).
    for (name, body) in staged {
        atomic::write_in_dir(http_d, name, body.as_bytes(), CONF_MODE)
            .with_context(|| format!("syncing {name}"))?;
    }
    Ok(())
}

// --------------------------------------------------------------- snapshotting

/// Capture the live http.d into `<data_dir>/backups/<ts>/manifest.json`,
/// rotating to keep the newest [`BACKUP_KEEP`].
fn snapshot_now(data_dir: &Path, http_d: &Path) -> anyhow::Result<Manifest> {
    let ts = now_epoch();
    let backups = data_dir.join("backups");
    let dir = backups.join(ts.to_string());
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating backup dir {}", dir.display()))?;
    let manifest = Manifest::capture(http_d, ts)?;
    atomic::write_in_dir(
        &dir,
        "manifest.json",
        manifest.to_json()?.as_bytes(),
        CONF_MODE,
    )?;
    rotate_backups(&backups, BACKUP_KEEP);
    Ok(manifest)
}

/// Keep the newest `keep` backup dirs (by name — timestamps sort lexically for
/// equal width, and numerically we sort explicitly), delete the rest.
fn rotate_backups(backups: &Path, keep: usize) {
    let mut dirs: Vec<(i64, PathBuf)> = match std::fs::read_dir(backups) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .filter_map(|e| {
                let ts: i64 = e.file_name().to_string_lossy().parse().ok()?;
                Some((ts, e.path()))
            })
            .collect(),
        Err(_) => return,
    };
    dirs.sort_by_key(|(ts, _)| *ts);
    if dirs.len() > keep {
        for (_, path) in &dirs[..dirs.len() - keep] {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

/// Latest snapshot manifest, if any (newest by timestamp dir name).
pub fn latest_snapshot(data_dir: &Path) -> Option<Manifest> {
    let backups = data_dir.join("backups");
    let mut dirs: Vec<(i64, PathBuf)> = std::fs::read_dir(&backups)
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let ts: i64 = e.file_name().to_string_lossy().parse().ok()?;
            Some((ts, e.path().join("manifest.json")))
        })
        .filter(|(_, p)| p.exists())
        .collect();
    dirs.sort_by_key(|(ts, _)| *ts);
    let (_, path) = dirs.pop()?;
    Manifest::from_json_file(&path).ok()
}

// ----------------------------------------------------- stderr → file mapping

/// Map `angie -t` stderr lines to the offending staged filename (PLAN.md §2.2
/// step 4). Angie emits diagnostics like:
///   `nginx: [emerg] unexpected "}" in /path/to/20-host-3.conf:12`
/// We extract the `<path>:<line>` reference and translate the path to the
/// staged basename when it points at either the staging or live http.d, so the
/// UI can highlight the exact host file.
fn map_stderr_to_files(stderr: &str, staging_http_d: &Path, live_http_d: &Path) -> Vec<FileError> {
    let mut out = Vec::new();
    for line in stderr.lines() {
        // Only emerg/error lines carry a location worth mapping.
        if !(line.contains("[emerg]") || line.contains("[error]") || line.contains("[crit]")) {
            continue;
        }
        let (file, line_no) = extract_location(line, staging_http_d, live_http_d);
        out.push(FileError {
            file,
            line: line_no,
            message: line.trim().to_string(),
        });
    }
    out
}

/// Pull `<path>.conf:<line>` (or bare `<basename>.conf:<line>`) out of a line.
/// Angie references files by absolute path under the staging or live http.d; we
/// reduce that to the basename so the UI can highlight the exact host file
/// (both http.d roots are passed only to document the mapping target).
fn extract_location(
    line: &str,
    _staging_http_d: &Path,
    _live_http_d: &Path,
) -> (Option<String>, Option<u32>) {
    for token in line.split_whitespace() {
        // Trim trailing punctuation Angie sometimes appends.
        let token = token.trim_end_matches([',', ')', '"']);
        let Some(conf_idx) = token.find(".conf") else {
            continue;
        };
        let path_part = &token[..conf_idx + 5];
        let rest = &token[conf_idx + 5..];
        let line_no = rest
            .strip_prefix(':')
            .and_then(|n| n.split(|c: char| !c.is_ascii_digit()).next())
            .and_then(|n| n.parse::<u32>().ok());

        let basename = Path::new(path_part)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path_part.to_string());
        return (Some(basename), line_no);
    }
    (None, None)
}

// --------------------------------------------------------------- misc helpers

fn load_staged_fileset(staging_http_d: &Path) -> anyhow::Result<FileSet> {
    let mut set: FileSet = BTreeMap::new();
    for f in scan_dir(staging_http_d)? {
        // The panel may stage foreign-looking files only in dev; keep them all
        // — validation and lint are the gates, not the header here.
        set.insert(f.name, f.contents);
    }
    Ok(set)
}

fn base_report(synthetic_base: bool) -> ApplyReport {
    ApplyReport {
        timestamp: now_epoch(),
        result: ApplyResult::Error,
        diff: None,
        lint_violations: Vec::new(),
        stderr: String::new(),
        file_errors: Vec::new(),
        error_log_tail: String::new(),
        rollback: None,
        synthetic_base,
        summary: String::new(),
    }
}

fn empty_diff() -> DiffReport {
    DiffReport {
        files: Vec::new(),
        foreign: Vec::new(),
        added: 0,
        modified: 0,
        removed: 0,
        unchanged: 0,
        has_drift: false,
    }
}

/// Write `apply-result.json` atomically (mode 0644 so the panel can read it).
fn write_report(data_dir: &Path, report: &ApplyReport) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(report)?;
    atomic::write_in_dir(data_dir, APPLY_RESULT_FILE, json.as_bytes(), 0o644)
}

/// Read the last apply report (panel side).
pub fn read_report(data_dir: &Path) -> Option<ApplyReport> {
    let raw = std::fs::read_to_string(data_dir.join(APPLY_RESULT_FILE)).ok()?;
    serde_json::from_str(&raw).ok()
}

fn set_in_progress(data_dir: &Path, on: bool) -> anyhow::Result<()> {
    let marker = data_dir.join(IN_PROGRESS_MARKER);
    if on {
        atomic::write_in_dir(
            data_dir,
            IN_PROGRESS_MARKER,
            now_epoch().to_string().as_bytes(),
            0o644,
        )
    } else {
        atomic::remove_in_dir(data_dir, IN_PROGRESS_MARKER)?;
        let _ = marker; // silence unused on non-unix
        Ok(())
    }
}

/// Whether an apply is marked in-progress (a previous run may have crashed).
pub fn apply_in_progress(data_dir: &Path) -> bool {
    data_dir.join(IN_PROGRESS_MARKER).exists()
}

// -------------------------------------------------------------- crash recovery

/// Called by the panel on startup (it wires the actual call — see PLAN.md §2.2
/// step 2, "recovery-check when the panel starts"). If an apply was interrupted
/// mid-swap (marker present), re-validate the live config; if it is now invalid,
/// restore the latest snapshot and reload. Safe to call when no apply was in
/// progress (returns `RecoveryOutcome::Clean`).
///
/// The panel passes a [`Runner`] so this stays testable and off the real
/// `systemctl` in dev. It clears the in-progress marker when done.
pub async fn recover_if_interrupted(
    cfg: &PanelConfig,
    runner: &dyn Runner,
) -> anyhow::Result<RecoveryOutcome> {
    let data_dir = &cfg.data_dir;
    if !apply_in_progress(data_dir) {
        return Ok(RecoveryOutcome::Clean);
    }
    tracing::warn!("apply-in-progress marker found on startup: verifying live config");

    let (ok, _stderr) = runner.angie_test(None).await;
    if ok {
        // The interrupted apply left a valid config — just clear the marker.
        set_in_progress(data_dir, false)?;
        return Ok(RecoveryOutcome::RecoveredValid);
    }

    // Invalid: restore the newest snapshot and reload.
    let Some(snapshot) = latest_snapshot(data_dir) else {
        set_in_progress(data_dir, false)?;
        return Ok(RecoveryOutcome::NoSnapshot);
    };
    snapshot
        .restore_into(&cfg.angie.http_d_dir)
        .context("restoring latest snapshot during recovery")?;
    let (reloaded, msg) = runner.reload().await;
    set_in_progress(data_dir, false)?;
    if reloaded {
        Ok(RecoveryOutcome::RolledBack)
    } else {
        Ok(RecoveryOutcome::RolledBackReloadFailed(msg))
    }
}

/// Result of [`recover_if_interrupted`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryOutcome {
    /// No apply was in progress.
    Clean,
    /// A marker was present but the live config validated — marker cleared.
    RecoveredValid,
    /// Live config was invalid; restored from the latest snapshot and reloaded.
    RolledBack,
    /// Restored but the reload failed (detail).
    RolledBackReloadFailed(String),
    /// Live config was invalid but no snapshot existed to restore.
    NoSnapshot,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;

    use crate::apply::stage::stage;
    use crate::apply::testutil::{foreign_body, managed_body, managed_fileset};

    // ------------------------------------------------------------ fake runner

    /// Scriptable [`Runner`]. Each `angie_test` and `reload` call pops the next
    /// queued result (or repeats the last). `generation` advances by one on a
    /// successful reload so `verify_reload` sees the bump.
    struct FakeRunner {
        test_results: Mutex<std::collections::VecDeque<(bool, String)>>,
        reload_ok: bool,
        generation: AtomicU64,
        error_log: String,
        calls: Mutex<Vec<String>>,
    }

    impl FakeRunner {
        fn new() -> Self {
            Self {
                test_results: Mutex::new(std::collections::VecDeque::new()),
                reload_ok: true,
                generation: AtomicU64::new(1),
                error_log: "2026/07/06 [emerg] bind() to 0.0.0.0:443 failed".into(),
                calls: Mutex::new(Vec::new()),
            }
        }
        /// Queue angie -t results in call order.
        fn with_tests(mut self, results: &[(bool, &str)]) -> Self {
            self.test_results =
                Mutex::new(results.iter().map(|(ok, s)| (*ok, s.to_string())).collect());
            self
        }
        fn with_reload_ok(mut self, ok: bool) -> Self {
            self.reload_ok = ok;
            self
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl Runner for FakeRunner {
        async fn angie_test(&self, conf: Option<&Path>) -> (bool, String) {
            self.calls.lock().unwrap().push(format!(
                "test:{}",
                conf.map(|p| p.display().to_string())
                    .unwrap_or_else(|| "live".into())
            ));
            let mut q = self.test_results.lock().unwrap();
            if q.len() > 1 {
                q.pop_front().unwrap()
            } else {
                q.front().cloned().unwrap_or((true, String::new()))
            }
        }
        async fn reload(&self) -> (bool, String) {
            self.calls.lock().unwrap().push("reload".into());
            if self.reload_ok {
                self.generation.fetch_add(1, Ordering::SeqCst);
                (true, "reloaded".into())
            } else {
                (false, "reload failed".into())
            }
        }
        async fn status_generation(&self) -> Option<u64> {
            Some(self.generation.load(Ordering::SeqCst))
        }
        async fn error_log_tail(&self) -> String {
            self.error_log.clone()
        }
    }

    // --------------------------------------------------------------- fixtures

    struct Fixture {
        _data: tempfile::TempDir,
        _http: tempfile::TempDir,
        _base: tempfile::TempDir,
        cfg: PanelConfig,
    }

    /// A PanelConfig whose data_dir / http_d_dir / angie_conf live in tempdirs
    /// (so tests never touch /etc and run on macOS).
    fn fixture() -> Fixture {
        let data = tempfile::tempdir().unwrap();
        let http = tempfile::tempdir().unwrap();
        let base = tempfile::tempdir().unwrap();
        let angie_conf = base.path().join("angie.conf");
        std::fs::write(
            &angie_conf,
            "events {}\nhttp {\n    include /etc/angie/http.d/*.conf;\n}\n",
        )
        .unwrap();
        let cfg: PanelConfig = toml::from_str(&format!(
            "data_dir = \"{}\"\n[angie]\nbin = \"/bin/true\"\nhttp_d_dir = \"{}\"\nangie_conf = \"{}\"",
            data.path().display(),
            http.path().display(),
            angie_conf.display(),
        ))
        .unwrap();
        Fixture {
            _data: data,
            _http: http,
            _base: base,
            cfg,
        }
    }

    fn noop_lint() -> Linter {
        Box::new(|_files, _policy| Vec::new())
    }

    fn ctx<'a>(fx: &'a Fixture, runner: &'a dyn Runner, lint: &'a Linter) -> ApplyCtx<'a> {
        ApplyCtx {
            cfg: &fx.cfg,
            runner,
            lint,
            lint_policy: crate::apply::lint_policy(&fx.cfg),
        }
    }

    /// Stage `files` into the fixture's data_dir so `run_apply` can read them.
    fn stage_files(fx: &Fixture, files: &[(&str, &str)]) {
        stage(
            &managed_fileset(files.iter().copied()),
            &fx.cfg.data_dir,
            &fx.cfg.angie,
        )
        .unwrap();
    }

    // ------------------------------------------------------------------ tests

    #[tokio::test]
    async fn happy_path_applies_and_reports_ok() {
        let fx = fixture();
        // Seed one pre-existing managed file in the live dir (to be replaced).
        std::fs::write(
            fx.cfg.angie.http_d_dir.join("20-old.conf"),
            managed_body("old"),
        )
        .unwrap();
        stage_files(&fx, &[("10-a.conf", "alpha"), ("30-b.conf", "beta")]);

        let runner = FakeRunner::new().with_tests(&[(true, "")]);
        let lint = noop_lint();
        let report = run_apply(&ctx(&fx, &runner, &lint)).await.unwrap();

        assert_eq!(report.result, ApplyResult::Ok, "summary={}", report.summary);
        // Live dir now holds exactly the staged set; the stale managed file is gone.
        let live = &fx.cfg.angie.http_d_dir;
        assert!(live.join("10-a.conf").exists());
        assert!(live.join("30-b.conf").exists());
        assert!(!live.join("20-old.conf").exists());
        // Report was written and reads back.
        let round = read_report(&fx.cfg.data_dir).unwrap();
        assert_eq!(round.result, ApplyResult::Ok);
        // Both a pre-swap (staged conf) and post-swap (live) angie -t ran, plus reload.
        let calls = runner.calls();
        assert_eq!(calls.iter().filter(|c| c.starts_with("test:")).count(), 2);
        assert!(calls.contains(&"reload".to_string()));
        // A snapshot was captured.
        assert!(latest_snapshot(&fx.cfg.data_dir).is_some());
        // Marker cleared.
        assert!(!apply_in_progress(&fx.cfg.data_dir));
    }

    #[tokio::test]
    async fn preserves_foreign_file_through_apply() {
        let fx = fixture();
        std::fs::write(
            fx.cfg.angie.http_d_dir.join("99-foreign.conf"),
            foreign_body("operator's own"),
        )
        .unwrap();
        stage_files(&fx, &[("10-a.conf", "a")]);

        let runner = FakeRunner::new();
        let lint = noop_lint();
        let report = run_apply(&ctx(&fx, &runner, &lint)).await.unwrap();
        assert_eq!(report.result, ApplyResult::Ok);
        // Foreign file untouched; managed file added.
        assert_eq!(
            std::fs::read_to_string(fx.cfg.angie.http_d_dir.join("99-foreign.conf")).unwrap(),
            foreign_body("operator's own")
        );
        assert!(fx.cfg.angie.http_d_dir.join("10-a.conf").exists());
    }

    #[tokio::test]
    async fn lint_failure_aborts_before_touching_etc() {
        let fx = fixture();
        std::fs::write(
            fx.cfg.angie.http_d_dir.join("20-existing.conf"),
            managed_body("must survive"),
        )
        .unwrap();
        stage_files(&fx, &[("10-a.conf", "a")]);

        let runner = FakeRunner::new();
        let lint: Linter = Box::new(|_f, _p| {
            vec![LintViolation {
                file: "10-a.conf".into(),
                line: Some(3),
                message: "load_module is forbidden".into(),
            }]
        });
        let report = run_apply(&ctx(&fx, &runner, &lint)).await.unwrap();

        assert_eq!(report.result, ApplyResult::LintFailed);
        assert_eq!(report.lint_violations.len(), 1);
        assert_eq!(report.lint_violations[0].file, "10-a.conf");
        // Nothing was changed and angie -t never ran.
        assert!(fx.cfg.angie.http_d_dir.join("20-existing.conf").exists());
        assert!(!fx.cfg.angie.http_d_dir.join("10-a.conf").exists());
        assert!(runner.calls().is_empty());
    }

    #[tokio::test]
    async fn staged_validation_failure_leaves_etc_untouched() {
        let fx = fixture();
        std::fs::write(
            fx.cfg.angie.http_d_dir.join("20-existing.conf"),
            managed_body("keep me"),
        )
        .unwrap();
        stage_files(&fx, &[("10-bad.conf", "broken")]);

        // First (pre-swap) angie -t fails; map it to the staged file.
        let stderr = "nginx: [emerg] unexpected \"}\" in /somewhere/staging/http.d/10-bad.conf:7\n";
        let runner = FakeRunner::new().with_tests(&[(false, stderr)]);
        let lint = noop_lint();
        let report = run_apply(&ctx(&fx, &runner, &lint)).await.unwrap();

        assert_eq!(report.result, ApplyResult::ValidationFailed);
        // Live dir is untouched: nothing added, nothing removed.
        assert!(fx.cfg.angie.http_d_dir.join("20-existing.conf").exists());
        assert!(!fx.cfg.angie.http_d_dir.join("10-bad.conf").exists());
        // stderr mapped file:line back to the staged basename.
        assert_eq!(report.file_errors.len(), 1);
        assert_eq!(report.file_errors[0].file.as_deref(), Some("10-bad.conf"));
        assert_eq!(report.file_errors[0].line, Some(7));
        // No swap happened, so no rollback and no reload.
        assert!(report.rollback.is_none());
        assert!(!runner.calls().contains(&"reload".to_string()));
        assert!(!apply_in_progress(&fx.cfg.data_dir));
    }

    #[tokio::test]
    async fn post_swap_validation_failure_rolls_back() {
        let fx = fixture();
        // Live starts with a known-good managed file.
        std::fs::write(
            fx.cfg.angie.http_d_dir.join("20-old.conf"),
            managed_body("good-v1"),
        )
        .unwrap();
        stage_files(
            &fx,
            &[("20-old.conf", "good-v2"), ("30-new.conf", "brand new")],
        );

        // Pre-swap test passes, post-swap test fails → rollback.
        let runner = FakeRunner::new().with_tests(&[
            (true, ""),
            (
                false,
                "nginx: [emerg] duplicate listen in /etc/angie/http.d/30-new.conf:2\n",
            ),
        ]);
        let lint = noop_lint();
        let report = run_apply(&ctx(&fx, &runner, &lint)).await.unwrap();

        assert_eq!(report.result, ApplyResult::ValidationFailed);
        let rb = report.rollback.unwrap();
        assert!(rb.attempted && rb.ok, "rollback detail={}", rb.detail);
        // Rolled back to the snapshot: 20-old.conf is v1 again, 30-new.conf gone.
        assert_eq!(
            std::fs::read_to_string(fx.cfg.angie.http_d_dir.join("20-old.conf")).unwrap(),
            managed_body("good-v1")
        );
        assert!(!fx.cfg.angie.http_d_dir.join("30-new.conf").exists());
        // Rollback reloaded Angie back to the good config.
        assert!(runner.calls().iter().filter(|c| *c == "reload").count() >= 1);
        assert!(!apply_in_progress(&fx.cfg.data_dir));
    }

    #[tokio::test]
    async fn reload_failure_rolls_back_and_captures_error_log() {
        let fx = fixture();
        std::fs::write(
            fx.cfg.angie.http_d_dir.join("20-old.conf"),
            managed_body("v1"),
        )
        .unwrap();
        stage_files(&fx, &[("20-old.conf", "v2")]);

        // Both angie -t pass, but reload fails (e.g. port conflict).
        let runner = FakeRunner::new()
            .with_tests(&[(true, "")])
            .with_reload_ok(false);
        let lint = noop_lint();
        let report = run_apply(&ctx(&fx, &runner, &lint)).await.unwrap();

        assert_eq!(report.result, ApplyResult::ReloadFailed);
        // Error-log tail captured for the UI (port conflicts only show here).
        assert!(report.error_log_tail.contains("bind()"));
        let rb = report.rollback.unwrap();
        assert!(rb.attempted);
        // Rolled back to v1.
        assert_eq!(
            std::fs::read_to_string(fx.cfg.angie.http_d_dir.join("20-old.conf")).unwrap(),
            managed_body("v1")
        );
    }

    #[tokio::test]
    async fn backups_rotate_keeping_latest_n() {
        let dir = tempfile::tempdir().unwrap();
        let backups = dir.path().join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        for ts in 1..=(BACKUP_KEEP as i64 + 5) {
            std::fs::create_dir_all(backups.join(ts.to_string())).unwrap();
        }
        rotate_backups(&backups, BACKUP_KEEP);
        let remaining: Vec<i64> = std::fs::read_dir(&backups)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().to_string_lossy().parse().ok())
            .collect();
        assert_eq!(remaining.len(), BACKUP_KEEP);
        // Oldest ones were dropped; newest kept.
        assert!(!remaining.contains(&1));
        assert!(remaining.contains(&(BACKUP_KEEP as i64 + 5)));
    }

    // ------------------------------------------------------- crash recovery

    #[tokio::test]
    async fn recovery_noop_without_marker() {
        let fx = fixture();
        let runner = FakeRunner::new();
        let outcome = recover_if_interrupted(&fx.cfg, &runner).await.unwrap();
        assert_eq!(outcome, RecoveryOutcome::Clean);
    }

    #[tokio::test]
    async fn recovery_clears_marker_when_live_config_valid() {
        let fx = fixture();
        set_in_progress(&fx.cfg.data_dir, true).unwrap();
        let runner = FakeRunner::new().with_tests(&[(true, "")]);
        let outcome = recover_if_interrupted(&fx.cfg, &runner).await.unwrap();
        assert_eq!(outcome, RecoveryOutcome::RecoveredValid);
        assert!(!apply_in_progress(&fx.cfg.data_dir));
    }

    #[tokio::test]
    async fn recovery_restores_snapshot_when_live_config_invalid() {
        let fx = fixture();
        // A good snapshot exists...
        std::fs::write(
            fx.cfg.angie.http_d_dir.join("20-a.conf"),
            managed_body("good"),
        )
        .unwrap();
        snapshot_now(&fx.cfg.data_dir, &fx.cfg.angie.http_d_dir).unwrap();
        // ...then an interrupted apply left a broken live tree + marker.
        std::fs::write(
            fx.cfg.angie.http_d_dir.join("20-a.conf"),
            managed_body("BROKEN"),
        )
        .unwrap();
        std::fs::write(
            fx.cfg.angie.http_d_dir.join("30-partial.conf"),
            managed_body("half-written"),
        )
        .unwrap();
        set_in_progress(&fx.cfg.data_dir, true).unwrap();

        // Live config is invalid → restore the snapshot + reload.
        let runner = FakeRunner::new().with_tests(&[(false, "broken")]);
        let outcome = recover_if_interrupted(&fx.cfg, &runner).await.unwrap();
        assert_eq!(outcome, RecoveryOutcome::RolledBack);
        // Restored to the good snapshot.
        assert_eq!(
            std::fs::read_to_string(fx.cfg.angie.http_d_dir.join("20-a.conf")).unwrap(),
            managed_body("good")
        );
        assert!(!fx.cfg.angie.http_d_dir.join("30-partial.conf").exists());
        assert!(!apply_in_progress(&fx.cfg.data_dir));
    }

    // ------------------------------------ real runner against a fake `angie`

    #[tokio::test]
    async fn real_runner_drives_a_fake_angie_binary() {
        use crate::apply::runner::RealRunner;
        let bin_dir = tempfile::tempdir().unwrap();
        let fake = bin_dir.path().join("angie");
        // Echoes its argv to stderr and exits 0 → angie -t "passes".
        std::fs::write(
            &fake,
            "#!/bin/sh\necho \"fake angie called: $@\" 1>&2\nexit 0\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let cfg: PanelConfig = toml::from_str(&format!(
            "data_dir = \"{}\"\n[angie]\nbin = \"{}\"",
            bin_dir.path().display(),
            fake.display(),
        ))
        .unwrap();
        let runner = RealRunner::new(&cfg);
        let (ok, out) = runner.angie_test(None).await;
        assert!(ok);
        assert!(out.contains("fake angie called"));
    }

    #[tokio::test]
    async fn real_runner_reports_failing_fake_angie() {
        use crate::apply::runner::RealRunner;
        let bin_dir = tempfile::tempdir().unwrap();
        let fake = bin_dir.path().join("angie");
        std::fs::write(
            &fake,
            "#!/bin/sh\necho \"[emerg] simulated failure in /x/http.d/20-h.conf:5\" 1>&2\nexit 1\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let cfg: PanelConfig = toml::from_str(&format!(
            "data_dir = \"{}\"\n[angie]\nbin = \"{}\"",
            bin_dir.path().display(),
            fake.display(),
        ))
        .unwrap();
        let runner = RealRunner::new(&cfg);
        let (ok, out) = runner.angie_test(None).await;
        assert!(!ok);
        // The mapper turns this stderr into a file:line the UI can highlight.
        let mapped = map_stderr_to_files(&out, Path::new("/x/http.d"), Path::new("/x/http.d"));
        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].file.as_deref(), Some("20-h.conf"));
        assert_eq!(mapped[0].line, Some(5));
    }
}
