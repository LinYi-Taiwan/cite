//! `skills-lock.json` read/write + content hashing (T024).
//!
//! Pins every cross-repo source for reproducible, deterministic builds (FR-008/011/022).
//! `--frozen` treats the lock as read-only and forbids network; `--locked` fails if a
//! build would change the lock (CI integrity).
//!
//! NOTE (research §4): network fetch via `gix` is deferred for the first cut. Sources are
//! **pre-cloned** — each locked source carries a `path` to a local mirror. This keeps
//! builds hermetic and tests deterministic without a network dependency.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::diagnostics::{Code, Diagnostic};

pub const LOCK_FILENAME: &str = "skills-lock.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    pub version: u32,
    #[serde(default)]
    pub sources: BTreeMap<String, LockedSource>,
}

impl Default for Lockfile {
    fn default() -> Self {
        Self {
            version: 1,
            sources: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedSource {
    pub url: String,
    pub commit: String,
    /// `sha256:<hex>` over the mirror's content (see [`hash_dir`]). When present it is
    /// **always verified** at build time — drifted mirror bytes are a hard error. When
    /// absent, the source is unpinned; `--locked` then refuses the build (CI integrity).
    #[serde(
        rename = "contentHash",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub content_hash: Option<String>,
    /// Local mirror path (pre-cloned source), relative to the catalog root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl Lockfile {
    /// Read `skills-lock.json` from the catalog root. A missing lock yields an empty
    /// (version 1) lockfile; a malformed lock is a hard error.
    pub fn read(catalog_root: &Path) -> Result<Self, Diagnostic> {
        let path = catalog_root.join(LOCK_FILENAME);
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| {
                Diagnostic::error(
                    Code::ConfigInvalid,
                    format!("malformed {LOCK_FILENAME}: {e}"),
                )
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(Diagnostic::error(
                Code::ConfigInvalid,
                format!("cannot read {LOCK_FILENAME}: {e}"),
            )),
        }
    }

    pub fn source(&self, id: &str) -> Option<&LockedSource> {
        self.sources.get(id)
    }

    /// Serialize deterministically (sorted keys via `BTreeMap`, trailing newline).
    pub fn to_json(&self) -> String {
        let mut s = serde_json::to_string_pretty(self).expect("lockfile serializes");
        s.push('\n');
        s
    }
}

/// `sha256:<hex>` over a directory's full content: every file under `root`, visited in
/// sorted relative-path order, contributes `<rel-path>\0<bytes>\0` to one running hash.
/// Path separators are normalized to `/` so the digest is platform-stable. This is the
/// value `LockedSource::content_hash` pins and the build verifies (drift detection).
pub fn hash_dir(root: &Path) -> Result<String, Diagnostic> {
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    collect_files(root, &mut files)?;
    let mut rels: Vec<(String, std::path::PathBuf)> = files
        .into_iter()
        .map(|p| {
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            (rel, p)
        })
        .collect();
    rels.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = Sha256::new();
    for (rel, path) in rels {
        let bytes = std::fs::read(&path).map_err(|e| {
            Diagnostic::error(
                Code::ConfigInvalid,
                format!("cannot read {}: {e}", path.display()),
            )
        })?;
        hasher.update(rel.as_bytes());
        hasher.update([0]);
        hasher.update(&bytes);
        hasher.update([0]);
    }
    let digest = hasher.finalize();
    let mut out = String::from("sha256:");
    for b in digest {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
    }
    Ok(out)
}

fn collect_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), Diagnostic> {
    let entries = std::fs::read_dir(dir).map_err(|e| {
        Diagnostic::error(
            Code::ConfigInvalid,
            format!("cannot read {}: {e}", dir.display()),
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| {
            Diagnostic::error(
                Code::ConfigInvalid,
                format!("cannot read {}: {e}", dir.display()),
            )
        })?;
        let path = entry.path();
        // Symlinks are rejected, not followed: following would let a crafted mirror loop
        // the walk (cycle), read outside the mirror (a link to `/`), or alias content so
        // two different trees hash equal. Mirrors are plain file trees by contract.
        let meta = std::fs::symlink_metadata(&path).map_err(|e| {
            Diagnostic::error(
                Code::ConfigInvalid,
                format!("cannot stat {}: {e}", path.display()),
            )
        })?;
        if meta.file_type().is_symlink() {
            return Err(Diagnostic::error(
                Code::ConfigInvalid,
                format!(
                    "mirror contains a symlink `{}`; symlinks are not allowed in hashed \
                     source mirrors",
                    path.display()
                ),
            ));
        }
        if meta.is_dir() {
            collect_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// `sha256:<hex>` over `bytes` — the dedup key + drift signal (FR-010/022).
pub fn content_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::from("sha256:");
    for b in digest {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_is_stable_and_prefixed() {
        let h = content_hash(b"hello");
        assert!(h.starts_with("sha256:"));
        assert_eq!(h, content_hash(b"hello"));
        assert_ne!(h, content_hash(b"world"));
    }

    #[test]
    fn parses_lockfile() {
        let json = r#"{"version":1,"sources":{"s":{"url":"u","commit":"c","contentHash":"sha256:x","path":"sources/s"}}}"#;
        let lock: Lockfile = serde_json::from_str(json).unwrap();
        assert_eq!(lock.source("s").unwrap().path.as_deref(), Some("sources/s"));
    }
}
