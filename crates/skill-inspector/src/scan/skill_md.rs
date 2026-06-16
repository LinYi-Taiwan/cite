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
/// Used for *convention* roots (`<base>/skills/`); manifest-declared skill dirs go through
/// `walk_skill_dir` directly (a plugin can point its skills anywhere, not just `skills/`).
pub fn walk_root(root: &Path) -> Vec<WalkedSkill> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.join("SKILL.md").is_file())
        .collect();
    dirs.sort();
    dirs.iter().filter_map(|d| walk_skill_dir(d)).collect()
}

/// One skill from its directory (must hold `SKILL.md`). The id is the dir name. `None` only when
/// `SKILL.md` is absent or the dir has no usable name; a malformed `SKILL.md` is kept and flagged
/// incomplete (FR-003). This is the unit a manifest's declared `skills: ["./path/to/dir"]` lists.
pub fn walk_skill_dir(dir: &Path) -> Option<WalkedSkill> {
    let skill_md = dir.join("SKILL.md");
    if !skill_md.is_file() {
        return None;
    }
    let id = dir
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())?
        .to_string();
    let (name, description, parse_ok) = read_frontmatter(&skill_md);
    let metadata_complete = parse_ok && name.is_some() && description.is_some();
    Some(WalkedSkill {
        id,
        name,
        description,
        path: dir.to_path_buf(),
        content_hash: hash_dir(dir),
        metadata_complete,
    })
}

/// Walk a `commands/` root: every `*.md` file (recursively) is a slash-command the agent loads,
/// so it belongs in the same inventory as `SKILL.md` skills (a command is a skill from the user's
/// view — it loads and is invocable). The id mirrors how the agent namespaces commands:
/// `commands/foo.md` → `foo`, `commands/sub/bar.md` → `sub:bar`. `name`/`description` come from the
/// command's own frontmatter (often `description`-only); a file with neither is flagged
/// `metadata_complete=false`, never dropped. Deterministic order (sorted by id). Missing/unreadable
/// dir ⇒ empty (an agent root without a `commands/` sibling contributes nothing).
pub fn walk_commands_root(root: &Path) -> Vec<WalkedSkill> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_md_files(root, &mut files);

    // Derive ids first, then sort by id (not raw path): the inventory is id-keyed, so id order is
    // the meaningful, OS/case-stable order. Files whose name yields no valid id are dropped here.
    let mut items: Vec<(String, PathBuf)> = files
        .into_iter()
        .filter_map(|f| command_id(root, &f).map(|id| (id, f)))
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = Vec::new();
    for (id, file) in items {
        let (name, description, parse_ok) = read_frontmatter(&file);
        // A command carries no separate `name`; its id IS the invocation. So "complete" means it
        // parsed and described itself — don't demand a `name` field commands never have.
        let metadata_complete = parse_ok && description.is_some();
        let content_hash = hash_file(&file);
        out.push(WalkedSkill {
            id,
            name,
            description,
            path: file,
            content_hash,
            metadata_complete,
        });
    }
    out
}

/// One command from a single declared `.md` file (a manifest's `commands: ["./path/x.md"]` entry,
/// where the dir may be anything — not just `commands/`). The id is the file stem, allowlist-gated
/// like the convention walker. `None` if the stem is empty/invalid. A missing/unreadable file is
/// still listed (the manifest declares it) but flagged incomplete.
pub fn walk_command_file(file: &Path) -> Option<WalkedSkill> {
    let stem = file.file_stem().and_then(|n| n.to_str())?;
    if !crate::scan::claude::is_valid_perm_segment(stem) {
        return None;
    }
    let (name, description, parse_ok) = read_frontmatter(file);
    let metadata_complete = parse_ok && description.is_some();
    Some(WalkedSkill {
        id: stem.to_string(),
        name,
        description,
        path: file.to_path_buf(),
        content_hash: hash_file(file),
        metadata_complete,
    })
}

/// `commands/<rel>.md` → namespaced id: drop the `.md` (any case), join path components with `:`
/// (`sub/bar.md` → `sub:bar`). Every component is validated against the same allowlist the
/// `Skill(<plugin>:<id>)` perm-name uses (`[A-Za-z0-9_.-]`), so a file/dir name carrying `:`, `)`,
/// whitespace, etc. can't inject into the inventory key or a downstream deny rule. `None` (⇒ the
/// file is skipped) if it's outside `root`, isn't a `.md`, or any component fails the allowlist.
fn command_id(root: &Path, file: &Path) -> Option<String> {
    let rel = file.strip_prefix(root).ok()?.to_str()?;
    // Strip a `.md` suffix case-insensitively (matches what `collect_md_files` admits).
    if rel.len() < 3 || !rel[rel.len() - 3..].eq_ignore_ascii_case(".md") {
        return None;
    }
    let stem = &rel[..rel.len() - 3];
    if stem.is_empty() {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in stem.split(['/', '\\']) {
        if !crate::scan::claude::is_valid_perm_segment(part) {
            return None;
        }
        parts.push(part);
    }
    Some(parts.join(":"))
}

/// Read `(name, description, parse_ok)` from a markdown file's frontmatter. `parse_ok` is false on
/// a read error or malformed YAML (caller decides completeness). Shared by skills and commands.
fn read_frontmatter(path: &Path) -> (Option<String>, Option<String>, bool) {
    match std::fs::read_to_string(path) {
        Ok(text) => match frontmatter::parse(&text, path) {
            Ok(parsed) => (
                parsed.frontmatter.name.clone(),
                parsed.frontmatter.description.clone(),
                true,
            ),
            Err(_) => (None, None, false),
        },
        Err(_) => (None, None, false),
    }
}

/// Recursively collect every `*.md` file under `dir` (used by the commands walker). Symlinks are
/// NOT followed: a symlinked dir/file could otherwise pull `.md` content from outside the scanned
/// root into the inventory (and then out over the serve modal), so we skip them by inspecting the
/// entry's own type rather than the (symlink-resolving) `is_dir`/`is_file`.
fn collect_md_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        if matches!(entry.file_type(), Ok(ft) if ft.is_symlink()) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_md_files(&path, out);
        } else if path.is_file()
            && path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            out.push(path);
        }
    }
}

/// Content hash over a single file (a command is one `.md`, not a dir): `sha256:<hex>` of its
/// bytes. Mirrors the `hash_dir` format so both kinds share the duplicate-by-identity machinery.
/// A read failure returns a distinct `sha256:unreadable` sentinel — never the hash of zero bytes,
/// which would make every unreadable file collide as a false duplicate.
pub fn hash_file(path: &Path) -> String {
    let Ok(bytes) = std::fs::read(path) else {
        return "sha256:unreadable".to_string();
    };
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    format!("sha256:{:x}", hasher.finalize())
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
        // Don't follow symlinks while hashing: a symlink inside the dir (even one that stays within
        // the install dir) could point at unrelated content and fold it into the content hash.
        // Mirrors `collect_md_files`.
        if matches!(entry.file_type(), Ok(ft) if ft.is_symlink()) {
            continue;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_commands_lists_nested_md_with_namespaced_ids() {
        let root = std::env::temp_dir().join(format!("si-cmd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("sub")).unwrap();
        // Top-level command with frontmatter description, no `name`.
        std::fs::write(
            root.join("specify.md"),
            "---\ndescription: make a spec\n---\nbody\n",
        )
        .unwrap();
        // Nested command → `:`-namespaced id.
        std::fs::write(root.join("sub").join("bar.md"), "no frontmatter here\n").unwrap();
        // A non-`.md` file is ignored.
        std::fs::write(root.join("README.txt"), "ignore me").unwrap();
        // Mixed-case `.MD` extension is admitted; the stem case is preserved.
        std::fs::write(root.join("Plan.MD"), "x").unwrap();
        // A name with a disallowed character (space) yields no valid id ⇒ dropped, not listed.
        std::fs::write(root.join("bad name.md"), "x").unwrap();

        let walked = walk_commands_root(&root);
        let ids: Vec<&str> = walked.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, vec!["Plan", "specify", "sub:bar"]);

        let specify = walked.iter().find(|w| w.id == "specify").unwrap();
        assert_eq!(specify.description.as_deref(), Some("make a spec"));
        assert!(specify.metadata_complete); // parsed + has a description
        assert!(specify.content_hash.starts_with("sha256:"));

        let bar = walked.iter().find(|w| w.id == "sub:bar").unwrap();
        assert!(bar.description.is_none());
        assert!(!bar.metadata_complete); // no frontmatter ⇒ incomplete, but still listed

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn walk_commands_missing_dir_is_empty() {
        let root = std::env::temp_dir().join(format!("si-cmd-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert!(walk_commands_root(&root).is_empty());
    }
}
