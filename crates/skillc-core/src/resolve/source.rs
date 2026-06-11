//! Cross-repo source resolution (T023).
//!
//! Resolves a `Repo(<id>)` import to a unit inside that source. Per research §4, the first
//! cut uses **pre-cloned** sources: the lockfile pins each source to a local mirror `path`,
//! which we parse as a sub-catalog. Real network fetch via `gix` slots in here later, behind
//! the same interface (populate the mirror, then parse it) — keeping the rest of the
//! pipeline unaware of how the bytes arrived, and keeping CI hermetic/deterministic.

use std::collections::BTreeMap;
use std::path::Path;

use crate::diagnostics::{Code, Diagnostic};
use crate::model::Catalog;
use crate::parse;
use crate::resolve::lock::Lockfile;

/// A resolved source: its parsed sub-catalog, cached by source id so repeated imports of
/// the same source parse it once (and dedup deterministically).
#[derive(Default)]
pub struct SourceCache {
    catalogs: BTreeMap<String, Catalog>,
}

impl SourceCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get (parsing+caching on first use) the sub-catalog for `source_id`.
    ///
    /// `frozen` is satisfied by construction here: this build never performs network fetch,
    /// and a source MUST already be pinned in the lock with a usable mirror `path`, else
    /// `error[source/unavailable]` (FR-009). The flag is threaded so that when live `gix`
    /// fetch is added, the network-vs-lock-only decision plugs in at this seam.
    ///
    /// `locked` (CI integrity): every used source must carry a `contentHash` pin. A pin,
    /// when present, is verified against the mirror's actual bytes regardless of the flag.
    pub fn get(
        &mut self,
        catalog_root: &Path,
        lock: &Lockfile,
        source_id: &str,
        locked: bool,
        frozen: bool,
    ) -> Result<&Catalog, Diagnostic> {
        let _ = frozen; // see doc comment: hermetic-by-construction in this cut.
        if !self.catalogs.contains_key(source_id) {
            let cat = load_source_catalog(catalog_root, lock, source_id, locked)?;
            self.catalogs.insert(source_id.to_string(), cat);
        }
        Ok(self.catalogs.get(source_id).expect("just inserted"))
    }
}

fn load_source_catalog(
    catalog_root: &Path,
    lock: &Lockfile,
    source_id: &str,
    locked: bool,
) -> Result<Catalog, Diagnostic> {
    let locked_src = lock.source(source_id).ok_or_else(|| {
        Diagnostic::error(
            Code::SourceUnavailable,
            format!("import source `{source_id}` is not pinned in skills-lock.json"),
        )
    })?;

    let mirror_rel = locked_src.path.as_deref().ok_or_else(|| {
        Diagnostic::error(
            Code::SourceUnavailable,
            format!(
                "source `{source_id}` has no local mirror path in skills-lock.json (network \
                 fetch is not available in this build)"
            ),
        )
    })?;

    let mirror = catalog_root.join(mirror_rel);
    if !mirror.is_dir() {
        return Err(Diagnostic::error(
            Code::SourceUnavailable,
            format!(
                "source `{source_id}` mirror `{}` not found",
                mirror.display()
            ),
        ));
    }

    // The lockfile `path` is "relative to the catalog root" by contract; enforce it. A
    // crafted lock with `path: ../../secret` would otherwise parse (and exfiltrate) files
    // outside the catalog. See the security review.
    let (root_canon, mirror_canon) = (
        std::fs::canonicalize(catalog_root).unwrap_or_else(|_| catalog_root.to_path_buf()),
        std::fs::canonicalize(&mirror).unwrap_or_else(|_| mirror.clone()),
    );
    if !mirror_canon.starts_with(&root_canon) {
        return Err(Diagnostic::error(
            Code::SourceUnavailable,
            format!("source `{source_id}` mirror path escapes the catalog root"),
        ));
    }

    // Pin integrity: a present contentHash is ALWAYS verified against the mirror's actual
    // bytes — a pin that isn't checked is decorative. `--locked` additionally refuses
    // unpinned sources, so CI cannot silently build from drifted/unreviewed mirror content.
    match &locked_src.content_hash {
        Some(pinned) => {
            let actual = crate::resolve::lock::hash_dir(&mirror)?;
            if &actual != pinned {
                return Err(Diagnostic::error(
                    Code::SourceUnavailable,
                    format!(
                        "source `{source_id}` mirror content does not match the pinned \
                         contentHash in skills-lock.json (pinned {pinned}, actual {actual}); \
                         re-pin after reviewing the mirror change"
                    ),
                ));
            }
        }
        None if locked => {
            let actual = crate::resolve::lock::hash_dir(&mirror)?;
            return Err(Diagnostic::error(
                Code::SourceUnavailable,
                format!(
                    "--locked: source `{source_id}` has no contentHash pin in \
                     skills-lock.json; pin it (current mirror content is {actual})"
                ),
            ));
        }
        None => {}
    }

    let mut diags = crate::diagnostics::Diagnostics::new();
    let cat = parse::parse_catalog(&mirror, &mut diags);
    // Run the same @include graph checks on the mirror so a cyclic mirror catalog fails with
    // a diagnostic instead of overflowing the stack during assemble. See the backend review.
    crate::graph::analyze_includes(&cat, &mut diags);
    if diags.has_errors() {
        let joined = diags
            .errors()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(Diagnostic::error(
            Code::SourceUnavailable,
            format!("source `{source_id}` is invalid: {joined}"),
        ));
    }
    Ok(cat)
}
