//! Tier-2 global disable (research.md §4): move the skill dir into the tool quarantine,
//! recording everything needed to restore it to its EXACT original path (FR-011). Used for
//! sources with no native per-folder toggle (plugin skills, other agents). Reversible; restore
//! detects hash drift (warn, don't clobber).

use std::path::Path;

use crate::fsutil::move_dir;
use crate::scan::skill_md::hash_dir;
use crate::state::QuarantineRecord;

/// Move `original_path` into `<quarantine_dir>/<agent>/<id>` and build the restore record.
/// The arguments mirror the `QuarantineRecord` fields the caller needs to persist.
#[allow(clippy::too_many_arguments)]
pub fn disable(
    skill_key: &str,
    agent: &str,
    id: &str,
    original_path: &Path,
    original_root: &Path,
    content_hash: &str,
    quarantine_dir: &Path,
    now: &str,
) -> std::io::Result<QuarantineRecord> {
    let dest = quarantine_dir.join(agent).join(id);
    if dest.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "quarantine destination already occupied",
        ));
    }
    move_dir(original_path, &dest)?;
    Ok(QuarantineRecord {
        skill_key: skill_key.to_string(),
        id: id.to_string(),
        agent: agent.to_string(),
        original_root: original_root.to_string_lossy().into_owned(),
        original_path: original_path.to_string_lossy().into_owned(),
        quarantine_path: dest.to_string_lossy().into_owned(),
        content_hash: content_hash.to_string(),
        disabled_at: now.to_string(),
    })
}

/// Outcome of a restore, surfacing drift so the UI can warn.
pub struct RestoreOutcome {
    pub drifted: bool,
}

/// Move the quarantined dir back to its exact original path. Refuses to clobber an existing
/// occupant at the original path (drift). Reports a content-hash mismatch as `drifted`.
pub fn restore(record: &QuarantineRecord) -> std::io::Result<RestoreOutcome> {
    let src = Path::new(&record.quarantine_path);
    let dest = Path::new(&record.original_path);
    if dest.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "original path is occupied (drift); not clobbering",
        ));
    }
    move_dir(src, dest)?;
    let new_hash = hash_dir(dest);
    Ok(RestoreOutcome {
        drifted: new_hash != record.content_hash,
    })
}
