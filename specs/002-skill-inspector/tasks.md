---
description: "Task list for Skill & Context Inspector implementation"
---

# Tasks: Skill & Context Inspector

**Input**: Design documents from `/specs/002-skill-inspector/`

**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/ ✅ (cli, source-provider, inventory-export, action-api, inspector-state), quickstart.md ✅

**Tests**: Included. The plan's **Testing** section and quickstart's **Automated equivalents** table explicitly name integration test files (`scan_inventory.rs`, `overlap_clusters.rs`, `action_roundtrip.rs`) with `insta` snapshots and fixture trees — so test tasks are part of the design, not optional add-ons.

**Organization**: Tasks are grouped by user story (US1–US5) to enable independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Which user story the task belongs to (US1–US5); Setup/Foundational/Polish carry no story label
- All paths are relative to the repo root `/Users/reid.lin/Desktop/repos/cite`

## Path Conventions

- New crate: `crates/skill-inspector/` (binary + core) in the existing Cargo workspace
- Source: `crates/skill-inspector/src/`, assets: `crates/skill-inspector/assets/`
- Tests + fixtures: `crates/skill-inspector/tests/` (so `cargo test -p skill-inspector` finds them, per quickstart)
- Reused seeds (NOT modified): `crates/skillc-core/src/parse/frontmatter.rs`, `crates/skillc-core/assets/graph.html`, `crates/skillc-core/src/graph_export.rs`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Stand up the new crate inside the existing workspace and pull in the two reused seeds.

- [X] T001 Add `"crates/skill-inspector"` to `[workspace] members` in `Cargo.toml` (root)
- [X] T002 Create `crates/skill-inspector/Cargo.toml`: bin target `skill-inspector`, depend on `skillc-core` (path), and workspace deps `serde`, `serde_json`, `serde_norway`, `sha2`, `clap`, plus `walkdir` and a `tiny_http`-class server crate; add `insta` as dev-dependency (per plan Primary Dependencies)
- [X] T003 [P] Create the source tree skeleton with empty module stubs so the crate compiles: `crates/skill-inspector/src/main.rs`, `cli.rs`, `model.rs`, `state.rs`, `overlap.rs`, `activation.rs`, `export.rs`, `server.rs`, `scan/mod.rs`, `scan/source.rs`, `scan/claude.rs`, `scan/skill_md.rs`, `context/mod.rs`, `context/claude_transcript.rs`, `action/mod.rs`, `action/overrides.rs`, `action/quarantine.rs`
- [X] T004 [P] Seed the UI asset: copy `crates/skillc-core/assets/graph.html` → `crates/skill-inspector/assets/inspector.html` as the starting point for the control-panel UI (extended in later stories)
- [X] T005 [P] Create the test fixture root `crates/skill-inspector/tests/fixtures/multi-source/` mirroring quickstart.md: `home/.claude/skills/{code-review,react-code-review,broken}/SKILL.md`, `home/.claude/plugins/devkit/skills/devkit.typescript.code-review/SKILL.md`, `project/.claude/skills/qa-playwright/SKILL.md`, plus a duplicate `home/.claude/skills/qa-playwright/SKILL.md` (duplicate-identity) and one `broken/SKILL.md` with malformed frontmatter

**Checkpoint**: `cargo build -p skill-inspector` compiles an empty-but-wired crate.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Crate-wide types and infrastructure every user story imports. No story work can begin until this is done.

**⚠️ CRITICAL**: Blocks all user stories.

- [X] T006 [P] Implement all core entities in `crates/skill-inspector/src/model.rs` per data-model.md: `Skill` (id, name?, description?, source_id, agent, path, content_hash, `state` enum {active, disabled-in-folder, disabled-global}, metadata_complete, labels), `SkillSource` (id, agent, kind enum, root, availability enum), `OverlapCluster` (id, kind enum, members, reason, score?), `ActivationState`, `ContextLoadRecord` (availability enum, loaded_skill_keys) — all `serde::Serialize` with stable field names matching `contracts/inventory-export.md`
- [X] T007 [P] Implement `InspectorState` persistence in `crates/skill-inspector/src/state.rs` per `contracts/inspector-state.md`: structs `InspectorState`, `QuarantineRecord`, `FolderOverrideRecord`, `labels` map; read/write `inspector-state.json` in the tool config dir (not inside any skill root); tolerate a missing file (empty state). Exclude timestamps from any snapshot-facing serialization helper
- [X] T008 Define the `SourceProvider` trait + `ScanContext` in `crates/skill-inspector/src/scan/source.rs` per `contracts/source-provider.md` (`agent()`, `sources(&ScanContext) -> Vec<SkillSource>`; `ScanContext{project_root, home}`), plus a provider registry keyed by agent id. Trait contract: read-only, never hard-errors on missing/unreadable root (classify via `availability`)
- [X] T009 [P] Implement the shared skill walker + frontmatter parse in `crates/skill-inspector/src/scan/skill_md.rs`: for a readable root, find immediate child dirs containing `SKILL.md`, parse `name`/`description` by reusing `skillc-core`'s `parse/frontmatter.rs`, compute `content_hash` via `sha2`, set `metadata_complete=false` (never drop) on parse failure (FR-003)
- [X] T010 [P] Define the CLI surface in `crates/skill-inspector/src/cli.rs` per `contracts/cli.md`: clap derive structs for `scan` (`--agent` repeatable, `--project`, `--format json|html`, `--out`) and `serve` (`--agent`, `--project`, `--port`)
- [X] T011 Wire `crates/skill-inspector/src/main.rs` clap dispatch for `scan` / `serve` subcommands (handlers can return "not yet implemented" until their stories land), depends on T010

**Checkpoint**: Foundation ready — entities, state, provider trait, walker, and CLI skeleton compile and are importable. User stories can now begin.

---

## Phase 3: User Story 1 - See every installed skill in one unified view (Priority: P1) 🎯 MVP

**Goal**: A single `scan` produces one consolidated, searchable inventory of every installed skill across sources, each with name, description, source, location, and enabled/disabled state; malformed metadata is flagged, never dropped.

**Independent Test**: `skill-inspector scan --agent claude-code --project tests/fixtures/multi-source/project --format json` lists every fixture skill once with `source_id`/`agent`/`state`/`path`; `broken` carries `metadata_complete:false`; a missing root shows `availability:"missing"` and scan exits 0; `--format html` renders in a browser.

### Tests for User Story 1

> Write FIRST; ensure they FAIL before implementation.

- [X] T012 [P] [US1] Integration test `crates/skill-inspector/tests/scan_inventory.rs`: multi-source scan over the fixture lists every skill once, flags `broken` as incomplete (not dropped), reports a missing root as `missing` with exit 0; assert with `insta` snapshot of the inventory export (SC-002, SC-006, FR-003)

### Implementation for User Story 1

- [X] T013 [US1] Implement the Claude Code provider in `crates/skill-inspector/src/scan/claude.rs` per `contracts/source-provider.md`: discover roots `claude:user` (`~/.claude/skills/`), `claude:project` (`<project_root>/.claude/skills/`), `claude:plugin:<name>` (plugin skill dirs under the Claude config dir — verify the real layout on-machine and mirror it in the fixture); classify each root `readable|missing|unreadable`. Register it in the T008 registry
- [X] T014 [US1] Implement scan orchestration in `crates/skill-inspector/src/scan/mod.rs`: discover sources via registry → walk each readable root (T009) → assemble the `Skill` list, merging `InspectorState` (T007) to set `state` and `labels`; enforce one entry per installed instance with unique `(agent, source_id, id)`, deterministic ordering by key (FR-001..005)
- [X] T015 [US1] Implement inventory export in `crates/skill-inspector/src/export.rs` per `contracts/inventory-export.md`: emit `version`, `generated_for`, `sources[]`, `skills[]`, and the derived `nodes`/`edges` projection (graph.html compatibility); deterministic stable ordering so it is snapshot-testable
- [X] T016 [US1] Implement the `scan` handler in `main.rs`: build inventory (T014), serialize via T015; `--format json` → inventory JSON to `--out`/stdout; `--format html` → self-contained snapshot reusing `inspector.html`; exit non-zero only on tool failure, never on malformed skills (FR-003, FR-013/021/SC-005: never writes)
- [X] T017 [US1] Extend `crates/skill-inspector/assets/inspector.html` to render the inventory list (name, description, source, location, state badge) from the export, with a search/filter box that filters by name + description keyword in < 1 s (FR-004)

**Checkpoint**: US1 fully functional — unified inventory visible via JSON, HTML snapshot, and searchable UI. MVP deliverable.

---

## Phase 4: User Story 2 - Surface overlaps, duplicates, and dead skills (Priority: P1)

**Goal**: The inspector groups skills that serve the same purpose into overlap clusters with a human-readable reason, distinguishes duplicate-by-identity from similarity, never force-groups unrelated skills, and plainly reports when there are no overlaps — all advisory, mutating nothing.

**Independent Test**: On the fixture, `clusters[]` contains a `similarity` cluster (`code-review` + `react-code-review` + `devkit.typescript.code-review`) with reason + score and a `duplicate-identity` cluster for the two `qa-playwright`; `qa-playwright` is not force-merged into the code-review cluster; an overlap-free fixture yields `clusters:[]` with a non-null `clusters_empty_reason`; re-scan diff shows no on-disk change.

### Tests for User Story 2

- [X] T018 [P] [US2] Integration test `crates/skill-inspector/tests/overlap_clusters.rs`: assert the similarity cluster, the duplicate-identity cluster, no false grouping of `qa-playwright`, and the empty-case `clusters_empty_reason`; assert scanning mutates nothing (re-scan → identical fixture tree); `insta` snapshot of `clusters[]` (SC-003, FR-008/010, SC-005)

### Implementation for User Story 2

- [X] T019 [US2] Implement overlap detection in `crates/skill-inspector/src/overlap.rs` per research.md §3: (a) duplicate-by-identity via same `id` across roots and/or identical `content_hash` (FR-008); (b) similarity clustering over `name + description` using token-set/n-gram Jaccard + TF-IDF cosine above a tuned threshold, each cluster carrying a human-readable `reason` and `score`; no singletons; deterministic cluster ids from sorted member keys (FR-006/007/010)
- [X] T020 [US2] Wire clusters into `export.rs` (T015): populate `clusters[]`, set `clusters_empty_reason` to a non-null string when empty, and add `kind:"overlap"` edges to the `edges[]` projection (FR-009/010)
- [X] T021 [US2] Extend `inspector.html` with cluster panels: show each cluster's members, kind (similarity vs duplicate-identity), reason/score, and per-member distinguishing detail (source, description); display a clear "no overlaps found" state; label the view as advisory suggestions (FR-007/009)

**Checkpoint**: US1 + US2 both work independently — inventory plus advisory overlap clusters.

---

## Phase 5: User Story 3 - Act on the findings: disable, remove, or organize safely (Priority: P2)

**Goal**: From the UI, the user can folder-scoped-disable (Tier-1 `skillOverrides`), global-disable (Tier-2 quarantine, honestly labeled), re-enable losslessly, remove with explicit confirmation + backup, and label skills — all only through explicit action API calls, never as a side effect of scanning.

**Independent Test**: Via `serve`, `/api/disable` (folder scope) toggles a skill off for one folder and re-`scan` shows `disabled-in-folder`; `/api/enable` restores it byte-identical; an unconfirmed `/api/remove` changes nothing, a confirmed one leaves a restorable backup; a write to a read-only root returns `ok:false` and leaves the tree untouched.

### Tests for User Story 3

- [X] T022 [P] [US3] Integration test `crates/skill-inspector/tests/action_roundtrip.rs`: Tier-1 folder disable → enable round-trips losslessly and writes/clears a `FolderOverrideRecord` preserving other keys in `settings.local.json`; Tier-2 quarantine disable → enable restores the dir to its exact original path byte-identical; unconfirmed remove is a no-op, confirmed remove backs up then deletes; a read-only root yields `ok:false` with the tree unchanged; assert `scan` never mutates (FR-011/012/014, SC-005)

### Implementation for User Story 3

- [X] T023 [US3] Implement Tier-1 per-folder disable in `crates/skill-inspector/src/action/overrides.rs` per `contracts/action-api.md`: read/write `skillOverrides["<name>"]="off"` in `<folder>/.claude/settings.local.json` (create file/key if absent, preserve other keys), append/clear a `FolderOverrideRecord` (T007); never move/delete skill files; reversible via key removal or `previous_value` restore (FR-011/023)
- [X] T024 [US3] Implement Tier-2 global quarantine in `crates/skill-inspector/src/action/quarantine.rs`: move the skill dir to the tool quarantine, append a `QuarantineRecord` with everything needed to restore to the exact original path; enable moves it back; detect hash drift on restore (warn, don't clobber) (FR-011, edge: drift)
- [X] T025 [US3] Implement the action dispatcher in `crates/skill-inspector/src/action/mod.rs`: `disable` routes by the source's strategy — folder scope → T023, global or no-scoped-toggle source → T024 with `effective_scope:"global"`; reject `scope="folder"` on a source with no per-folder mechanism (`folder_scope_unsupported` + `would_be_scope:"global"`); `remove` requires `confirm:true` (else `confirmation_required`), backs up to tool trash then deletes; `label` sets labels in `InspectorState`; all-or-nothing atomicity with rollback on failure, prior-state-preserving errors (FR-012/013/014/015/024)
- [X] T026 [US3] Implement the `serve` embedded HTTP server in `crates/skill-inspector/src/server.rs` per `contracts/cli.md` + `contracts/action-api.md`: bind loopback only; serve `inspector.html` + initial inventory export (same as `scan --format json`); expose `POST /api/{disable,enable,remove,label}` dispatching to `action/` (T025); return enough for the UI to update without a full rescan
- [X] T027 [US3] Wire the `serve` handler in `main.rs` to start the server (T026) on `--port` (auto if unset), depends on T011
- [X] T028 [US3] Extend `inspector.html` with action controls: per-skill disable (folder vs global, with an explicit confirmation/warning when effective scope is global per FR-024), enable, remove (with explicit confirmation dialog per FR-012), and label/group editing; reflect new state from the action response and surface action failures clearly (FR-014)

**Checkpoint**: US1 + US2 + US3 all independently functional — view, analyze, and safely act.

---

## Phase 6: User Story 4 - Inspect activation and what the agent actually loaded (Priority: P3)

**Goal**: Show which skills are eligible/triggerable and which are active in a given context (computed from disk), and surface what an agent recorded as loaded where available — explicitly marking runtime context data `unavailable` rather than fabricating it.

**Independent Test**: `scan` output's `activation[]` marks non-quarantined/non-folder-disabled skills `eligible/active` and a disabled one not; `context_loads[]` shows `availability:"present"` with loaded keys for a fixture carrying a transcript, and `availability:"unavailable"` with empty `loaded_skill_keys` where no record exists.

### Tests for User Story 4

- [X] T029 [P] [US4] Extend `crates/skill-inspector/tests/scan_inventory.rs` (context fixture): assert `activation[]` eligibility/active reflects disk state, and that a no-record agent yields `availability:"unavailable"` with empty `loaded_skill_keys` (never fabricated) (FR-016/018, SC-007)

### Implementation for User Story 4

- [X] T030 [P] [US4] Implement activation computation in `crates/skill-inspector/src/activation.rs`: per skill in the scan context, set `eligible` (in an active root applicable to the context) and `active` (false if quarantined-global or set `off` via this folder's `skillOverrides`) — purely from on-disk state (FR-016)
- [X] T031 [P] [US4] Implement best-effort context-load reading in `crates/skill-inspector/src/context/claude_transcript.rs` + `context/mod.rs`: read-only parse of Claude session transcripts (`~/.claude/projects/**/**.jsonl`) where present → `ContextLoadRecord{availability:"present", loaded_skill_keys}`; when absent/unparseable → `availability:"unavailable"` with empty keys, never inferred (FR-017/018, SC-007)
- [X] T032 [US4] Wire `activation[]` and `context_loads[]` into `export.rs` (T015) and surface them in `inspector.html`: show eligible/active state and the loaded-context record, with an explicit "runtime context unavailable for this agent" message when unavailable (FR-016/017/018, FR-022)

**Checkpoint**: US1–US4 independently functional.

---

## Phase 7: User Story 5 - Span multiple agents from one view (Priority: P3)

**Goal**: Include skills from more than one agent in the same inventory, each labeled by originating agent, and allow overlap clusters to span agents.

**Independent Test**: With a second-agent fixture and `--agent <other>`, `skills[]` includes both agents correctly labeled, and a cluster can span agents.

### Tests for User Story 5

- [X] T033 [P] [US5] Integration test in `crates/skill-inspector/tests/scan_inventory.rs` (multi-agent fixture): both agents appear labeled by `agent`, and a similarity/duplicate cluster spans agents (FR-019/020)

### Implementation for User Story 5

- [X] T034 [P] [US5] Add a second `SourceProvider` implementation (e.g. a stub/other-agent provider) registered in the T008 registry, with a fixture root under `tests/fixtures/`, proving the trait+registry extends without rewrite (FR-019)
- [X] T035 [US5] Confirm `scan/mod.rs` (T014) honors repeated `--agent` flags to scan multiple providers and that `overlap.rs` (T019) clusters across agents (cross-agent members); label each skill by `agent` in inventory + UI (FR-019/020)

**Checkpoint**: All user stories independently functional.

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Improvements spanning multiple stories.

- [X] T036 [P] Run the full quickstart.md validation end-to-end (US1–US5 sections) against the fixtures and confirm each "Expect" holds
- [X] T037 [P] Verify SC-001 performance: cold `scan` produces full inventory in < 30 s for an O(10²)-skill tree (add a generated large fixture if needed)
- [X] T038 [P] Confirm determinism (identical roots → identical inventory + clusters) and run `cargo insta review` to bless the inventory/cluster/export snapshots
- [X] T039 [P] Update repo docs (`README` / `CLAUDE.md`) to introduce the `skill-inspector` crate, its `scan`/`serve` commands, and the safe-by-default boundary
- [X] T040 `cargo fmt` + `cargo clippy -p skill-inspector` clean; final cleanup of the module stubs and dead `not-yet-implemented` handlers

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately
- **Foundational (Phase 2)**: Depends on Setup — BLOCKS all user stories
- **User Stories (Phase 3–7)**: All depend on Foundational
  - US1 (P1) and US2 (P1) are the MVP core; US2's clusters extend US1's export
  - US3 (P2), US4 (P3), US5 (P3) build on the US1 scan/export foundation
- **Polish (Phase 8)**: Depends on all targeted stories complete

### User Story Dependencies

- **US1 (P1)**: After Foundational — no dependency on other stories
- **US2 (P1)**: After Foundational — consumes US1's `Skill` list + `export.rs`; independently testable via its own snapshot
- **US3 (P2)**: After Foundational — needs the inventory (US1) to act on; the only writer (`action/` + `serve`)
- **US4 (P3)**: After Foundational — reads US1 scan state; activation/context are additive export keys
- **US5 (P3)**: After Foundational — additive provider; lightly touches US1 scan loop + US2 clustering

### Within Each User Story

- Tests written and FAILING before implementation
- Entities/models (Phase 2) before services; services before endpoints/UI; core before integration
- Story complete before moving to next priority

### Parallel Opportunities

- Setup: T003, T004, T005 in parallel (after T001/T002)
- Foundational: T006, T007, T009, T010 in parallel; T008 then T011 follow
- US1: T012 (test) alongside; T013/T014/T015 are sequential (provider → scan → export), T016/T017 follow
- US4: T030 and T031 in parallel (different modules), then T032
- Polish: T036, T037, T038, T039 in parallel
- With capacity, once Foundational is done US1/US2 (one pair) and US3/US4/US5 can be split across developers — each story is independently testable

---

## Parallel Example: Foundational (Phase 2)

```bash
# After T001/T002, launch the independent foundational modules together:
Task: "Implement core entities in crates/skill-inspector/src/model.rs"
Task: "Implement InspectorState persistence in crates/skill-inspector/src/state.rs"
Task: "Implement shared walker + frontmatter parse in crates/skill-inspector/src/scan/skill_md.rs"
Task: "Define CLI surface in crates/skill-inspector/src/cli.rs"
```

---

## Implementation Strategy

### MVP First (US1 + US2)

1. Phase 1: Setup → Phase 2: Foundational
2. Phase 3: US1 (unified inventory) → **STOP and VALIDATE** (the smallest pain-relieving win)
3. Phase 4: US2 (overlap clusters) → the "aha" — both P1, together the MVP
4. Demo: a searchable, de-duplicated inventory with advisory overlap clusters

### Incremental Delivery

1. Setup + Foundational → foundation ready
2. US1 → test → demo (inventory)
3. US2 → test → demo (overlaps) — MVP complete
4. US3 → test → demo (safe actions)
5. US4 → test → demo (activation/context transparency)
6. US5 → test → demo (cross-agent)
7. Each story adds value without breaking previous ones

### Parallel Team Strategy

1. Whole team completes Setup + Foundational
2. Then split: Dev A → US1+US2 (P1 core), Dev B → US3 (actions), Dev C → US4+US5
3. Stories integrate through the shared `export.rs` + `model.rs` contracts

---

## Notes

- [P] = different files, no incomplete-task dependency
- Read (`scan`/`overlap`/`activation`/`context`) and write (`action/` via `serve`) are separated at the module boundary — safe-by-default is structural (plan Structure Decision)
- Reused seeds (`skillc-core` frontmatter parser, `graph.html`) are NOT modified — `skill-inspector` depends on `skillc-core`, copies `graph.html` → `inspector.html`
- Never fabricate vendor-gated data; mark `unavailable` (FR-018, SC-007)
- Verify the real Claude plugin skill-root layout on-machine (T013) and mirror it in the fixture before relying on it
- Commit after each task or logical group; stop at any checkpoint to validate a story independently
