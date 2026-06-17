# Quickstart: Codex Agent Support

Runnable validation that the Codex tab + provider work end-to-end. Assumes the `002-skill-inspector`
binary builds. Contracts: [codex-source-provider](./contracts/codex-source-provider.md),
[codex-action-api](./contracts/codex-action-api.md), [agent-tab-ui](./contracts/agent-tab-ui.md).

## Prerequisites

- Rust 1.83+, the existing workspace builds (`cargo build -p skill-inspector`).
- Codex CLI installed (verified target: 0.137.0) with at least one **enabled** plugin, OR the test
  fixture `tests/fixtures/codex_home/` (synthetic `$CODEX_HOME`) for deterministic runs.

## Scenario A — Codex skills appear, attributed and grouped (US1 + US2)

```bash
cargo run -p skill-inspector -- scan --format json > /tmp/inv.json
```

Expected:
- Entries with `"agent":"codex"` exist (FR-003), each carrying a `source_id` and `state` (SC-003).
- Source kinds present for Codex: `codex:user` (global), one `codex:plugin:<name>` per installed plugin
  (FR-005). `codex:project` appears only if a Codex project skills dir exists (research §2).
- A skill from an `enabled=false` plugin has `"state":"disabled-plugin"` (FR-007).
- A skill under `.system/` is flagged built-in (FR-008).

Validate against ground truth:
```bash
# every installed Codex plugin's skills are discovered (SC-002)
find "${CODEX_HOME:-$HOME/.codex}/plugins/cache" -name SKILL.md | wc -l
jq '[.skills[] | select(.agent=="codex" and (.source_id|startswith("codex:plugin:")))] | length' /tmp/inv.json
```

## Scenario B — Switch to the Codex tab (US1)

```bash
cargo run -p skill-inspector -- serve --port 7777
# open http://localhost:7777
```

Expected (agent-tab-ui contract):
- An agent switcher shows `claude-code` and `codex` tabs (FR-001).
- Clicking **Codex** filters the inventory to only `agent="codex"` skills (FR-002); the Codex view shows
  global + per-plugin (+ project if present) groups, no hidden plugin section.
- Type a search term, switch to Claude and back — the search term is preserved (FR-004).

## Scenario C — Toggle a Codex plugin off and back (US3)

From the Codex tab, switch a plugin **off** at its group header. The UI states the change is **global /
all projects** before applying (FR-015), then calls Action 1.

Verify the write is surgical (research §4):
```bash
cp "${CODEX_HOME:-$HOME/.codex}/config.toml" /tmp/cfg.before
# (perform the toggle in the UI)
diff /tmp/cfg.before "${CODEX_HOME:-$HOME/.codex}/config.toml"
# Expected: the ONLY changed line is `enabled = true` → `enabled = false`
# under the targeted [plugins."<name>@<marketplace>"] table (SC-004, FR-012).
```

Re-scan: that plugin's skills now report `"state":"disabled-plugin"`. Toggle back on → `"state":"active"`,
config.toml returns to its prior content (reversible, no data loss). Plugin files were never moved (FR-012).

## Scenario D — Safety: scanning never mutates (SC-005)

```bash
cp "${CODEX_HOME:-$HOME/.codex}/config.toml" /tmp/cfg.s
cargo run -p skill-inspector -- scan --format json > /dev/null
diff /tmp/cfg.s "${CODEX_HOME:-$HOME/.codex}/config.toml"   # expect: no differences
```

## Automated coverage (maps to tasks)

Integration tests live FLAT at `crates/skill-inspector/tests/*.rs` (each a `[[test]]` block in
`Cargo.toml`), not under `tests/integration/`.

| Test | Asserts |
|---|---|
| `tests/scan_codex.rs` | source kinds discovered + `agent="codex"`; plugin-off → `DisabledPlugin`; `.system` → built-in; missing Codex home degrades gracefully (insta snapshot) |
| `tests/codex_plugin_toggle.rs` | enabled-flag write flips state and round-trips; `config.toml` otherwise byte-stable; idempotent no-op |
| `tests/overlap_clusters.rs` (extended) | a cross-agent cluster includes a Codex member; a same-id-in-two-Codex-sources case is `DuplicateIdentity` (FR-011) |
| `tests/action_roundtrip.rs` (extended) | Codex skill quarantine disable→restore lossless; built-in remove refused (FR-008) |
