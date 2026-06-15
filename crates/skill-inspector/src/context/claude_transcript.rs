//! Best-effort, read-only parse of Claude session transcripts (FR-017/018, SC-007).
//!
//! Where present, transcripts live under `<home>/.claude/projects/**/**.jsonl`. The on-disk
//! schema is vendor-controlled and unstable, so we parse conservatively: a JSONL line is
//! recognized only when it explicitly carries a `loaded_skill_keys` array. Anything we
//! cannot positively read yields NOTHING here — the caller then reports `unavailable`. We
//! never infer or fabricate membership.

use std::path::Path;

use crate::model::{ContextAvailability, ContextLoadRecord};

/// Read recognized load records for the Claude agent. Empty ⇒ caller marks `unavailable`.
pub fn read(home: &Path) -> Vec<ContextLoadRecord> {
    let projects = home.join(".claude").join("projects");
    let mut files = Vec::new();
    collect_jsonl(&projects, &mut files);
    files.sort();

    let mut out = Vec::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let basename = file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("transcript")
            .to_string();
        for (idx, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let Some(arr) = value.get("loaded_skill_keys").and_then(|v| v.as_array()) else {
                continue;
            };
            let keys: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            out.push(ContextLoadRecord {
                turn_ref: format!("{basename}#{idx}"),
                availability: ContextAvailability::Present,
                loaded_skill_keys: keys,
            });
        }
    }
    out
}

fn collect_jsonl(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            out.push(path);
        }
    }
}
