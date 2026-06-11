//! Resolve stage: markers → imports (T022), cross-repo pull (T023), lockfile (T024),
//! dependency dedup + version-conflict detection (T025).
//!
//! Produces a [`Resolution`]: the alias→unit-id map used to render `{{Alias}}` markers,
//! plus the set of external skills pulled into the bundle (deduped by stable id).

pub mod imports;
pub mod lock;
pub mod source;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::assemble;
use crate::diagnostics::{Code, Diagnostics};
use crate::model::{Catalog, ImportRef, Skill, Source};
use lock::Lockfile;
use source::SourceCache;

/// The result of the resolve stage.
#[derive(Debug, Clone, Default)]
pub struct Resolution {
    /// `(skill_id, alias)` → resolved unit id, for marker rendering.
    pub marker_targets: BTreeMap<(String, String), String>,
    /// External skills pulled into the bundle, deduped by id (FR-008/010).
    pub pulled: BTreeMap<String, PulledSkill>,
    /// Pulled id → the external ids *it* pulls in turn (transitive closure edges for
    /// shake; FR-008 "pull it and its dependencies").
    pub pulled_deps: BTreeMap<String, BTreeSet<String>>,
}

/// dep id → (version → importers), for conflict detection (FR-011).
type Versions = BTreeMap<String, BTreeMap<String, Vec<String>>>;

/// Hard cap on import-pull recursion. Dedup bounds re-visits, but a crafted mirror chain
/// of *unique* ids would otherwise recurse one stack frame per skill — a few thousand
/// frames overflows the stack (DoS). No legitimate dependency chain approaches this.
const MAX_PULL_DEPTH: usize = 128;

/// An external skill pulled in as a top-level bundle member.
#[derive(Debug, Clone)]
pub struct PulledSkill {
    /// The source this skill was pulled from — id collisions across *different* sources
    /// are a hard error (silent shadowing would be a supply-chain hole).
    pub source: String,
    pub id: String,
    /// `description` carried from the source skill's frontmatter so the emitted `SKILL.md`
    /// stays a conformant, loadable skill (the spec `name` is the slug `id`, re-rendered at
    /// emit; only `description` needs carrying).
    pub description: String,
    /// Body with its own `@include`s expanded against its source mirror and its own
    /// `{{Alias}}` markers rendered (a pulled skill is as fully compiled as a local one —
    /// FR-009: zero unresolved references in the bundle).
    pub content: String,
    /// The first importer that pulled it (deterministic), for the manifest `reason`.
    pub pulled_by: String,
    pub includes: Vec<String>,
    pub imports: BTreeMap<String, String>,
    /// The skill's own `references/<stem>.md` files, carried into the bundle (and pruned
    /// per-target at assemble like any catalog skill's).
    pub references: Vec<PulledReference>,
}

/// One `references/<stem>.md` of a pulled external skill.
#[derive(Debug, Clone)]
pub struct PulledReference {
    pub stem: String,
    pub content: String,
}

/// Run the resolve stage. `catalog_root` is the build's catalog dir (mirror paths and the
/// lockfile are resolved relative to it).
pub fn resolve(
    catalog: &Catalog,
    catalog_root: &Path,
    lock: &Lockfile,
    locked: bool,
    frozen: bool,
    diags: &mut Diagnostics,
) -> Resolution {
    let mut res = Resolution::default();
    let mut cache = SourceCache::new();
    let by_dir = skill_dir_index(catalog);
    // id → source currently being pulled (in-progress stack frames). Distinct from
    // `res.pulled` (completed): both are needed for cycle safety — see pull_external.
    let mut visiting: BTreeMap<String, String> = BTreeMap::new();

    // Conflict detection tracks BOTH catalog-level and transitive (pulled-skill) versioned
    // imports (FR-011); only Some versions are tracked.
    let mut versions: Versions = BTreeMap::new();

    for skill in catalog.skills.values() {
        let used = imports::check_markers(skill, true, diags);
        for alias in &used {
            let import = &skill.imports[alias];
            let Some(resolved) = resolve_one(
                import,
                skill,
                catalog_root,
                lock,
                locked,
                frozen,
                &by_dir,
                &mut cache,
                &mut res,
                &mut visiting,
                &mut versions,
                diags,
            ) else {
                continue;
            };

            res.marker_targets
                .insert((skill.id.clone(), alias.clone()), resolved.id.clone());

            if let Some(version) = &resolved.version {
                versions
                    .entry(resolved.id.clone())
                    .or_default()
                    .entry(version.clone())
                    .or_default()
                    .push(skill.id.clone());
            }
        }
    }

    detect_version_conflicts(&versions, diags);
    res
}

/// One resolved marker target.
struct Resolved {
    id: String,
    version: Option<String>,
}

#[allow(clippy::too_many_arguments)]
fn resolve_one(
    import: &ImportRef,
    importer: &Skill,
    catalog_root: &Path,
    lock: &Lockfile,
    locked: bool,
    frozen: bool,
    by_dir: &BTreeMap<PathBuf, String>,
    cache: &mut SourceCache,
    res: &mut Resolution,
    visiting: &mut BTreeMap<String, String>,
    versions: &mut Versions,
    diags: &mut Diagnostics,
) -> Option<Resolved> {
    match &import.source {
        Source::Repo(source_id) => {
            let id = pull_external(
                source_id,
                &import.subpath,
                &import.raw,
                &importer.id,
                catalog_root,
                lock,
                locked,
                frozen,
                cache,
                res,
                visiting,
                versions,
                0,
                diags,
            )?;
            Some(Resolved {
                id,
                version: import.version.clone(),
            })
        }
        Source::Local => {
            // A local import points at another catalog skill's directory.
            let base = importer.source_path.parent().unwrap_or(Path::new("."));
            let target_dir = base.join(&import.subpath);
            let canonical = std::fs::canonicalize(&target_dir).ok();
            let resolved_id = canonical.as_deref().and_then(|c| by_dir.get(c)).cloned();
            match resolved_id {
                Some(id) => {
                    // Already a catalog member (mounted/membership decided by shake in US3);
                    // no pull needed.
                    Some(Resolved {
                        id,
                        version: import.version.clone(),
                    })
                }
                None => {
                    diags.error(
                        Code::SourceUnavailable,
                        format!(
                            "import `{}` in skill `{}` does not resolve to a catalog skill",
                            import.raw, importer.id
                        ),
                    );
                    None
                }
            }
        }
    }
}

/// Pull an external skill from `source_id` into the bundle — **recursively**: its own
/// `@include`s are expanded against the mirror, its own `{{Alias}}` markers are resolved
/// and rendered (local-to-the-mirror imports and further cross-source imports are pulled
/// in turn), and its `references/` are carried. FR-008: importing a skill pulls it *and
/// its dependencies*; FR-009: the bundle ships zero unresolved references.
///
/// Returns the pulled skill's id. Dedup: an id already pulled from the SAME source (or
/// currently being pulled — import cycles are legal here, the link just points at the
/// co-bundled skill) returns immediately; the same id from a DIFFERENT source is a hard
/// error, never silent shadowing.
#[allow(clippy::too_many_arguments)]
fn pull_external(
    source_id: &str,
    skill_key: &str,
    import_raw: &str,
    pulled_by: &str,
    catalog_root: &Path,
    lock: &Lockfile,
    locked: bool,
    frozen: bool,
    cache: &mut SourceCache,
    res: &mut Resolution,
    visiting: &mut BTreeMap<String, String>,
    versions: &mut Versions,
    depth: usize,
    diags: &mut Diagnostics,
) -> Option<String> {
    if depth > MAX_PULL_DEPTH {
        diags.error(
            Code::SourceUnavailable,
            format!(
                "import `{import_raw}` in skill `{pulled_by}`: dependency chain exceeds \
                 {MAX_PULL_DEPTH} levels — refusing (runaway import chain)"
            ),
        );
        return None;
    }

    // Resolve the skill's id with a cheap borrow FIRST; the full catalog clone below only
    // happens when we actually proceed to pull (diamond re-references skip it).
    let ext_id = {
        let src_cat = match cache.get(catalog_root, lock, source_id, locked, frozen) {
            Ok(c) => c,
            Err(d) => {
                diags.push(d);
                return None;
            }
        };
        let Some(ext) = src_cat.skills.get(skill_key) else {
            diags.error(
                Code::SourceUnavailable,
                format!(
                    "import `{import_raw}` in skill `{pulled_by}`: source `{source_id}` \
                     has no skill `{skill_key}`"
                ),
            );
            return None;
        };
        ext.id.clone()
    };

    // Dedup (FR-010) / cycle guard. TWO checks are required and not redundant:
    // `res.pulled` covers COMPLETED pulls; `visiting` covers pulls currently on the
    // recursion stack (a cycle re-enters before completion). Removing either reintroduces
    // infinite recursion or duplicate bundling. Same id from a different source is a
    // dependency-confusion hazard → hard error, never first-one-wins.
    let prior_source = res
        .pulled
        .get(&ext_id)
        .map(|p| p.source.clone())
        .or_else(|| visiting.get(&ext_id).cloned());
    if let Some(prior) = prior_source {
        if prior != source_id {
            diags.error(
                Code::DepVersionConflict,
                format!(
                    "skill id `{ext_id}` is provided by two different sources: `{prior}` \
                     and `{source_id}` (via import `{import_raw}` in `{pulled_by}`) — \
                     refusing to silently shadow one with the other"
                ),
            );
            return None;
        }
        return Some(ext_id);
    }
    visiting.insert(ext_id.clone(), source_id.to_string());

    // Clone the source catalog out of the cache: recursion below needs `cache` mutable
    // again, and catalogs at this scale (O(10²) markdown units) are cheap to clone — once
    // per actually-pulled skill, not per reference (see dedup above).
    let src_cat = match cache.get(catalog_root, lock, source_id, locked, frozen) {
        Ok(c) => c.clone(),
        Err(d) => {
            diags.push(d);
            return None;
        }
    };
    let ext = src_cat
        .skills
        .get(skill_key)
        .expect("existence checked above");

    // Pulled cross-repo skills never pass through the local `validate` stage, yet their
    // `id` becomes a directory + the emitted `name` and their `description` is emitted
    // verbatim. Run the same conformance rules here so a third-party source cannot inject
    // an unloadable/path-unsafe skill into the bundle (FR-016).
    crate::schema::check_skill(ext, diags);
    let expanded = assemble::expand_includes(&ext.body, &src_cat);

    // Resolve the external skill's own markers within ITS context: local imports resolve
    // against the mirror's skills, repo imports recurse into the named source.
    // warn_unused = false: the user cannot edit a third-party skill, so its unused
    // imports would be unactionable noise. Undefined markers still hard-error.
    let used = imports::check_markers(ext, false, diags);
    let mirror_by_dir = skill_dir_index(&src_cat);
    let mut local_targets: BTreeMap<String, String> = BTreeMap::new();
    let mut deps: BTreeSet<String> = BTreeSet::new();
    for alias in &used {
        let imp = &ext.imports[alias];
        let resolved_id = match &imp.source {
            Source::Repo(other_source) => pull_external(
                other_source,
                &imp.subpath,
                &imp.raw,
                &ext_id,
                catalog_root,
                lock,
                locked,
                frozen,
                cache,
                res,
                visiting,
                versions,
                depth + 1,
                diags,
            ),
            Source::Local => {
                let base = ext.source_path.parent().unwrap_or(Path::new("."));
                let canonical = std::fs::canonicalize(base.join(&imp.subpath)).ok();
                match canonical.as_deref().and_then(|c| mirror_by_dir.get(c)) {
                    Some(sibling_id) => {
                        // A mirror-local sibling is still external to OUR catalog → pull it.
                        let sibling_id = sibling_id.clone();
                        pull_external(
                            source_id,
                            &sibling_id,
                            &imp.raw,
                            &ext_id,
                            catalog_root,
                            lock,
                            locked,
                            frozen,
                            cache,
                            res,
                            visiting,
                            versions,
                            depth + 1,
                            diags,
                        )
                    }
                    None => {
                        diags.error(
                            Code::SourceUnavailable,
                            format!(
                                "import `{}` in pulled skill `{ext_id}` (source \
                                 `{source_id}`) does not resolve to a skill in that source",
                                imp.raw
                            ),
                        );
                        None
                    }
                }
            }
        };
        if let Some(rid) = resolved_id {
            // Transitive versioned imports participate in conflict detection too (FR-011):
            // a dep pinned @1 by one external skill and @2 by another must surface, not
            // silently dedup.
            if let Some(version) = &imp.version {
                versions
                    .entry(rid.clone())
                    .or_default()
                    .entry(version.clone())
                    .or_default()
                    .push(ext_id.clone());
            }
            local_targets.insert(alias.clone(), rid.clone());
            deps.insert(rid);
        }
    }

    // Render the external skill's markers as the same followable links local skills get.
    let content = crate::parse::marker::render(&expanded, &|alias: &str| {
        local_targets
            .get(alias)
            .map(|id| format!("[{id}](../{id}/SKILL.md)"))
    });

    // Reference stems become filenames at emit/install. Catalog skills' stems pass
    // through validate; pulled skills bypass that stage, so gate here — a crafted mirror
    // must fail the BUILD with a named diagnostic, not surface as a cryptic install error
    // (defense-in-depth: install re-checks anyway).
    let mut references = Vec::new();
    for r in &ext.references {
        if crate::model::is_safe_segment(&r.stem) {
            references.push(PulledReference {
                stem: r.stem.clone(),
                content: r.content.clone(),
            });
        } else {
            diags.error(
                Code::SchemaInvalid,
                format!(
                    "pulled skill `{ext_id}` (source `{source_id}`) has reference stem \
                     `{}` which is not a safe filename",
                    r.stem
                ),
            );
        }
    }

    res.pulled.insert(
        ext_id.clone(),
        PulledSkill {
            source: source_id.to_string(),
            id: ext_id.clone(),
            description: ext.description.clone(),
            content,
            pulled_by: pulled_by.to_string(),
            includes: ext.includes.clone(),
            imports: ext
                .imports
                .iter()
                .map(|(k, v)| (k.clone(), v.raw.clone()))
                .collect(),
            references,
        },
    );
    res.pulled_deps.insert(ext_id.clone(), deps);
    visiting.remove(&ext_id);
    Some(ext_id)
}

/// Build a map from each skill's canonical directory → skill id (for local-import lookup).
fn skill_dir_index(catalog: &Catalog) -> BTreeMap<PathBuf, String> {
    let mut map = BTreeMap::new();
    for skill in catalog.skills.values() {
        if let Some(dir) = skill.source_path.parent() {
            if let Ok(canon) = std::fs::canonicalize(dir) {
                map.insert(canon, skill.id.clone());
            }
        }
    }
    map
}

fn detect_version_conflicts(
    versions: &BTreeMap<String, BTreeMap<String, Vec<String>>>,
    diags: &mut Diagnostics,
) {
    for (dep_id, by_version) in versions {
        if by_version.len() < 2 {
            continue;
        }
        // Render `code-review required at 1.2 (via admin) and 2.0 (via scm)`.
        let parts: Vec<String> = by_version
            .iter()
            .map(|(v, importers)| format!("{v} (via {})", importers.join(", ")))
            .collect();
        diags.error(
            Code::DepVersionConflict,
            format!("`{dep_id}` required at {}", parts.join(" and ")),
        );
    }
}
