//! Parse stage (T009): walk the catalog directory and build a [`Catalog`] of [`Unit`]s.
//!
//! Catalog layout convention:
//! ```text
//! <catalog>/
//! ├── skills/<...>/<skill-dir>/SKILL.md     # one skill per dir, at any depth
//! │                            references/<stem>.md
//! └── blocks/<...>/<block-id>.md            # one reusable block per file, at any depth
//! ```
//! The skill's `id` is taken from its frontmatter; the block's id is its filename stem.
//! Intermediate directories are free-form grouping (by layer, team, domain — the
//! compiler imposes no taxonomy); ids must stay unique across the whole catalog.
//!
//! `@include` extraction (T014) and `{{Alias}}` extraction (T021) populate the
//! `includes`/`markers` fields; this stage leaves them empty until those modules land.

pub mod frontmatter;
pub mod include;
pub mod marker;

use std::path::Path;

use crate::diagnostics::{Code, Diagnostics};
use crate::model::{Block, Catalog, ImportRef, Reference, ReferenceMode, Skill, Source};

/// Parse the catalog rooted at `catalog`. Hard parse failures (malformed YAML, unreadable
/// files, duplicate ids) are collected into `diags`; a `None` return means a fatal error
/// prevented building a usable catalog.
pub fn parse_catalog(catalog: &Path, diags: &mut Diagnostics) -> Catalog {
    let mut cat = Catalog::new();

    parse_blocks(catalog, &mut cat, diags);
    parse_skills(catalog, &mut cat, diags);

    cat
}

/// Discovery never descends deeper than this below `skills/` / `blocks/` — a bound on
/// stack recursion so an adversarial mirrored repo cannot crash the compiler with a
/// pathologically deep tree.
const MAX_NESTING: usize = 32;

fn parse_blocks(catalog: &Path, cat: &mut Catalog, diags: &mut Diagnostics) {
    let blocks_dir = catalog.join("blocks");
    if !blocks_dir.is_dir() {
        return;
    }
    walk_blocks(&blocks_dir, cat, diags, 0);
}

/// Recursively collect `*.md` blocks under `dir`. Subdirectories are organizational
/// groupings the author chose — the compiler imposes no taxonomy. A block's id is its
/// filename stem regardless of depth, so two files with the same stem in different
/// groups collide (`duplicate block id`), keeping `@include <id>` unambiguous.
/// Hidden (dot-prefixed) entries and symlinks are skipped (symlinks are never
/// followed, mirroring the lockfile hash walk — a mirrored repo must not be able to
/// point discovery outside the catalog).
fn walk_blocks(dir: &Path, cat: &mut Catalog, diags: &mut Diagnostics, depth: usize) {
    if depth > MAX_NESTING {
        diags.error(
            Code::SchemaInvalid,
            format!(
                "catalog nesting exceeds {MAX_NESTING} levels at {}",
                dir.display()
            ),
        );
        return;
    }
    let entries = match read_dir_sorted(dir) {
        Ok(e) => e,
        Err(e) => {
            diags.error(
                Code::ConfigInvalid,
                format!("cannot read blocks dir {}: {e}", dir.display()),
            );
            return;
        }
    };
    for path in entries {
        if is_hidden(&path) {
            continue;
        }
        if is_symlink(&path) {
            diags.warning(
                Code::CatalogSkipped,
                format!("symlink skipped during block discovery: {}", path.display()),
            );
            continue;
        }
        if path.is_dir() {
            walk_blocks(&path, cat, diags, depth + 1);
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let Some(id) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                diags.error(
                    Code::ConfigInvalid,
                    format!("cannot read block `{id}`: {e}"),
                );
                continue;
            }
        };
        let includes = include::extract(&content);
        if cat.blocks.contains_key(&id) {
            diags.error(
                Code::SchemaInvalid,
                format!("duplicate block id `{id}` at {}", path.display()),
            );
            continue;
        }
        cat.blocks.insert(
            id.clone(),
            Block {
                id,
                content,
                includes,
                source_path: path,
            },
        );
    }
}

fn parse_skills(catalog: &Path, cat: &mut Catalog, diags: &mut Diagnostics) {
    let skills_dir = catalog.join("skills");
    if !skills_dir.is_dir() {
        return;
    }
    walk_skills(&skills_dir, cat, diags, 0);
}

/// Recursively discover skills under `dir`. A directory containing a `SKILL.md` is a
/// skill — its subdirectories (`references/`, `scripts/`, …) belong to it and are not
/// descended into. Any other directory is an organizational grouping the author chose
/// (by layer, by team, by domain — the compiler imposes no taxonomy) and is recursed.
/// Hidden (dot-prefixed) directories and symlinks are skipped (symlinks are never
/// followed — a mirrored repo must not be able to point discovery outside the
/// catalog). Skill ids stay globally unique regardless of grouping (`duplicate skill
/// id` otherwise), so imports and `dist/` output are unaffected by how the tree is
/// organized.
fn walk_skills(dir: &Path, cat: &mut Catalog, diags: &mut Diagnostics, depth: usize) {
    if depth > MAX_NESTING {
        diags.error(
            Code::SchemaInvalid,
            format!(
                "catalog nesting exceeds {MAX_NESTING} levels at {}",
                dir.display()
            ),
        );
        return;
    }
    let dirs = match read_dir_sorted(dir) {
        Ok(e) => e,
        Err(e) => {
            diags.error(
                Code::ConfigInvalid,
                format!("cannot read skills dir {}: {e}", dir.display()),
            );
            return;
        }
    };
    for entry in dirs {
        if is_hidden(&entry) {
            continue;
        }
        if is_symlink(&entry) {
            diags.warning(
                Code::CatalogSkipped,
                format!(
                    "symlink skipped during skill discovery: {}",
                    entry.display()
                ),
            );
            continue;
        }
        if !entry.is_dir() {
            continue;
        }
        let skill_md = entry.join("SKILL.md");
        if !skill_md.is_file() {
            walk_skills(&entry, cat, diags, depth + 1);
            continue;
        }
        match parse_one_skill(&entry, &skill_md, diags) {
            Some(skill) => {
                if cat.skills.contains_key(&skill.id) {
                    diags.error(
                        Code::SchemaInvalid,
                        format!(
                            "duplicate skill id `{}` at {}",
                            skill.id,
                            skill_md.display()
                        ),
                    );
                    continue;
                }
                cat.skills.insert(skill.id.clone(), skill);
            }
            None => continue,
        }
    }
}

/// True when the final path component starts with a dot (`.git`, `.DS_Store`, …).
fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|n| n.starts_with('.'))
}

/// True when the entry itself is a symlink (checked WITHOUT following it —
/// `Path::is_dir` resolves through symlinks and must not be trusted alone).
fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

fn parse_one_skill(dir: &Path, skill_md: &Path, diags: &mut Diagnostics) -> Option<Skill> {
    let text = match std::fs::read_to_string(skill_md) {
        Ok(t) => t,
        Err(e) => {
            diags.error(
                Code::ConfigInvalid,
                format!("cannot read {}: {e}", skill_md.display()),
            );
            return None;
        }
    };
    let parsed = match frontmatter::parse(&text, skill_md) {
        Ok(p) => p,
        Err(d) => {
            diags.push(d);
            return None;
        }
    };
    let fm = parsed.frontmatter;

    // `id` falls back to the directory name when frontmatter omits it; the schema stage
    // (T036) enforces that required fields are actually present.
    let id = fm
        .id
        .clone()
        .or_else(|| dir.file_name().and_then(|s| s.to_str()).map(str::to_string))
        .unwrap_or_default();

    let reference_mode = match &fm.reference_mode {
        Some(s) => match ReferenceMode::parse(s) {
            Some(m) => m,
            None => {
                diags.error(
                    Code::SchemaInvalid,
                    format!("skill `{id}` has invalid referenceMode `{s}`"),
                );
                ReferenceMode::default()
            }
        },
        None => ReferenceMode::default(),
    };

    let applies_to_all = fm.applies_to.as_deref() == Some("all");

    let imports = parse_imports(fm.imports.unwrap_or_default());

    let includes = include::extract(&parsed.body);
    let markers = marker::extract(&parsed.body);

    let references = parse_references(dir, &id, diags);

    Some(Skill {
        id,
        name: fm.name.unwrap_or_default(),
        description: fm.description.unwrap_or_default(),
        reference_mode,
        applies_to_all,
        imports,
        includes,
        markers,
        references,
        body: parsed.body,
        source_path: skill_md.to_path_buf(),
    })
}

/// Parse an `imports` map into typed [`ImportRef`]s. A value of the form
/// `<sourceId>:<subpath>` is a cross-repo import (FR-008); anything else is a local path.
fn parse_imports(
    raw: std::collections::BTreeMap<String, String>,
) -> std::collections::BTreeMap<String, ImportRef> {
    let mut out = std::collections::BTreeMap::new();
    for (alias, spec) in raw {
        out.insert(alias, parse_import_ref(&spec));
    }
    out
}

/// Parse an import spec: `<sourceId>:<subpath>[@<version>]` → `Repo`; `./path`, `../path`,
/// or a bare path `[@<version>]` → `Local`. A `:` only counts as a source separator when
/// the left side looks like an id (non-empty, no `/`/`.`); the trailing `@<version>` is
/// optional (FR-011 conflict detection keys on it).
pub fn parse_import_ref(spec: &str) -> ImportRef {
    // Peel an optional trailing `@version` (the version must not look like a path).
    let (base, version) = match spec.rsplit_once('@') {
        Some((b, v)) if !b.is_empty() && !v.is_empty() && !v.contains('/') => {
            (b, Some(v.to_string()))
        }
        _ => (spec, None),
    };

    if let Some((source_id, subpath)) = base.split_once(':') {
        let looks_like_source = !source_id.is_empty()
            && !source_id.contains('/')
            && !source_id.contains('.')
            && !subpath.starts_with('/'); // not a `://` URL scheme
        if looks_like_source {
            return ImportRef {
                source: Source::Repo(source_id.to_string()),
                subpath: subpath.to_string(),
                version,
                raw: spec.to_string(),
            };
        }
    }
    ImportRef {
        source: Source::Local,
        subpath: base.to_string(),
        version,
        raw: spec.to_string(),
    }
}

fn parse_references(dir: &Path, host_skill: &str, diags: &mut Diagnostics) -> Vec<Reference> {
    let refs_dir = dir.join("references");
    if !refs_dir.is_dir() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let entries = match read_dir_sorted(&refs_dir) {
        Ok(e) => e,
        Err(e) => {
            diags.error(
                Code::ConfigInvalid,
                format!("cannot read references for `{host_skill}`: {e}"),
            );
            return out;
        }
    };
    for path in entries {
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let Some(stem) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                diags.error(
                    Code::ConfigInvalid,
                    format!("cannot read reference `{stem}` for `{host_skill}`: {e}"),
                );
                continue;
            }
        };
        out.push(Reference {
            stem,
            host_skill: host_skill.to_string(),
            path,
            content,
        });
    }
    out
}

/// Read a directory's entries, sorted by path for determinism (no filesystem order leaks
/// into the build — FR-022).
fn read_dir_sorted(dir: &Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    paths.sort();
    Ok(paths)
}
