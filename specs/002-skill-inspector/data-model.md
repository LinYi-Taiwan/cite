# Phase 1 Data Model: Skill & Context Inspector

Entities derive directly from the spec's Key Entities + Functional Requirements. Field names are
indicative (Rust structs in `crates/skill-inspector/src/model.rs` / `state.rs`); the JSON-facing shapes
are pinned by the contracts in `contracts/`.

---

## Skill  (spec: "Skill"; FR-001..005, FR-019)

One installed skill instance discovered under a source root.

| Field | Type | Notes / Validation |
|---|---|---|
| `id` | string | Directory name `<id>` containing `SKILL.md`. Non-empty. |
| `name` | string? | From frontmatter `name`. May be absent → `metadata_complete=false`. |
| `description` | string? | From frontmatter `description`. Used for overlap (US2). Absent → incomplete. |
| `source_id` | string | FK → `SkillSource.id` it was found under. |
| `agent` | string | Originating agent label (e.g. `claude-code`). Drives US5 labeling. |
| `path` | string | Absolute on-disk location of the skill dir. |
| `content_hash` | string | `sha2` over skill dir contents — duplicate-by-identity + safe-remove backup key. |
| `state` | enum | `active` \| `disabled-in-folder` (Tier-1 `skillOverrides` `off` for the current context) \| `disabled-global` (Tier-2 quarantine). Reflects real on-disk + settings state each scan (FR-005). |
| `metadata_complete` | bool | False when name/description unparseable/missing → flagged, never dropped (FR-003). |
| `labels` | string[] | User-assigned labels/groups (FR-015), from `InspectorState`. |

**Rules**: a Skill is listed exactly once per installed instance (FR-003). `(agent, source_id, id)` is
unique within a scan. Malformed frontmatter ⇒ `metadata_complete=false`, entry still present.

---

## SkillSource  (spec: "Skill Source"; FR-001, FR-019, edge: unreadable source)

An origin scanned for skills.

| Field | Type | Notes |
|---|---|---|
| `id` | string | Stable id, e.g. `claude:user`, `claude:project`, `claude:plugin:<name>`. |
| `agent` | string | Owning agent. |
| `kind` | enum | `user` \| `project` \| `plugin` \| `other-agent`. |
| `root` | string | Absolute root path walked for `<id>/SKILL.md`. |
| `availability` | enum | `readable` \| `missing` \| `unreadable`. Non-readable ⇒ reported, scan continues (edge). |

---

## OverlapCluster  (spec: "Overlap Cluster"; FR-006..010)

A suggested grouping of skills. **Advisory only** — never causes mutation.

| Field | Type | Notes |
|---|---|---|
| `id` | string | Deterministic id from sorted member ids. |
| `kind` | enum | `duplicate-identity` (same id/hash, FR-008) \| `similarity` (lexical, FR-006). |
| `members` | string[] | Skill keys `(agent/source/id)`. ≥2. |
| `reason` | string | Human-readable basis, e.g. `shared terms: code-review, react, lint` or `identical content hash` (FR-007). |
| `score` | number? | Similarity score for `similarity` kind (omitted for identity). |

**Rules**: no singleton clusters; unrelated skills are not forced together (FR-010). When zero clusters,
the export says so explicitly rather than inventing weak ones (FR-010).

---

## ActivationState  (spec: "Activation State"; FR-016)

Per skill, in a given context. Computed from on-disk state — no vendor dependency.

| Field | Type | Notes |
|---|---|---|
| `skill_key` | string | `(agent/source/id)`. |
| `context` | string | Project/cwd the evaluation is for. |
| `eligible` | bool | Skill is in an active root applicable to this context (triggerable). |
| `active` | bool | Currently active in this context — false if quarantined (global) **or** set `off` via this folder's `skillOverrides`. |

---

## ContextLoadRecord  (spec: "Context Load Record"; FR-017/018, SC-007)

What an agent actually loaded in a turn — **best-effort, vendor-gated**.

| Field | Type | Notes |
|---|---|---|
| `turn_ref` | string | Session/turn identifier (e.g. transcript file + index). |
| `availability` | enum | `present` \| `unavailable`. **Must** be `unavailable` when the agent exposes nothing (FR-018). |
| `loaded_skill_keys` | string[] | Skills recorded as loaded — populated only when `present`. |

**Rules**: when `availability=unavailable`, `loaded_skill_keys` is empty and the UI states the data is
unavailable for this agent. The tool never infers/fabricates membership (SC-007).

---

## InspectorState  (the only persisted, tool-owned mutation record; FR-011..015)

Persisted to `inspector-state.json`. Written **only** by `action/`.

| Field | Type | Notes |
|---|---|---|
| `version` | int | Schema version. |
| `quarantine` | QuarantineRecord[] | One per **Tier-2 global** disable (sources with no scoped toggle, e.g. plugin skills). |
| `folder_overrides` | FolderOverrideRecord[] | One per **Tier-1 per-folder** disable the tool wrote, for bookkeeping/undo. |
| `labels` | map<skill_key, string[]> | User labels/groups (FR-015). |

**QuarantineRecord**: `{ skill_key, id, agent, original_root, original_path, quarantine_path,
content_hash, disabled_at }` — enough to restore a disabled skill to its **exact** original location
(FR-011) and to detect drift.

**FolderOverrideRecord**: `{ skill_key, agent, folder, settings_path, previous_value, new_value,
disabled_at }` — records that the tool set `skillOverrides[<name>]` in `<folder>/.claude/settings.local.json`
(FR-023). The settings file is the source of truth; this record is for undo + drift detection (the user
or `/skills` may change the same key). Tier-1 never moves or deletes skill files.

---

## Relationships

```text
SkillSource 1 ──< Skill              (a source yields many skills)
Skill       >──< OverlapCluster      (a skill may be in 0..1 similarity cluster + 0..1 identity cluster)
Skill       1 ──1 ActivationState    (per evaluated context)
Skill       *  ~  ContextLoadRecord  (referenced by key when a turn records loads; may be unavailable)
InspectorState ──< QuarantineRecord     (one per Tier-2 global-disabled Skill)
InspectorState ──< FolderOverrideRecord (one per Tier-1 folder-disabled Skill)  ── labels map → Skill.labels
```

## State transitions (Skill.state)

```text
# Tier-1 (preferred, folder-scoped) — used when the source has a native per-project toggle (Claude skillOverrides)
active ──disable-in-folder (action/, explicit)──▶ disabled-in-folder   # write skillOverrides[name]="off" in <folder>/.claude/settings.local.json; FolderOverrideRecord written; files untouched
disabled-in-folder ──enable (action/, explicit)─▶ active               # remove the key (or restore previous_value); record cleared

# Tier-2 (fallback, global) — used when the source has no scoped toggle (plugin skills, future agents)
active ──disable-global (action/, explicit)──▶ disabled-global         # dir moved to quarantine; QuarantineRecord written
disabled-global ──enable (action/, explicit)─▶ active                  # dir moved back to original_path; record cleared

active ──remove (action/, explicit + confirm)──▶ (gone)                # backup taken, then deleted from active root
```

Scanning/analysis trigger **no** transitions (FR-013, SC-005).
