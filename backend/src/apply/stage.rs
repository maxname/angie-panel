//! Panel-side staging (unprivileged) — writes ONLY under `data_dir`.
//!
//! Lays down the header-wrapped FileSet into `<data_dir>/staging/http.d/` and a
//! `<data_dir>/staging/angie-test.conf` whose `include .../http.d/*.conf` line
//! points at the staging http.d. This is what lets the root helper run
//! `angie -t` **before** touching `/etc` (the critical validate-before-swap fix
//! from the review). See PLAN.md §2.2 step 1.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Serialize;

use super::atomic;
use super::manifest::CONF_MODE;
use crate::config::AngieConfig;
use crate::generator::FileSet;

pub const STAGING_DIR: &str = "staging";
pub const STAGING_HTTP_D: &str = "http.d";
pub const STAGING_STREAM_D: &str = "stream.d";
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
    pub stream_d: PathBuf,
    pub test_conf: PathBuf,
}

impl StagingPaths {
    pub fn new(data_dir: &Path) -> Self {
        let root = data_dir.join(STAGING_DIR);
        Self {
            http_d: root.join(STAGING_HTTP_D),
            stream_d: root.join(STAGING_STREAM_D),
            test_conf: root.join(STAGING_TEST_CONF),
            root,
        }
    }
}

/// Split a FileSet into (http.d files, stream.d files). Stream keys carry the
/// `stream.d/` prefix, which is stripped so the value is the bare filename.
pub fn split_fileset(files: &FileSet) -> (FileSet, FileSet) {
    let mut http = FileSet::new();
    let mut stream = FileSet::new();
    for (name, body) in files {
        if let Some(bare) = name.strip_prefix(crate::generator::STREAM_PREFIX) {
            stream.insert(bare.to_string(), body.clone());
        } else {
            http.insert(name.clone(), body.clone());
        }
    }
    (http, stream)
}

/// Write `files` (already header-wrapped and linted) into the staging http.d
/// and build the staging `angie-test.conf`. Stale managed files from a previous
/// staging run are removed so the staged set is exactly `files`.
pub fn stage(files: &FileSet, data_dir: &Path, angie: &AngieConfig) -> anyhow::Result<StageResult> {
    let paths = StagingPaths::new(data_dir);
    let (http_files, stream_files) = split_fileset(files);

    let mut written = Vec::new();
    stage_dir(&paths.http_d, &http_files, &mut written)?;
    // Prefix stream filenames in the report so the two dirs stay distinguishable.
    let mut stream_written = Vec::new();
    stage_dir(&paths.stream_d, &stream_files, &mut stream_written)?;
    written.extend(
        stream_written
            .into_iter()
            .map(|n| format!("{}{n}", crate::generator::STREAM_PREFIX)),
    );

    let (test_conf_body, synthetic_base) = build_test_conf(
        angie,
        &paths.http_d,
        &paths.stream_d,
        !stream_files.is_empty(),
    );
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

/// Write `files` into `dir` atomically, clearing stale *.conf from a previous
/// staging run first (this is our own scratch dir, not /etc). Appends written
/// names to `written`.
fn stage_dir(dir: &Path, files: &FileSet, written: &mut Vec<String>) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("creating staging dir {}", dir.display()))?;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".conf") || name.starts_with(atomic::TMP_PREFIX) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    for (name, body) in files {
        atomic::write_in_dir(dir, name, body.as_bytes(), CONF_MODE)
            .with_context(|| format!("staging {name}"))?;
        written.push(name.clone());
    }
    Ok(())
}

/// Build the `angie-test.conf` body. Prefer rewriting the real packaged
/// `angie.conf`'s http.d include to point at the staging dir (validates against
/// the true base); on failure to read it, fall back to a minimal synthetic base.
/// When `has_streams`, also point the stream include at the staging stream.d and
/// ensure a `stream {}` block is active. Returns `(body, synthetic_base)`.
fn build_test_conf(
    angie: &AngieConfig,
    staging_http_d: &Path,
    staging_stream_d: &Path,
    has_streams: bool,
) -> (String, bool) {
    match std::fs::read_to_string(&angie.angie_conf) {
        Ok(base) => {
            let mut out = rewrite_http_d_include(&base, staging_http_d);
            if has_streams {
                out = ensure_stream_include(&out, staging_stream_d);
            }
            (out, false)
        }
        Err(e) => {
            tracing::warn!(
                path = %angie.angie_conf.display(),
                error = %e,
                "angie.conf unreadable; validating against a synthetic base"
            );
            let mut out = synthetic_base(staging_http_d);
            if has_streams {
                let _ = writeln!(
                    out,
                    "stream {{\n    include {}/*.conf;\n}}",
                    staging_stream_d.display()
                );
            }
            (out, true)
        }
    }
}

/// Ensure the staging test-conf has an active `stream { include <staging>/*.conf; }`.
/// The packaged angie.conf ships the stream block COMMENTED OUT, so we either
/// uncomment+retarget it or append a fresh one. This only affects the throwaway
/// staging conf; activating streams in the LIVE angie.conf is a separate,
/// explicit step (the enable-streams helper).
fn ensure_stream_include(base: &str, staging_stream_d: &Path) -> String {
    let staged = staging_stream_d.display();
    let mut out = String::with_capacity(base.len() + 128);
    for line in base.lines() {
        // Drop the packaged COMMENTED stream scaffolding (`#stream {`,
        // `#    include .../stream.d/*.conf;`, `#}`) so we can append a clean
        // active block. Only commented lines are dropped — never live config.
        let commented = line.trim_start().starts_with('#');
        let inner = line.trim_start().trim_start_matches('#').trim_start();
        let is_scaffold = inner.starts_with("stream")
            || inner == "}"
            || (inner.starts_with("include") && line.contains("stream.d"));
        if commented && is_scaffold {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    let _ = writeln!(
        out,
        "\n# angie-panel: staging stream context\nstream {{\n    include {staged}/*.conf;\n}}"
    );
    out
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
