//! The compiler's in-memory model (T005): the `Unit` sum type and supporting structs.
//!
//! Per `data-model.md`, the catalog is a directed graph of [`Unit`]s. A unit's *kind*
//! (not its import site) decides what it becomes in the artifact: a [`Skill`] is a
//! top-level bundle member, a [`Block`] is inlined content, a [`Reference`] lands under
//! its host skill.

use std::collections::BTreeMap;
use std::path::PathBuf;

pub type SkillId = String;
pub type BlockId = String;
pub type Alias = String;
pub type TargetName = String;
pub type SourceId = String;

/// The central node kind. See `data-model.md` → Unit.
#[derive(Debug, Clone)]
pub enum Unit {
    Skill(Skill),
    Block(Block),
    Reference(Reference),
}

/// `referenceMode` frontmatter field (FR-017).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReferenceMode {
    PerTarget,
    /// skill-schema.md: default when the field is omitted.
    #[default]
    Optional,
    None,
}

impl ReferenceMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "per-target" => Some(Self::PerTarget),
            "optional" => Some(Self::Optional),
            "none" => Some(Self::None),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::PerTarget => "per-target",
            Self::Optional => "optional",
            Self::None => "none",
        }
    }
}

/// An authored unit (`SKILL.md` + optional references / `@include`s / imports).
///
/// Enters a bundle by being **mounted in a target entry** or **pulled as a dependency**.
#[derive(Debug, Clone)]
pub struct Skill {
    pub id: SkillId,
    pub name: String,
    pub description: String,
    pub reference_mode: ReferenceMode,
    /// `appliesTo: all` opt-in auto-mount on every target (FR-014c).
    pub applies_to_all: bool,
    /// Alias → import target (possibly cross-repo). FR-006.
    pub imports: BTreeMap<Alias, ImportRef>,
    /// `@include <block>` directives in body order (FR-001). Populated in T014.
    pub includes: Vec<BlockId>,
    /// `{{Alias}}` occurrences in body order; each MUST resolve to an `imports` key
    /// (FR-007). Populated in T021. May repeat.
    pub markers: Vec<Alias>,
    /// Files under the skill's `references/`.
    pub references: Vec<Reference>,
    /// Source body markdown, pre-assembly.
    pub body: String,
    /// Path to the `SKILL.md` file, for diagnostics.
    pub source_path: PathBuf,
}

/// A reusable content fragment authored once, **inlined** into skills via `@include`
/// (change-once-sync-everywhere — the defining differentiator).
#[derive(Debug, Clone)]
pub struct Block {
    pub id: BlockId,
    /// Inlined verbatim into each includer (FR-001).
    pub content: String,
    /// Blocks may include blocks (graph edge; cycle-checked, FR-004). Populated in T014.
    pub includes: Vec<BlockId>,
    pub source_path: PathBuf,
}

/// A `references/<stem>.md` belonging to a skill.
#[derive(Debug, Clone)]
pub struct Reference {
    pub stem: String,
    pub host_skill: SkillId,
    pub path: PathBuf,
    pub content: String,
}

/// Derived classification of a reference against the config registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceKind {
    /// `<stem>` is a registered target name (FR-015).
    PerTarget,
    /// `<stem>` ∈ config `sharedReferences`.
    Shared,
    /// neither → reported (FR-018).
    Stray,
}

/// A frontmatter `Alias → ImportRef` binding referenced in the body via `{{Alias}}`.
#[derive(Debug, Clone)]
pub struct ImportRef {
    pub source: Source,
    /// Path within the source (or the local path for `Local`).
    pub subpath: String,
    /// Optional version for repo sources; conflict → hard fail (FR-011).
    pub version: Option<String>,
    /// Original spelling, preserved for diagnostics and the manifest.
    pub raw: String,
}

/// Where a unit resolves from, including external repos (FR-008).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A path relative to the importing skill / catalog.
    Local,
    /// A cross-repo source keyed by logical id (`<sourceId>:<subpath>`).
    Repo(SourceId),
}

/// The parsed catalog: all discovered units. References are attached to their host skill.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub skills: BTreeMap<SkillId, Skill>,
    pub blocks: BTreeMap<BlockId, Block>,
}

impl Catalog {
    pub fn new() -> Self {
        Self::default()
    }
}

/// True if `s` is safe to use as a **single filesystem path component**: non-empty, not a
/// `.`/`..` traversal token, no path separators or NUL, and not absolute.
///
/// User-controlled strings (skill ids, reference stems, target/agent names) are joined into
/// output/install paths; without this guard a crafted `id: ../../foo` (or an absolute
/// `id: /etc/...`) escapes the intended directory on write. See the security review.
pub fn is_safe_segment(s: &str) -> bool {
    if s.is_empty() || s == "." || s == ".." {
        return false;
    }
    if s.contains('/') || s.contains('\\') || s.contains('\0') {
        return false;
    }
    // Exactly one `Normal` path component (rejects RootDir/ParentDir/CurDir/Prefix).
    let mut comps = std::path::Path::new(s).components();
    matches!(comps.next(), Some(std::path::Component::Normal(_))) && comps.next().is_none()
}

/// True if `s` is a valid Agent Skills `name` (https://agentskills.io/specification): 1–64
/// chars, ASCII lowercase letters / digits / hyphens only, not starting or ending with a
/// hyphen, and no consecutive hyphens.
///
/// The emitted `SKILL.md` `name` is the skill `id` (and the skill's directory name), and the
/// agent loader validates that field — so an `id` that fails here cannot become a loadable
/// skill. ASCII-only (not "unicode lowercase") is the conservative, cross-platform-safe
/// reading: the value is also a directory name on disk.
pub fn is_valid_skill_name(s: &str) -> bool {
    if s.starts_with('-') || s.ends_with('-') || s.contains("--") {
        return false;
    }
    if !s
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return false;
    }
    // Charset is now guaranteed ASCII, so byte length == char count.
    (1..=64).contains(&s.len())
}

#[cfg(test)]
mod tests {
    use super::{is_safe_segment, is_valid_skill_name};

    #[test]
    fn skill_name_accepts_valid_slugs() {
        for ok in [
            "a",
            "git",
            "frontend-coding",
            "code-review",
            "v1",
            "a1b2",
            &"x".repeat(64),
        ] {
            assert!(
                is_valid_skill_name(ok),
                "{ok:?} should be a valid skill name"
            );
        }
    }

    #[test]
    fn skill_name_rejects_non_slugs() {
        for bad in [
            "",
            &"x".repeat(65),  // too long
            "Frontend",       // uppercase
            "code_review",    // underscore
            "a.b",            // dot
            "-lead",          // leading hyphen
            "trail-",         // trailing hyphen
            "double--hyphen", // consecutive hyphens
            "has space",      // space
            "café",           // non-ascii
            "前端",           // CJK
        ] {
            assert!(!is_valid_skill_name(bad), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn rejects_traversal_and_absolute() {
        for bad in [
            "",
            ".",
            "..",
            "../x",
            "a/b",
            "a\\b",
            "/etc/passwd",
            "x/..",
            "a\0b",
        ] {
            assert!(!is_safe_segment(bad), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn accepts_slugs() {
        for ok in [
            "frontend-coding",
            "git",
            "code_review",
            "admin",
            "a.b",
            "v1.2",
        ] {
            assert!(is_safe_segment(ok), "{ok:?} should be accepted");
        }
    }
}
