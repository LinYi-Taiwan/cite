//! Diagnostics (T006): error-code enum, severity, the accumulator, and exit-code mapping.
//!
//! The compiler fails loudly and names the rule (FR-026); warnings are surfaced, never
//! silent (FR-027). Exit codes follow `cli.md`: `0` success, `1` hard failure, `2` usage.

use std::fmt;

/// Stable error/warning codes. Rendered as `error[<code>]` / `warning[<code>]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    // --- hard failures (exit 1) ---
    /// `@include` of a non-existent block (FR-003).
    IncludeMissing,
    /// `@include` / dependency cycle (FR-004).
    Cycle,
    /// `{{Alias}}` with no matching import (FR-007).
    MarkerUndefined,
    /// Unresolvable import / cross-repo source unavailable (FR-009).
    SourceUnavailable,
    /// Same dependency at conflicting versions (FR-011).
    DepVersionConflict,
    /// Entry mounts a non-existent skill (FR-014a).
    EntryMissing,
    /// `per-target` reference missing for an applicable target (FR-017).
    ReferenceMissing,
    /// A required frontmatter field is missing / malformed (FR-016).
    SchemaInvalid,
    // --- usage errors (exit 2) ---
    /// Unknown target/agent vs config registry, or missing config (FR-019).
    ConfigInvalid,
    // --- warnings (exit 0) ---
    /// A block no skill includes (FR-005).
    BlockUnused,
    /// An import declared but never referenced (FR-012).
    ImportUnused,
    /// A unit reached by no entry and no import (FR-014b).
    SkillOrphan,
    /// A target with an empty entry (Edge Cases).
    EntryEmpty,
    /// A `references/<stem>.md` whose stem is neither target nor shared (FR-018).
    ReferenceStray,
    /// A `sharedReferences` stem equal to a registered target name (Edge Cases).
    ReferenceSharedCollision,
    /// A catalog entry skipped during discovery (e.g. a symlink — never followed).
    CatalogSkipped,
}

impl Code {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::IncludeMissing => "include/missing",
            Self::Cycle => "cycle",
            Self::MarkerUndefined => "marker/undefined",
            Self::SourceUnavailable => "source/unavailable",
            Self::DepVersionConflict => "dep/version-conflict",
            Self::EntryMissing => "entry/missing",
            Self::ReferenceMissing => "reference/missing",
            Self::SchemaInvalid => "schema/invalid",
            Self::ConfigInvalid => "config/invalid",
            Self::BlockUnused => "block/unused",
            Self::ImportUnused => "import/unused",
            Self::SkillOrphan => "skill/orphan",
            Self::EntryEmpty => "entry/empty",
            Self::ReferenceStray => "reference/stray",
            Self::ReferenceSharedCollision => "reference/shared-collision",
            Self::CatalogSkipped => "catalog/skipped",
        }
    }

    /// Usage errors map to exit code 2; everything else that is an error → 1.
    pub fn is_usage(self) -> bool {
        matches!(self, Self::ConfigInvalid)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// A single diagnostic: a coded, actionable message naming the offending unit/rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: Code,
    pub message: String,
}

impl Diagnostic {
    pub fn error(code: Code, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code,
            message: message.into(),
        }
    }

    pub fn warning(code: Code, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{label}[{}]: {}", self.code.as_str(), self.message)
    }
}

/// Accumulates diagnostics across pipeline stages so the compiler can report *all*
/// errors rather than only the first (a compiler should not stop at error #1).
#[derive(Debug, Clone, Default)]
pub struct Diagnostics {
    items: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, d: Diagnostic) {
        self.items.push(d);
    }

    pub fn error(&mut self, code: Code, message: impl Into<String>) {
        self.push(Diagnostic::error(code, message));
    }

    pub fn warning(&mut self, code: Code, message: impl Into<String>) {
        self.push(Diagnostic::warning(code, message));
    }

    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.severity == Severity::Error)
    }

    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.items.iter().filter(|d| d.severity == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.items
            .iter()
            .filter(|d| d.severity == Severity::Warning)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.items.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn extend(&mut self, other: Diagnostics) {
        self.items.extend(other.items);
    }

    /// Drop exact-duplicate diagnostics, keeping first occurrence order. Multi-target
    /// passes (`check`, cartesian builds) re-run catalog-wide analyses per target; the
    /// repeated identical lines add noise, not information. The key includes severity so
    /// a same-text error is never swallowed by an earlier warning.
    pub fn dedup(&mut self) {
        let mut seen: std::collections::BTreeSet<(bool, &'static str, String)> =
            std::collections::BTreeSet::new();
        self.items.retain(|d| {
            seen.insert((
                d.severity == Severity::Error,
                d.code.as_str(),
                d.message.clone(),
            ))
        });
    }
}

/// Process exit codes per `cli.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ExitCode {
    Success = 0,
    HardFailure = 1,
    Usage = 2,
}

impl ExitCode {
    pub fn code(self) -> i32 {
        self as i32
    }
}
