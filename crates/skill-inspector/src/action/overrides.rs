//! Tier-1 per-folder disable (contracts/action-api.md): write `skillOverrides[<id>]="off"`
//! into `<folder>/.claude/settings.local.json` (preserving other keys), tracked by a
//! `FolderOverrideRecord`. Never moves/deletes skill files; reversible via key removal or
//! `previous_value` restore (FR-011/023).

use std::path::Path;

use crate::settings;
use crate::state::FolderOverrideRecord;

/// Write the override and return the bookkeeping record to append to `InspectorState`.
pub fn disable(
    skill_key: &str,
    agent: &str,
    folder: &Path,
    id: &str,
    now: &str,
) -> std::io::Result<FolderOverrideRecord> {
    let previous = settings::set_override(folder, id, "off")?;
    Ok(FolderOverrideRecord {
        skill_key: skill_key.to_string(),
        agent: agent.to_string(),
        folder: folder.to_string_lossy().into_owned(),
        settings_path: settings::settings_path(folder)
            .to_string_lossy()
            .into_owned(),
        previous_value: previous,
        new_value: "off".to_string(),
        disabled_at: now.to_string(),
    })
}

/// Reverse a Tier-1 disable: restore `previous_value` (or remove the key).
pub fn enable(record: &FolderOverrideRecord, id: &str) -> std::io::Result<()> {
    let folder = Path::new(&record.folder);
    settings::clear_override(folder, id, record.previous_value.as_deref())
}
