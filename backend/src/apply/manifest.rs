//! Snapshot manifest of the managed files in an http.d directory.
//!
//! Snapshotting = writing a [`Manifest`] (filename → hash + full contents of
//! every file carrying the MANAGED-BY header). Rollback = driving the live
//! directory back to exactly that manifest: rewrite/create every listed file,
//! delete managed files that are *not* in the manifest, and never touch a
//! foreign (unmanaged) file. See PLAN.md §2.2 step 5.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::atomic;

/// Files written by the apply pipeline get mode 0644 (Angie workers read them).
pub const CONF_MODE: u32 = 0o644;

/// One managed file as captured in a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestEntry {
    /// sha256 (hex) of the full on-disk file (header included) at capture time.
    pub sha256: String,
    /// The complete file body, header included — enough to recreate it byte for
    /// byte on rollback.
    pub contents: String,
}

/// The managed subset of an http.d directory at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Manifest {
    /// When the snapshot was taken (UTC epoch seconds).
    pub timestamp: i64,
    /// filename → entry, for files bearing the MANAGED-BY header only.
    pub files: BTreeMap<String, ManifestEntry>,
    /// Foreign (unmanaged) filenames observed at capture time. Recorded for the
    /// UI; rollback never touches these.
    #[serde(default)]
    pub foreign: Vec<String>,
}

/// A single `*.conf` file found in the live directory, already classified.
pub struct ScannedFile {
    pub name: String,
    pub contents: String,
    /// True when the file carries a MANAGED-BY header (parsed by the generator).
    pub managed: bool,
    /// For managed files: does the on-disk body still hash to its own declared
    /// header hash? `false` = someone hand-edited it (drift). `None` = foreign.
    pub header_intact: Option<bool>,
}

/// sha256 hex of arbitrary bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// Scan a directory for `*.conf` files and classify each as managed (via the
/// generator's [`crate::generator::managed_meta`]) or foreign. Temp files
/// (`.angie-panel.*.tmp`) and non-`.conf` entries are ignored, matching Angie's
/// own `include http.d/*.conf` glob. Missing directory → empty list.
pub fn scan_dir(dir: &Path) -> anyhow::Result<Vec<ScannedFile>> {
    let mut out = Vec::new();
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => {
            return Err(anyhow::Error::from(e).context(format!("reading dir {}", dir.display())))
        }
    };
    for entry in rd {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".conf") || name.starts_with(atomic::TMP_PREFIX) {
            continue;
        }
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let contents = std::fs::read_to_string(entry.path())
            .with_context(|| format!("reading {}", entry.path().display()))?;
        let (managed, header_intact) = match super::header::parse(&contents) {
            Some(meta) => (true, Some(meta.hash_matches)),
            None => (false, None),
        };
        out.push(ScannedFile {
            name,
            contents,
            managed,
            header_intact,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

impl Manifest {
    /// Capture the managed files currently in `live_dir`.
    pub fn capture(live_dir: &Path, timestamp: i64) -> anyhow::Result<Self> {
        let mut files = BTreeMap::new();
        let mut foreign = Vec::new();
        for f in scan_dir(live_dir)? {
            if f.managed {
                files.insert(
                    f.name,
                    ManifestEntry {
                        sha256: sha256_hex(f.contents.as_bytes()),
                        contents: f.contents,
                    },
                );
            } else {
                foreign.push(f.name);
            }
        }
        Ok(Manifest {
            timestamp,
            files,
            foreign,
        })
    }

    /// Serialize to pretty JSON.
    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Read a manifest from a `manifest.json` file.
    pub fn from_json_file(path: &Path) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading manifest {}", path.display()))?;
        Ok(serde_json::from_str(&raw)?)
    }

    /// Restore `live_dir` to exactly this manifest:
    /// - create/overwrite every file listed here (atomic same-dir write);
    /// - delete managed files present on disk but *absent* from the manifest;
    /// - never touch a foreign (unmanaged) file — even one whose name later
    ///   collides is left alone unless it now carries our header.
    ///
    /// Returns the list of `(action, filename)` performed, for the report.
    pub fn restore_into(&self, live_dir: &Path) -> anyhow::Result<Vec<(RestoreAction, String)>> {
        let mut actions = Vec::new();
        let on_disk = scan_dir(live_dir)?;

        // Delete managed files that are no longer in the manifest.
        for f in &on_disk {
            if f.managed && !self.files.contains_key(&f.name) {
                atomic::remove_in_dir(live_dir, &f.name)?;
                actions.push((RestoreAction::Deleted, f.name.clone()));
            }
        }

        // (Re)write every manifest file whose on-disk bytes differ. Foreign
        // files sharing a name are refused rather than clobbered.
        let disk_by_name: BTreeMap<&str, &ScannedFile> =
            on_disk.iter().map(|f| (f.name.as_str(), f)).collect();
        for (name, entry) in &self.files {
            if let Some(existing) = disk_by_name.get(name.as_str()) {
                if !existing.managed {
                    anyhow::bail!(
                        "refusing to overwrite foreign file {name:?} during rollback \
                         (it does not carry the panel's MANAGED-BY header)"
                    );
                }
                if existing.contents == entry.contents {
                    continue; // already correct
                }
            }
            atomic::write_in_dir(live_dir, name, entry.contents.as_bytes(), CONF_MODE)?;
            actions.push((RestoreAction::Written, name.clone()));
        }
        Ok(actions)
    }
}

/// What `restore_into` did to a given file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreAction {
    Written,
    Deleted,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::testutil::{foreign_body, managed_body};

    #[test]
    fn capture_separates_managed_and_foreign() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("20-a.conf"), managed_body("aaa")).unwrap();
        std::fs::write(
            dir.path().join("99-foreign.conf"),
            foreign_body("hand-written"),
        )
        .unwrap();
        // Non-.conf and temp files are ignored.
        std::fs::write(dir.path().join("notes.txt"), "ignore").unwrap();
        std::fs::write(dir.path().join(".angie-panel.x.conf.tmp"), "ignore").unwrap();

        let m = Manifest::capture(dir.path(), 123).unwrap();
        assert_eq!(m.timestamp, 123);
        assert_eq!(m.files.keys().collect::<Vec<_>>(), vec!["20-a.conf"]);
        assert_eq!(m.foreign, vec!["99-foreign.conf".to_string()]);
    }

    #[test]
    fn roundtrips_through_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("20-a.conf"), managed_body("body")).unwrap();
        let m = Manifest::capture(dir.path(), 1).unwrap();
        let restored: Manifest = serde_json::from_str(&m.to_json().unwrap()).unwrap();
        assert_eq!(m, restored);
    }

    #[test]
    fn restore_recreates_deletes_managed_and_preserves_foreign() {
        let live = tempfile::tempdir().unwrap();
        // Snapshot state: one managed file + a foreign file.
        std::fs::write(live.path().join("20-a.conf"), managed_body("v1")).unwrap();
        std::fs::write(live.path().join("keepme.conf"), foreign_body("foreign")).unwrap();
        let snap = Manifest::capture(live.path(), 1).unwrap();

        // Mutate: change the managed file, add a new managed file, delete the
        // original — leave the foreign file untouched.
        std::fs::write(live.path().join("20-a.conf"), managed_body("v2-edited")).unwrap();
        std::fs::write(live.path().join("30-b.conf"), managed_body("newly added")).unwrap();

        let actions = snap.restore_into(live.path()).unwrap();

        // 20-a.conf is back to v1; 30-b.conf (managed, not in snapshot) deleted.
        assert_eq!(
            std::fs::read_to_string(live.path().join("20-a.conf")).unwrap(),
            managed_body("v1")
        );
        assert!(!live.path().join("30-b.conf").exists());
        // Foreign file preserved verbatim.
        assert_eq!(
            std::fs::read_to_string(live.path().join("keepme.conf")).unwrap(),
            foreign_body("foreign")
        );

        assert!(actions
            .iter()
            .any(|(a, n)| *a == RestoreAction::Deleted && n == "30-b.conf"));
        assert!(actions
            .iter()
            .any(|(a, n)| *a == RestoreAction::Written && n == "20-a.conf"));
    }

    #[test]
    fn restore_is_noop_when_already_matching() {
        let live = tempfile::tempdir().unwrap();
        std::fs::write(live.path().join("20-a.conf"), managed_body("stable")).unwrap();
        let snap = Manifest::capture(live.path(), 1).unwrap();
        let actions = snap.restore_into(live.path()).unwrap();
        assert!(actions.is_empty());
    }
}
