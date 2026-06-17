# Phase 1 Data Model: Codex Agent Support

This feature **adds no new core entity**. It reuses the `002-skill-inspector` model
(`crates/skill-inspector/src/model.rs`) and only (a) populates it from a new Codex source and (b) adds a
small built-in flag and a client-side view model for the agent tab. Reuse is the headline: Codex maps
onto the same shapes Claude already uses.

## Reused entities (no change needed)

| Entity | Field used for Codex | Source of truth |
|---|---|---|
| `Skill` | `agent = "codex"`; `source_id` per kind; `state` incl. `DisabledPlugin`; `key()` = `codex/<source_id>/<id>` | model.rs:24, 43 |
| `SkillSource` | `agent = "codex"`; `kind ∈ {User, Plugin, Project}`; `root`; `availability` | model.rs:80 |
| `SourceKind` | `User` (global), `Plugin` (per installed plugin), `Project` (conditional) — **already exist** | model.rs:63 |
| `SkillState` | `Active`, `DisabledPlugin` (plugin `enabled=false`), `DisabledGlobal` (Tier-2 quarantine), `DisabledInFolder` (only if a native toggle is ever found) — **already exist** | model.rs:10 |
| `OverlapCluster` | unchanged; Codex skills participate as members (cross-agent) | model.rs:97 |

**Why no new `SourceKind`/`SkillState`**: the original "Codex needs new kinds" intuition came from the
wrong premise. Once Codex is recognized to have plugins, its kinds are exactly `User`/`Plugin`/`Project`
and its plugin-off state is exactly `DisabledPlugin` — all already defined for Claude.

## Source id scheme (Codex)

Mirrors the Claude scheme so keys stay uniform (`agent/source_id/id`):

| Kind | `source_id` | Example key |
|---|---|---|
| global/user | `codex:user` | `codex/codex:user/skill-creator` |
| plugin | `codex:plugin:<plugin>` (bare plugin name, pre-`@`) | `codex/codex:plugin:self/git` |
| project | `codex:project` (only emitted if a project skills root is confirmed) | `codex/codex:project/foo` |

`split_key` (model.rs:51) already handles `source_id` values containing `:` (e.g. `codex:plugin:self`),
so no parser change is required.

## New attribute: built-in flag

Codex ships built-in `.system` skills (research §2). To satisfy FR-008 (don't offer them for destructive
remove; mark them built-in), add one optional, defaulted-false flag:

- On `Skill` (and/or carried via `SkillSource`): `built_in: bool` — true for skills discovered under the
  `.system/` subtree of the Codex global root. Serialized only when true
  (`skip_serializing_if = "is_false"`) so existing snapshots/exports for other agents are unaffected.

State transitions for a Codex skill (no transition is automatic — each is an explicit user action):

```text
Active ──disable plugin (config enabled=false)──▶ DisabledPlugin ──enable plugin──▶ Active
Active ──disable skill (no native toggle)───────▶ DisabledGlobal (Tier-2 quarantine) ──restore──▶ Active
built_in skill: Active ⇄ DisabledPlugin/DisabledGlobal allowed; REMOVE refused (FR-008)
```

## View model: Agent tab (client-side only)

Lives in `inspector.html`; not persisted, not exported. Derived each render from the loaded inventory.

| Field | Meaning |
|---|---|
| `agents[]` | distinct `agent` values present in the export (e.g. `claude-code`, `codex`), each with a display label and detected/empty state (FR-001/003) |
| `activeAgent` | the currently selected tab; the inventory + clusters are filtered to `skill.agent === activeAgent` (FR-002) |
| `preservedView` | `{ search, filters }` retained across tab switches so switching is non-destructive (FR-004) |

The per-source-kind grouping that already exists renders **inside** the active agent tab. For `codex`,
the groups are global, one per plugin, and project (when present); a plugin group whose plugin is
`enabled=false` renders its skills as `DisabledPlugin` (FR-007).

## Validation rules (from requirements)

- A Codex plugin's displayed enabled/disabled state MUST equal its `enabled` flag in `config.toml`
  on each scan (FR-010, SC-004) — re-read every scan, never cached.
- Every Codex `Skill` MUST carry `agent="codex"`, a `source_id`, and a `state` (SC-003).
- A skill discovered under `.system/` MUST have `built_in=true` (FR-008).
- Discovery MUST not hard-fail on a missing/unreadable Codex root — classify via `Availability`
  (reuses `classify_availability`, source.rs:30).
