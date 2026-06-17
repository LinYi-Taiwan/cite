---
description: "Task list for Codex Agent Support (Per-Agent Tabs)"
---

# Tasks: Codex Agent Support (Per-Agent Tabs)

**Input**: Design documents from `/specs/003-codex-skill-support/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/ (all present)

**Tests**: INCLUDED — the plan's Testing section and quickstart.md "Automated coverage" table
explicitly request `scan_codex.rs`, `codex_plugin_toggle.rs`, and extensions to the existing
overlap/action harnesses.

**Organization**: Tasks are grouped by user story. This feature is additive on top of
`002-skill-inspector`; it extends the existing `skill-inspector` crate (no new crate).

## Real-layout notes (verified against the repo)

- Integration tests live FLAT at `crates/skill-inspector/tests/*.rs` and are each registered as a
  `[[test]]` block in `crates/skill-inspector/Cargo.toml` — NOT under `tests/integration/` as the prose
  docs sketch. New test files MUST add a `[[test]]` entry.
- Fixtures live at `crates/skill-inspector/tests/fixtures/`.
- `quarantine` (Tier-2) disable in `action/mod.rs` is already agent-generic, so a Codex per-skill
  global disable mostly works today; US3's real work is the plugin toggle + the built-in guard.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 / US2 / US3 (maps to spec.md user stories)

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Bring in the one new dependency and the synthetic Codex fixture both used across stories.

- [X] T001 Add a format-preserving TOML editor dependency: add `toml_edit = "0.22"` to `[workspace.dependencies]` in `Cargo.toml` and reference it as `toml_edit.workspace = true` in `crates/skill-inspector/Cargo.toml` (research §4 — needed only for the Codex `config.toml` write in US3; declare it now so the crate compiles once US3 lands)
- [X] T002 [P] Create the synthetic `$CODEX_HOME` fixture tree under `crates/skill-inspector/tests/fixtures/codex_home/`: `skills/<id>/SKILL.md` (a user skill), `skills/.system/<id>/SKILL.md` (a built-in), `config.toml` with one `[plugins."<a>@<mkt>"].enabled = true` and one `[plugins."<b>@<mkt>"].enabled = false` plus surrounding comments/ordering to prove format preservation, and `plugins/cache/<mkt>/<a>/<ver>/skills/<id>/SKILL.md` (≥2 entries) plus the same for `<b>` (the disabled plugin)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The single cross-cutting model change every story touches.

**⚠️ CRITICAL**: Complete before any user story — the Codex provider (US1) sets this flag, US2 surfaces it, US3 guards on it.

- [X] T003 Add `built_in: bool` to `Skill` in `crates/skill-inspector/src/model.rs` with `#[serde(skip_serializing_if = "is_false", default)]` (a small `fn is_false(&bool) -> bool` helper) so existing snapshots/exports for other agents stay byte-identical (data-model.md §"New attribute"); thread it through the `Skill` construction site in `crates/skill-inspector/src/scan/mod.rs` (default `false`) and the quarantine-merge `Skill` there

**Checkpoint**: Model carries the built-in flag — Codex provider work can begin.

---

## Phase 3: User Story 1 - Switch the view to Codex via a tab (Priority: P1) 🎯 MVP

**Goal**: A first-class Codex agent + an agent-switcher tab that scopes the whole view to Codex.

**Independent Test**: On a machine (or fixture) with Codex global skills, open the inspector, click the Codex tab, and confirm only Codex's skills show, labeled Codex, with the Claude view one click away and each tab keeping its own search.

- [X] T004 [P] [US1] Create `crates/skill-inspector/src/scan/codex.rs` — `CodexProvider` implementing `SourceProvider` (`scan/source.rs:20`): `agent()` → `"codex"` (const `AGENT`); resolve Codex home = `$CODEX_HOME` else `ctx.home.join(".codex")`; emit the global/user source (`id="codex:user"`, `kind=User`, `root=<codex_home>/skills`) via `classify_availability`; emit the conditional project source (`id="codex:project"`, `kind=Project`) ONLY if a Codex project skills root is confirmed for `ctx` (else omit — no fabricated path, research §2); include the source-id helpers `is_plugin_source(&str)` and `supports_folder_scope(&str)->false` (codex has no native folder toggle, research §3)
- [X] T005 [US1] Flag built-in skills + wire Codex into the scan: in `crates/skill-inspector/src/scan/mod.rs` set `built_in=true` for any Codex skill whose source root path is under `skills/.system/` (FR-008); register `CodexProvider` in `Registry::with_defaults` (`crates/skill-inspector/src/scan/source.rs:52`); add `"codex"` to the default scan agents in `crates/skill-inspector/src/cli.rs:62` (`agents_or_default`) so Codex is scanned without an explicit `--agent` (depends on T004)
- [X] T006 [US1] Add the agent-switcher tabs to `crates/skill-inspector/assets/inspector.html`: render one tab per distinct `agent` in the loaded export in stable order, Claude (`claude-code`) active by default when present (FR-002), and scope the inventory + per-source-kind groups + overlap view to `skill.agent === activeAgent`; a tab appears only when that agent has ≥1 skill (FR-001 — installed-but-skill-less Codex contributes no tab)
- [X] T007 [US1] Per-agent independent view state in `crates/skill-inspector/assets/inspector.html`: each tab's search text + active filters apply only to that agent, are retained on switch-away, and restored on return; a query entered on one tab is never carried onto another (FR-004) (depends on T006, same file)
- [X] T008 [P] [US1] New integration test `crates/skill-inspector/tests/scan_codex.rs` (+ register a `[[test]]` block in `crates/skill-inspector/Cargo.toml`): point `ScanContext` at `tests/fixtures/codex_home`; assert Codex global skills are discovered with `agent="codex"`, a `.system` skill carries `built_in=true`, and a missing Codex home degrades gracefully (empty-but-valid, no error) — `insta` snapshot

**Checkpoint**: Codex appears as its own tab showing its global skills; the tab switcher scopes and preserves state.

---

## Phase 4: User Story 2 - See Codex's skills by its real source kinds (Priority: P1)

**Goal**: Codex view organized by global / per-plugin / project, honest about plugin-off and built-in.

**Independent Test**: On a fixture with Codex global skills and ≥1 installed plugin, open the Codex tab and confirm a global group and a per-plugin group each appear with the right skills; a disabled plugin's skills show as disabled-because-plugin-off; a `.system` skill shows as built-in.

- [X] T009 [US2] Extend `CodexProvider` in `crates/skill-inspector/src/scan/codex.rs` with plugin discovery: parse `[plugins."<name>@<marketplace>"]` tables in `<codex_home>/config.toml`, emit one source per installed plugin (`id="codex:plugin:<name>"` bare name, `kind=Plugin`, `root=<codex_home>/plugins/cache/<mkt>/<name>/<version>/skills`, resolving the installed `<version>` dir); robust to a missing/garbled `config.toml` → zero plugin sources (mirror `claude.rs::discover_plugins`); add a `resolve_plugin_full_key(codex_home, bare_name) -> Option<String>` analogue (claude.rs:121) that recovers `"<name>@<marketplace>"` and reads its `enabled` flag, deterministic (sorted-key first-match) on a name collision (depends on T004)
- [X] T010 [US2] Extend the plugin-enabled state mapping in `crates/skill-inspector/src/scan/mod.rs` to Codex: a `codex:plugin:<name>` skill resolves to `SkillState::DisabledPlugin` when its config `enabled=false`, `Active` otherwise — re-read from `config.toml` every scan, never cached (FR-007, FR-010); keep the existing `claude:plugin:` path unchanged (depends on T009)
- [X] T011 [US2] Render the Codex source-kind groups in `crates/skill-inspector/assets/inspector.html`: global, one group per plugin, project (only when a `codex:project` source exists) — no "plugin section hidden" case; a `DisabledPlugin` skill renders disabled-because-plugin-off (reuse the Claude plugin-off treatment ~inspector.html:304); `.system` skills marked built-in; a metadata-incomplete skill shows an indicator rather than being dropped (FR-006, FR-008, FR-009) (depends on T006)
- [X] T012 [US2] Extend `crates/skill-inspector/tests/overlap_clusters.rs` to include a Codex member in a cross-agent cluster, asserting Codex skills participate in cross-agent overlap and that a same-id-in-two-Codex-sources case is `DuplicateIdentity`, distinct from similarity (FR-011)
- [X] T013 [P] [US2] Extend `crates/skill-inspector/tests/scan_codex.rs`: assert per-plugin attribution (each plugin's skills under its `codex:plugin:<name>` source), a plugin with `enabled=false` yields `state="disabled-plugin"`, and SC-002 discovery (every fixture plugin skill is found) — extend the `insta` snapshot (depends on T009, T010)

**Checkpoint**: The Codex tab is an honest mirror — correct source kinds, plugin-off and built-in surfaced, overlap spans agents.

---

## Phase 5: User Story 3 - Disable / re-enable Codex skills, scoped correctly (Priority: P2)

**Goal**: From the Codex tab, toggle a plugin off/on (format-preserving config write) and reversibly disable a skill, with honest global-scope messaging and a built-in remove guard.

**Independent Test**: From the Codex tab, switch an installed plugin off and confirm its skills show disabled and `config.toml` changed only the one `enabled` line; switch back on and confirm it returns. Disable an individual skill (quarantine) and confirm it deactivates reversibly, stated as global; confirm a built-in's destructive remove is refused.

- [X] T014 [US3] Create `crates/skill-inspector/src/action/codex_config.rs`: format-preserving `toml_edit` write that flips only `[plugins."<name>@<marketplace>"].enabled` in `<codex_home>/config.toml`, recovering the full key from the bare name first; in-memory edit then a single atomic write (temp + rename); idempotent no-op when already in the desired state; clear error (write nothing) on missing/unwritable config or unknown plugin key (FR-012, FR-016, FR-017; research §4)
- [X] T015 [US3] Dispatch the Codex plugin toggle in `crates/skill-inspector/src/action/mod.rs`: in `plugin_set_enabled`, when the target plugin belongs to the `codex` agent route to `action::codex_config` (writes `~/.codex/config.toml`) instead of the Claude `settings.json` path; leave the Claude branch unchanged; return the honest "global / all projects" + restart-note result (depends on T014)
- [X] T016 [US3] Built-in remove guard in `crates/skill-inspector/src/action/mod.rs` `remove()`: refuse a `remove` when the resolved skill has `built_in=true`, returning a clear reason (FR-008); a quarantine `disable` of a built-in is still allowed (per data-model state transitions)
- [X] T017 [US3] Codex control UI in `crates/skill-inspector/assets/inspector.html`: each Codex plugin group header carries an on/off control calling Action 1, stating the change is global / all projects BEFORE applying (FR-015) and showing a Codex restart / `/skills` refresh note after success; the per-skill disable on the Codex tab states it is a global quarantine disable; the built-in destructive-remove control is disabled (depends on T011)
- [X] T018 [P] [US3] New integration test `crates/skill-inspector/tests/codex_plugin_toggle.rs` (+ register a `[[test]]` block in `crates/skill-inspector/Cargo.toml`): against a temp copy of the fixture `config.toml`, assert the `enabled` flip changes plugin skill state, the file is otherwise byte-stable (only the one value changed), and a no-op toggle succeeds idempotently
- [X] T019 [P] [US3] Extend `crates/skill-inspector/tests/action_roundtrip.rs`: a Codex skill quarantine disable→restore is lossless, and a `remove` on a `built_in` Codex skill is refused (FR-008)

**Checkpoint**: All three stories function — view, source fidelity, and reversible control — independently testable.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T020 [P] Run quickstart.md Scenarios A–D (scan attribution/grouping, tab switch + preserved search, surgical plugin toggle diff, scan-never-mutates) against the fixture and, if Codex 0.137.0 is installed, the live machine
- [X] T021 [P] `cargo fmt` + `cargo clippy -p skill-inspector` clean; reconcile any prose in plan.md/quickstart.md that points at `tests/integration/` with the real flat `tests/` layout
- [X] T022 Full `cargo test -p skill-inspector` green and `cargo insta review` accepting the new/updated `scan_codex` / `overlap_clusters` snapshots

---

## Dependencies & Execution Order

### Phase dependencies

- **Setup (P1)** → no deps; start immediately.
- **Foundational (P2: T003)** → after Setup; BLOCKS all stories.
- **US1 (P3)** → after Foundational. The MVP.
- **US2 (P4)** → after US1 (extends `codex.rs` from T004 and the inspector tab from T006).
- **US3 (P5)** → after US1 (and US2's grouping for the toggle UI); the config writer (T014) only needs T001 + Foundational.
- **Polish (P6)** → after the stories you intend to ship.

### Key intra/cross-story edges

- T005 → T004 (registers/flags the provider it creates)
- T007 → T006 · T011 → T006 · T017 → T011 (all `inspector.html`, sequential)
- T009 → T004 · T010 → T009 · T013 → T009,T010
- T015 → T014
- US2 and US3 both build on US1's `codex.rs` + tab; US3's `codex_config.rs` (T014) is independent of US2.

### Parallel opportunities

- T001 ∥ T002 (Setup, different files).
- Within US1: T004 ∥ T008 (provider vs its test scaffold); T006/T007 serialize (same file).
- Within US2: T013 runs alongside T011/T012 once T009/T010 land.
- Within US3: T018 ∥ T019 (different test files); T014 can start as soon as Setup+Foundational are done, in parallel with US1/US2 UI work.

---

## Parallel Example: User Story 1

```bash
# After Foundational (T003) is done, start the provider and its test scaffold together:
Task: "T004 Create CodexProvider in crates/skill-inspector/src/scan/codex.rs"
Task: "T008 New integration test crates/skill-inspector/tests/scan_codex.rs"
# Then T005 (wire/register) → T006 (tabs) → T007 (per-tab state).
```

---

## Implementation Strategy

### MVP first (US1 only)

1. Setup (T001–T002) → Foundational (T003).
2. US1 (T004–T008): Codex provider for global skills + the agent-switcher tab.
3. **STOP and VALIDATE**: open the inspector, click the Codex tab, confirm Codex-only view with preserved per-tab search (quickstart Scenario B). This alone delivers the user's explicit ask.

### Incremental delivery

1. US1 → MVP (the tab + Codex presence).
2. US2 → honest source kinds (plugins, plugin-off, built-in, overlap).
3. US3 → reversible control (plugin toggle, per-skill disable, built-in guard).
4. Each phase is independently testable and adds value without breaking the prior.

---

## Notes

- Safe-by-default is structural: only files under `src/action/` write. Discovery (`scan/codex.rs`) and the tab UI never mutate (FR-016, FR-018).
- Reuse over new shapes: no new `SourceKind`/`SkillState` — Codex maps onto `User`/`Project`/`Plugin` + `DisabledPlugin`, already defined (data-model.md).
- The Codex `config.toml` write MUST be format-preserving; assert byte-stability in T018 (research §4).
- Commit after each task or logical group; verify new tests fail before implementing the behavior they cover.
