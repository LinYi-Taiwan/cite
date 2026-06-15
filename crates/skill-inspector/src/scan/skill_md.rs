//! Shared skill walker + frontmatter parse (contracts/source-provider.md §Walker).
//!
//! For a readable root: find immediate child dirs containing `SKILL.md`, parse
//! `name`/`description` via the local `frontmatter` parser, compute a content hash, and flag
//! `metadata_complete=false` (never drop) on parse failure (FR-003).

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::scan::frontmatter;

/// One skill discovered under a root (pre-`Skill`: source/state/labels added by `scan`).
pub struct WalkedSkill {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub path: PathBuf,
    pub content_hash: String,
    pub metadata_complete: bool,
}

/// Walk one readable root: every immediate child dir holding a `SKILL.md` becomes a skill.
/// Deterministic order (sorted by id). A malformed `SKILL.md` is flagged, never dropped.
pub fn walk_root(root: &Path) -> Vec<WalkedSkill> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.join("SKILL.md").is_file())
        .collect();
    dirs.sort();

    for dir in dirs {
        let id = match dir.file_name().and_then(|n| n.to_str()) {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => continue,
        };
        let skill_md = dir.join("SKILL.md");
        let (name, description, parse_ok) = match std::fs::read_to_string(&skill_md) {
            Ok(text) => match frontmatter::parse(&text, &skill_md) {
                Ok(parsed) => (
                    parsed.frontmatter.name.clone(),
                    parsed.frontmatter.description.clone(),
                    true,
                ),
                // Malformed frontmatter: keep the skill, mark incomplete (FR-003).
                Err(_) => (None, None, false),
            },
            Err(_) => (None, None, false),
        };
        let metadata_complete = parse_ok && name.is_some() && description.is_some();
        let content_hash = hash_dir(&dir);
        out.push(WalkedSkill {
            id,
            name,
            description,
            path: dir,
            content_hash,
            metadata_complete,
        });
    }
    out
}

/// Content hash over a skill dir: every regular file's path-relative-to-dir + bytes, in
/// sorted order. Identical content ⇒ identical hash (duplicate-by-identity, FR-008) and a
/// stable safe-remove backup key. Returns `sha256:<hex>`.
pub fn hash_dir(dir: &Path) -> String {
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    collect_files(dir, dir, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = Sha256::new();
    for (rel, path) in files {
        hasher.update(rel.as_bytes());
        hasher.update([0u8]);
        if let Ok(bytes) = std::fs::read(&path) {
            hasher.update((bytes.len() as u64).to_le_bytes());
            hasher.update(&bytes);
        } else {
            hasher.update((0u64).to_le_bytes());
        }
        hasher.update([0u8]);
    }
    format!("sha256:{:x}", hasher.finalize())
}

fn collect_files(base: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect_files(base, &path, out);
        } else if path.is_file() {
            let rel = path
                .strip_prefix(base)
                .ok()
                .and_then(|p| p.to_str())
                .map(|s| s.replace('\\', "/"))
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            out.push((rel, path));
        }
    }
}
