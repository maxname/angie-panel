//! Diff of the live managed config against a staged FileSet — the data behind
//! the UI "Apply preview" page (PLAN.md §2.2 step 2).
//!
//! Only managed files (those carrying the MANAGED-BY header) participate in the
//! Added/Modified/Removed/Unchanged comparison. Foreign files are reported
//! separately and never counted as removals — the panel leaves them alone.
//! Drift (a managed file hand-edited on disk so its body no longer matches its
//! own header hash) is flagged distinctly so the UI can warn before applying.

use serde::Serialize;
use similar::TextDiff;

use super::manifest::scan_dir;
use crate::generator::FileSet;

/// Per-file comparison outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileStatus {
    /// Present in the staged set, absent live.
    Added,
    /// Present in both, different bodies.
    Modified,
    /// Managed live, absent from the staged set (will be deleted).
    Removed,
    /// Identical bodies.
    Unchanged,
}

/// One entry of the preview.
#[derive(Debug, Clone, Serialize)]
pub struct FileDiff {
    pub name: String,
    pub status: FileStatus,
    /// Unified text diff, only for `Modified` (empty otherwise).
    pub unified: String,
    /// The live managed file was hand-edited on disk (body != header hash).
    /// The operator's local change would be overwritten by this apply.
    pub drift: bool,
}

/// A foreign (unmanaged) file the panel found and will not touch.
#[derive(Debug, Clone, Serialize)]
pub struct ForeignFile {
    pub name: String,
}

/// Full preview report. Serializes into the ApplyReport and the UI.
#[derive(Debug, Clone, Serialize)]
pub struct DiffReport {
    pub files: Vec<FileDiff>,
    pub foreign: Vec<ForeignFile>,
    /// Convenience roll-ups for the UI.
    pub added: usize,
    pub modified: usize,
    pub removed: usize,
    pub unchanged: usize,
    /// Any managed file on disk is drifted (hand-edited).
    pub has_drift: bool,
}

impl DiffReport {
    /// True when applying would change something on disk.
    pub fn has_changes(&self) -> bool {
        self.added + self.modified + self.removed > 0
    }
}

/// Compare the managed files currently in `live_dir` against `staged` (a
/// header-wrapped FileSet, filename → full body).
pub fn diff(live_dir: &std::path::Path, staged: &FileSet) -> anyhow::Result<DiffReport> {
    let on_disk = scan_dir(live_dir)?;

    let mut files: Vec<FileDiff> = Vec::new();
    let mut foreign: Vec<ForeignFile> = Vec::new();

    // Index live managed files by name; record foreign files.
    let mut live_managed: std::collections::BTreeMap<&str, &str> =
        std::collections::BTreeMap::new();
    let mut drift_names: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for f in &on_disk {
        if f.managed {
            live_managed.insert(&f.name, &f.contents);
            if f.header_intact == Some(false) {
                drift_names.insert(&f.name);
            }
        } else {
            foreign.push(ForeignFile {
                name: f.name.clone(),
            });
        }
    }

    // Walk the union of names in a stable order.
    let mut names: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    names.extend(live_managed.keys().copied());
    names.extend(staged.keys().map(|s| s.as_str()));

    let (mut added, mut modified, mut removed, mut unchanged) = (0, 0, 0, 0);
    let mut has_drift = false;

    for name in names {
        let live = live_managed.get(name).copied();
        let want = staged.get(name).map(|s| s.as_str());
        let drift = drift_names.contains(name);
        if drift {
            has_drift = true;
        }
        let (status, unified) = match (live, want) {
            (None, Some(_)) => {
                added += 1;
                (FileStatus::Added, String::new())
            }
            (Some(_), None) => {
                removed += 1;
                (FileStatus::Removed, String::new())
            }
            (Some(a), Some(b)) if a == b => {
                unchanged += 1;
                (FileStatus::Unchanged, String::new())
            }
            (Some(a), Some(b)) => {
                modified += 1;
                (FileStatus::Modified, unified_diff(name, a, b))
            }
            (None, None) => unreachable!("name came from one of the two maps"),
        };
        files.push(FileDiff {
            name: name.to_string(),
            status,
            unified,
            drift,
        });
    }

    Ok(DiffReport {
        files,
        foreign,
        added,
        modified,
        removed,
        unchanged,
        has_drift,
    })
}

/// Unified text diff (git-style `---/+++` headers, 3 lines of context).
fn unified_diff(name: &str, old: &str, new: &str) -> String {
    TextDiff::from_lines(old, new)
        .unified_diff()
        .context_radius(3)
        .header(&format!("a/{name}"), &format!("b/{name}"))
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::testutil::{foreign_body, managed_body, managed_fileset};

    #[test]
    fn classifies_added_modified_removed_unchanged() {
        let live = tempfile::tempdir().unwrap();
        // Live managed files: keep (unchanged), edit (modified), drop (removed).
        std::fs::write(live.path().join("10-keep.conf"), managed_body("keep")).unwrap();
        std::fs::write(live.path().join("20-edit.conf"), managed_body("old body")).unwrap();
        std::fs::write(live.path().join("30-drop.conf"), managed_body("bye")).unwrap();

        let staged = managed_fileset([
            ("10-keep.conf", "keep"),
            ("20-edit.conf", "new body"),
            ("40-new.conf", "fresh"),
        ]);

        let report = diff(live.path(), &staged).unwrap();
        assert_eq!(report.added, 1);
        assert_eq!(report.modified, 1);
        assert_eq!(report.removed, 1);
        assert_eq!(report.unchanged, 1);
        assert!(report.has_changes());
        assert!(!report.has_drift);

        let by_name = |n: &str| report.files.iter().find(|f| f.name == n).unwrap();
        assert_eq!(by_name("40-new.conf").status, FileStatus::Added);
        assert_eq!(by_name("20-edit.conf").status, FileStatus::Modified);
        assert_eq!(by_name("30-drop.conf").status, FileStatus::Removed);
        assert_eq!(by_name("10-keep.conf").status, FileStatus::Unchanged);
        // Modified file carries a non-empty unified diff mentioning both bodies.
        let ud = &by_name("20-edit.conf").unified;
        assert!(ud.contains("old body"));
        assert!(ud.contains("new body"));
    }

    #[test]
    fn reports_foreign_files_without_counting_them_removed() {
        let live = tempfile::tempdir().unwrap();
        std::fs::write(live.path().join("99-foreign.conf"), foreign_body("theirs")).unwrap();
        let staged = managed_fileset([("10-a.conf", "a")]);

        let report = diff(live.path(), &staged).unwrap();
        assert_eq!(report.removed, 0, "foreign files are never 'removed'");
        assert_eq!(report.added, 1);
        assert_eq!(report.foreign.len(), 1);
        assert_eq!(report.foreign[0].name, "99-foreign.conf");
    }

    #[test]
    fn flags_drift_on_hand_edited_managed_file() {
        let live = tempfile::tempdir().unwrap();
        // Stamp a header for "original" but write a different body — drift.
        let good = managed_body("original");
        let header_line = good.split_once('\n').unwrap().0;
        let drifted = format!("{header_line}\ntampered\n");
        std::fs::write(live.path().join("20-a.conf"), &drifted).unwrap();

        let staged = managed_fileset([("20-a.conf", "clean")]);
        let report = diff(live.path(), &staged).unwrap();
        assert!(report.has_drift);
        let f = report.files.iter().find(|f| f.name == "20-a.conf").unwrap();
        assert!(f.drift);
        assert_eq!(f.status, FileStatus::Modified);
    }

    #[test]
    fn empty_live_dir_is_all_additions() {
        let live = tempfile::tempdir().unwrap();
        let staged = managed_fileset([("10-a.conf", "a"), ("20-b.conf", "b")]);
        let report = diff(live.path(), &staged).unwrap();
        assert_eq!(report.added, 2);
        assert!(report.has_changes());
    }
}
