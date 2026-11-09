//! Crash-safe filesystem primitives shared across the apply pipeline.
//!
//! The single rule (PLAN.md §2.2 step 6): temp file **inside the target
//! directory**, fsync it, same-dir rename over the destination, then fsync the
//! directory fd. Cross-directory renames are forbidden — under
//! `ProtectSystem=strict` `/etc` and `/var/lib` are separate bind-mounts, so a
//! rename across them returns `EXDEV`; and on the R4S's SD card the directory
//! fsync is what actually makes the rename durable.

use std::io::Write;
use std::path::Path;

use anyhow::{bail, Context};

/// Prefix for in-place temp files. The `.` and the missing `.conf` suffix keep
/// them out of Angie's `include http.d/*.conf` glob, so a crash mid-write can
/// never leave a half-written file that Angie would load.
pub const TMP_PREFIX: &str = ".angie-panel.";
const TMP_SUFFIX: &str = ".tmp";

/// Atomically write `content` to `dir/name`, keeping the temp file inside
/// `dir` (never a cross-directory rename). `mode` is applied to the file on
/// Unix. `dir` must already exist.
pub fn write_in_dir(dir: &Path, name: &str, content: &[u8], mode: u32) -> anyhow::Result<()> {
    if name.contains('/') || name.contains('\\') {
        bail!("refusing to write file with a path separator in its name: {name:?}");
    }
    let tmp = dir.join(format!("{TMP_PREFIX}{name}{TMP_SUFFIX}"));
    let dst = dir.join(name);

    // Write + fsync the temp file.
    {
        let mut f = std::fs::File::create(&tmp)
            .with_context(|| format!("creating temp file {}", tmp.display()))?;
        f.write_all(content)
            .with_context(|| format!("writing {}", tmp.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            f.set_permissions(std::fs::Permissions::from_mode(mode))
                .with_context(|| format!("chmod {}", tmp.display()))?;
        }
        #[cfg(not(unix))]
        let _ = mode;
        f.sync_all()
            .with_context(|| format!("fsync {}", tmp.display()))?;
    }

    // Same-dir rename (atomic replace of the destination).
    std::fs::rename(&tmp, &dst).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        anyhow::Error::from(e).context(format!(
            "renaming {} -> {} (same-dir only; cross-dir renames EXDEV under \
             ProtectSystem=strict)",
            tmp.display(),
            dst.display()
        ))
    })?;

    fsync_dir(dir)?;
    Ok(())
}

/// Remove `dir/name` and fsync the directory so the unlink is durable. A
/// missing file is not an error (idempotent).
pub fn remove_in_dir(dir: &Path, name: &str) -> anyhow::Result<()> {
    let path = dir.join(name);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(anyhow::Error::from(e).context(format!("removing {}", path.display())))
        }
    }
    fsync_dir(dir)?;
    Ok(())
}

/// fsync a directory fd so a rename/unlink within it survives power loss.
/// No-op error on platforms that reject opening a directory for fsync.
pub fn fsync_dir(dir: &Path) -> anyhow::Result<()> {
    match std::fs::File::open(dir) {
        Ok(f) => {
            // Directory fsync is unsupported on some platforms (returns
            // EINVAL/EBADF); treat that as best-effort rather than fatal.
            let _ = f.sync_all();
            Ok(())
        }
        Err(e) => Err(anyhow::Error::from(e).context(format!("opening dir {}", dir.display()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_and_replace_is_atomic_and_clean() {
        let dir = tempfile::tempdir().unwrap();
        write_in_dir(dir.path(), "a.conf", b"first", 0o644).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.conf")).unwrap(),
            "first"
        );
        write_in_dir(dir.path(), "a.conf", b"second", 0o644).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.conf")).unwrap(),
            "second"
        );
        // No leftover temp files.
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(TMP_PREFIX))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_sets_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        write_in_dir(dir.path(), "m.conf", b"x", 0o600).unwrap();
        let mode = std::fs::metadata(dir.path().join("m.conf"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn rejects_path_separator_in_name() {
        let dir = tempfile::tempdir().unwrap();
        assert!(write_in_dir(dir.path(), "../evil.conf", b"x", 0o644).is_err());
        assert!(write_in_dir(dir.path(), "sub/evil.conf", b"x", 0o644).is_err());
    }

    #[test]
    fn remove_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        write_in_dir(dir.path(), "gone.conf", b"x", 0o644).unwrap();
        remove_in_dir(dir.path(), "gone.conf").unwrap();
        assert!(!dir.path().join("gone.conf").exists());
        // Removing again is a no-op, not an error.
        remove_in_dir(dir.path(), "gone.conf").unwrap();
    }
}
