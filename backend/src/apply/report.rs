//! The ApplyReport the helper writes for the panel to pick up
//! (`<data_dir>/apply-result.json`, mode 0644). Mirrors ConfigtestReport's
//! contract in system.rs. See PLAN.md §2.1 (result of the helper) / §2.2.

use serde::{Deserialize, Serialize};

use super::diff::DiffReport;
use crate::generator::lint::LintViolation;

/// Filename of the JSON report the helper drops in `data_dir`.
pub const APPLY_RESULT_FILE: &str = "apply-result.json";

/// Round-trippable mirror of the generator's [`LintViolation`] (which is
/// serialize-only). The report is read back by the panel, so it needs
/// `Deserialize`; we convert on the way in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportedLint {
    pub file: String,
    pub line: Option<usize>,
    pub message: String,
}

impl From<LintViolation> for ReportedLint {
    fn from(v: LintViolation) -> Self {
        Self {
            file: v.file,
            line: v.line,
            message: v.message,
        }
    }
}

/// Terminal outcome of an apply attempt. String-serialized to match the
/// `apply_history.result` column values in migrations/0002_hosts.sql.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyResult {
    Ok,
    LintFailed,
    ValidationFailed,
    ReloadFailed,
    Error,
}

impl ApplyResult {
    pub fn is_ok(self) -> bool {
        matches!(self, ApplyResult::Ok)
    }

    /// Canonical snake_case token stored in the `apply_history.result` column
    /// (matches the values enumerated in migrations/0002_hosts.sql and the
    /// serde representation). The UI matches these exactly.
    pub fn as_str(self) -> &'static str {
        match self {
            ApplyResult::Ok => "ok",
            ApplyResult::LintFailed => "lint_failed",
            ApplyResult::ValidationFailed => "validation_failed",
            ApplyResult::ReloadFailed => "reload_failed",
            ApplyResult::Error => "error",
        }
    }
}

/// Maps an Angie `angie -t` error back to a staged filename where possible
/// (PLAN.md §2.2 step 4: `file:line` → the offending host).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileError {
    /// The staged filename the error was attributed to, if we could map it.
    pub file: Option<String>,
    pub line: Option<u32>,
    pub message: String,
}

/// Full apply report, written to `<data_dir>/apply-result.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyReport {
    pub timestamp: i64,
    pub result: ApplyResult,
    /// The preview diff this apply was based on (Serialize-only shape).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<serde_json::Value>,
    /// Lint violations, when `result == LintFailed`.
    #[serde(default)]
    pub lint_violations: Vec<ReportedLint>,
    /// Raw `angie -t` stderr (pre- or post-swap), when validation failed.
    #[serde(default)]
    pub stderr: String,
    /// Per-file error mapping derived from `stderr`.
    #[serde(default)]
    pub file_errors: Vec<FileError>,
    /// Tail of `/var/log/angie/error.log`, when a reload failed
    /// (port conflicts only surface here — `angie -t` does not bind sockets).
    #[serde(default)]
    pub error_log_tail: String,
    /// Whether a rollback ran and how it went.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback: Option<RollbackOutcome>,
    /// Whether validation used a synthetic base (real angie.conf unreadable).
    #[serde(default)]
    pub synthetic_base: bool,
    /// Human-readable one-liner for the UI banner.
    pub summary: String,
}

/// Outcome of a rollback triggered by a post-swap failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackOutcome {
    pub attempted: bool,
    pub ok: bool,
    pub detail: String,
}

impl ApplyReport {
    /// Attach a diff report (as opaque JSON so the report stays serialize-only).
    pub fn with_diff(mut self, diff: &DiffReport) -> Self {
        self.diff = serde_json::to_value(diff).ok();
        self
    }
}
