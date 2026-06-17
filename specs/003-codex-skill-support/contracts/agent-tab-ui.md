# Contract: Agent Tab UI

A **client-side** addition to `crates/skill-inspector/assets/inspector.html`. No server endpoint, no
export schema change — it filters the already-loaded inventory by the existing `agent` field
(research §5; `agent` present at model.rs:33, 83).

## Behavior

- **Tabs**: render one tab per distinct `agent` in the loaded export, in stable order (e.g.
  `claude-code`, `codex`). Each tab shows a human label and is visibly the active/inactive state
  (FR-001). The active agent is unambiguous (FR-002).
- **Scoping**: selecting a tab filters the inventory, the per-source-kind groups, and the overlap/graph
  view to `skill.agent === activeAgent` (FR-002). Exactly one agent's view is shown at a time.
- **Codex view composition**: inside the Codex tab, reuse the existing per-source-kind grouping —
  **global**, one group **per plugin**, and **project** (only when a `codex:project` source exists). There
  is no "plugin section hidden" special case (Codex has plugins). Built-in (`.system`) skills are marked
  as built-in and their destructive-remove control is disabled (FR-008).
- **Plugin-off rendering**: a Codex skill in `DisabledPlugin` state renders as
  disabled-because-plugin-off (reuse the existing Claude `not enabled` plugin treatment, see
  inspector.html:304–310) — shown, not hidden, not active (FR-007).
- **Non-destructive switching**: `search` text and active filters are preserved across tab switches
  (FR-004). Switching tabs performs no write and triggers no rescan.
- **Empty / single-agent states**: if a tab's agent has zero skills, show an empty state naming where the
  tool looked (FR edge cases); if only one agent is detected, the switcher still labels the active agent
  rather than vanishing.

## Plugin toggle control (Codex)

Inside the Codex tab, each plugin group header carries an on/off control that calls **Action 1**
(codex-action-api.md). Before applying, the UI states the change is **global / all projects** (matching
`DisabledPlugin` semantics and the existing Claude plugin-toggle copy at inspector.html:404). After a
successful write, the UI prompts that a Codex restart / `/skills` refresh may be needed for a running
session to pick it up (parallel to the Claude `/reload-plugins` note).

## Out of scope for this contract

- The cross-agent overlap **graph** continues to operate on the full set for cluster computation; only the
  rendered/highlighted view is agent-scoped. Cross-agent clusters remain discoverable (FR-011) — e.g. via
  the existing overlap panel — even though the inventory list is filtered to one agent.
