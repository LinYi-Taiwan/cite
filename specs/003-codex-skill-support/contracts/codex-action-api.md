# Contract: Codex Action API

Extends the existing `serve` action layer (`crates/skill-inspector/src/action/`). These are the **only**
Codex writers. Every action is explicit, reversible, and leaves the setup consistent on failure
(FR-016, FR-017). Dispatched from `action/mod.rs` by the target's `agent`/`source_id`.

## Action 1 — Toggle a Codex plugin (primary control)

`POST` set a plugin's `enabled` flag. Implemented in a new `action/codex_config.rs`.

- **Input**: bare plugin name (from a `codex:plugin:<name>` source) + desired `enabled: bool`.
- **Effect**: in `<codex_home>/config.toml`, set `[plugins."<name>@<marketplace>"].enabled = <bool>`,
  recovering the full `"<name>@<marketplace>"` key first.
- **Write rule**: **format-preserving** edit (research §4) — only the `enabled` value changes; comments,
  key order, and all other tables remain byte-identical. The edit MUST be the sole diff.
- **Result on disk**: on next scan, that plugin's skills resolve to `Active` (true) or `DisabledPlugin`
  (false). Plugin files are never moved or deleted (FR-012).
- **Honest scope**: this is **global** (all projects). The UI labels it as such before applying — it is
  not folder-scoped, matching the `DisabledPlugin` semantics already used for Claude.
- **Failure**: config missing / unwritable / plugin key not found → return a clear error, write nothing
  (FR-017). A no-op (already in desired state) succeeds idempotently.

## Action 2 — Disable / re-enable an individual Codex skill

Reuses the existing tiered disable in `action/mod.rs` + `action/quarantine.rs`.

- **Tier-1 (folder-scoped)**: only if a native Codex per-project skill toggle is confirmed (research §3:
  none today). Not implemented in v1.
- **Tier-2 (fallback, v1)**: reversible **quarantine move** of the skill directory, tracked in
  `inspector-state.json`; surfaced as a **global** disable (FR-015). Restore moves it back losslessly
  (FR-013).
- **Built-in guard**: a skill with `built_in = true` (under `.system/`) MAY be quarantine-disabled but
  MUST NOT be offered for destructive **remove** (FR-008); the remove endpoint refuses it with a clear
  reason.

## Action 3 — Remove a Codex skill

Reuses the existing confirmed-remove path (backup-then-delete) unchanged, with the built-in guard above.
Requires explicit confirmation; refused for `built_in` skills.

## Cross-cutting

- No action runs as a side effect of `scan`/view/overlap (FR-016).
- Each action is reversible (plugin re-enable; skill restore from quarantine; remove restores from
  backup) — no data loss on the reverse operation (FR-013, SC-005).
- Errors never leave a half-written `config.toml` (write to temp + atomic rename, or `toml_edit`
  in-memory then single write).
