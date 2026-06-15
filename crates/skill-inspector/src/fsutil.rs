//! Small std-only filesystem helpers for the action layer: recursive copy and a
//! move-with-copy-fallback (so quarantine/restore works across filesystems).

use std::path::Path;

/// Recursively copy `src` dir to `dst` (created). Files and subdirs only.
pub fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// `EXDEV` — "cross-device link". The one rename failure for which a copy+delete fallback is
/// the *correct* behavior. macOS and Linux both use 18.
const EXDEV: i32 = 18;

/// Move `src` dir to `dst`. Tries an atomic rename first (no partial state on failure). The
/// copy+delete fallback runs ONLY for a genuine cross-device rename — for any other rename
/// error (e.g. a read-only parent) the error is returned and the source is left fully intact,
/// preserving prior state (FR-014). On a cross-device copy failure the partial `dst` is removed.
pub fn move_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(EXDEV) => {
            // Different filesystems: copy fully, then delete the source.
            if let Err(copy_err) = copy_dir(src, dst) {
                let _ = std::fs::remove_dir_all(dst);
                return Err(copy_err);
            }
            // If the source can't be removed, undo the copy so we don't leave a silent
            // duplicate (src intact + dst populated) — prior state is preserved either way.
            if let Err(rm_err) = std::fs::remove_dir_all(src) {
                let _ = std::fs::remove_dir_all(dst);
                return Err(rm_err);
            }
            Ok(())
        }
        Err(e) => Err(e),
    }
}
