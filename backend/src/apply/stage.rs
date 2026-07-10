//! Panel-side staging (unprivileged) — writes ONLY under `data_dir`.
//!
//! Lays down the header-wrapped FileSet into `<data_dir>/staging/http.d/` and a
//! `<data_dir>/staging/angie-test.conf` whose `include .../http.d/*.conf` line
//! points at the staging http.d. This is what lets the root helper run
//! `angie -t` **before** touching `/etc` (the critical validate-before-swap fix
//! from the review). See PLAN.md §2.2 step 1.

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Serialize;

use super::atomic;
use super::manifest::CONF_MODE;
use crate::config::AngieConfig;
use crate::generator::FileSet;

pub const STAGING_DIR: &str = "staging";
pub const STAGING_HTTP_D: &str = "http.d";
pub const STAGING_TEST_CONF: &str = "angie-test.conf";

/// Result of staging, returned to the caller and folded into the report.
#[derive(Debug, Clone, Serialize)]
pub struct StageResult {
    /// Absolute path to `<data_dir>/staging`.
    pub staging_dir: PathBuf,
    /// Absolute path to the staged http.d.
    pub http_d_dir: PathBuf,
    /// Absolute path to the generated `angie-test.conf` for `angie -t -c`.
    pub test_conf: PathBuf,
    /// True when the real `angie.conf` could not be read and a minimal
    /// synthetic base was substituted (dev / off-device). The UI must surface
    /// that validation ran against a synthetic base, not the packaged config.
    pub synthetic_base: bool,
    /// Filenames written into the staging http.d.
    pub files: Vec<String>,
}

/// The staging directory layout under `data_dir` (created on demand).
pub struct StagingPaths {
    pub root: PathBuf,
    pub http_d: PathBuf,
    pub test_conf: PathBuf,
}

impl StagingPaths {
    pub fn new(data_dir: &Path) -> Self {
        let root = data_dir.join(STAGING_DIR);
        Self {
            http_d: root.join(STAGING_HTTP_D),
            test_conf: root.join(STAGING_TEST_CONF),
            root,
        }
    }
}

/// Write `files` (already header-wrapped and linted) into the staging http.d
/// and build the staging `angie-test.conf`. Stale managed files from a previous
/// staging run are removed so the staged set is exactly `files`.
pub fn stage(files: &FileSet, data_dir: &Path, angie: &AngieConfig) -> anyhow::Result<StageResult> {
    let paths = StagingPaths::new(data_dir);
    std::fs::create_dir_all(&paths.http_d)
        .with_context(|| format!("creating staging dir {}", paths.http_d.display()))?;

    // Clear out any *.conf left from a previous staging run (managed or not —
    // this is our own scratch dir, not /etc), then write the current set.
    for entry in std::fs::read_dir(&paths.http_d)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".conf") || name.starts_with(atomic::TMP_PREFIX) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let mut written = Vec::new();
    for (name, body) in files {
        atomic::write_in_dir(&paths.http_d, name, body.as_bytes(), CONF_MODE)
            .with_context(|| format!("staging {name}"))?;
        written.push(name.clone());
    }

    let (test_conf_body, synthetic_base) = build_test_conf(angie, &paths.http_d);
    atomic::write_in_dir(
        &paths.root,
        STAGING_TEST_CONF,
        test_conf_body.as_bytes(),
        CONF_MODE,
    )
    .with_context(|| format!("writing {}", paths.test_conf.display()))?;

    Ok(StageResult {
        staging_dir: paths.root,
        http_d_dir: paths.http_d,
        test_conf: paths.test_conf,
        synthetic_base,
        files: written,
    })
}

/// Build the `angie-test.conf` body. Prefer rewriting the real packaged
/// `angie.conf`'s http.d include to point at the staging dir (validates against
/// the true base); on failure to read it, fall back to a minimal synthetic base
/// that just includes the staging http.d. Returns `(body, synthetic_base)`.
fn build_test_conf(angie: &AngieConfig, staging_http_d: &Path) -> (String, bool) {
    match std::fs::read_to_string(&angie.angie_conf) {
        Ok(base) => (rewrite_http_d_include(&base, staging_http_d), false),
        Err(e) => {
            tracing::warn!(
                path = %angie.angie_conf.display(),
                error = %e,
                "angie.conf unreadable; validating against a synthetic base"
            );
            (synthetic_base(staging_http_d), true)
        }
    }
}

/// Rewrite every `include <...>/http.d/*.conf;` line in `base` so it points at
/// the staging http.d instead of the live one. Other includes are left as-is.
fn rewrite_http_d_include(base: &str, staging_http_d: &Path) -> String {
    let staged = staging_http_d.display();
    let mut out = String::with_capacity(base.len() + 128);
    let mut rewrote = false;
    for line in base.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("include ") && line.contains("http.d/") && line.contains("*.conf") {
            let indent = &line[..line.len() - trimmed.len()];
            out.push_str(&format!(
                "{indent}# staging-rewritten by angie-panel (was: {})\n",
                trimmed.trim_end()
            ));
            out.push_str(&format!("{indent}include {staged}/*.conf;\n"));
            rewrote = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !rewrote {
        // The base had no recognizable http.d include (unusual). Append one so
        // the staged files are still validated rather than silently skipped.
        out.push_str(&format!(
            "\n# angie-panel: no http.d include found in base; appended for staging\n\
             http {{\n    include {staged}/*.conf;\n}}\n"
        ));
    }
    out
}

/// Minimal self-contained config that loads only the staged http.d. Used when
/// the packaged `angie.conf` is unreadable (dev boxes, macOS). Deliberately
/// tiny: enough for `angie -t` to parse the staged server blocks.
fn synthetic_base(staging_http_d: &Path) -> String {
    let staged = staging_http_d.display();
    format!(
        "# SYNTHETIC angie-panel test base (real angie.conf was unreadable).\n\
         # Validation ran against this stand-in, not the packaged config.\n\
         events {{}}\n\
         http {{\n    \
             include {staged}/*.conf;\n\
         }}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::testutil::managed_fileset;

    fn angie_cfg(angie_conf: &Path) -> AngieConfig {
        AngieConfig {
            angie_conf: angie_conf.to_path_buf(),
            ..AngieConfig::default()
        }
    }

    #[test]
    fn stage_writes_fileset_and_test_conf_rewriting_include() {
        let data = tempfile::tempdir().unwrap();
        // A realistic packaged angie.conf with an http.d include.
        let base = tempfile::tempdir().unwrap();
        let angie_conf = base.path().join("angie.conf");
        std::fs::write(
            &angie_conf,
            "events {}\nhttp {\n    include /etc/angie/http.d/*.conf;\n}\n",
        )
        .unwrap();

        let files = managed_fileset([("10-a.conf", "a"), ("20-b.conf", "b")]);
        let res = stage(&files, data.path(), &angie_cfg(&angie_conf)).unwrap();

        assert!(!res.synthetic_base);
        assert_eq!(res.files, vec!["10-a.conf", "20-b.conf"]);
        // Files landed in staging/http.d.
        assert!(res.http_d_dir.join("10-a.conf").exists());
        assert!(res.http_d_dir.join("20-b.conf").exists());
        // test conf includes the *staging* dir, not /etc.
        let tc = std::fs::read_to_string(&res.test_conf).unwrap();
        assert!(tc.contains(&format!("include {}/*.conf;", res.http_d_dir.display())));
        // No *active* (non-comment) include of the live /etc dir remains — the
        // original is preserved only inside a `# ...` breadcrumb comment.
        assert!(!tc
            .lines()
            .any(|l| !l.trim_start().starts_with('#')
                && l.contains("include /etc/angie/http.d/*.conf;")));
    }

    #[test]
    fn stage_degrades_to_synthetic_base_when_angie_conf_missing() {
        let data = tempfile::tempdir().unwrap();
        let cfg = angie_cfg(Path::new("/nonexistent/angie.conf"));
        let files = managed_fileset([("10-a.conf", "a")]);
        let res = stage(&files, data.path(), &cfg).unwrap();

        assert!(res.synthetic_base);
        let tc = std::fs::read_to_string(&res.test_conf).unwrap();
        assert!(tc.contains("SYNTHETIC"));
        assert!(tc.contains(&format!("include {}/*.conf;", res.http_d_dir.display())));
    }

    #[test]
    fn restaging_removes_stale_files() {
        let data = tempfile::tempdir().unwrap();
        let cfg = angie_cfg(Path::new("/nonexistent/angie.conf"));
        stage(
            &managed_fileset([("10-a.conf", "a"), ("20-b.conf", "b")]),
            data.path(),
            &cfg,
        )
        .unwrap();
        // Re-stage with only one file: the other must be gone.
        let res = stage(&managed_fileset([("10-a.conf", "a2")]), data.path(), &cfg).unwrap();
        assert!(res.http_d_dir.join("10-a.conf").exists());
        assert!(!res.http_d_dir.join("20-b.conf").exists());
    }
}
