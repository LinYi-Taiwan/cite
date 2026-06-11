---
description: "Task list for Skill Compiler implementation"
---

# Tasks: Skill Compiler

**Input**: Design documents from `/specs/001-skill-compiler/`

**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/ ✅ (cli, framework-config, skill-schema, artifact-layout), quickstart.md ✅

**Tests**: INCLUDED. The plan mandates test-first on the resolution/validation core and `insta` golden/snapshot tests with per-scenario fixtures (plan.md Constitution Check + Testing). Each user story carries a fixture + integration test.

**Organization**: Tasks grouped by user story (priorities from spec.md). Stories are sequenced P1→P1→P1→P2→P2; because this is a compiler pipeline, later stages depend on earlier ones — real cross-story dependencies are called out in the Dependencies section (the pipeline layers; stories are not fully independent, but each is independently testable via fixtures once its predecessors exist).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1–US5 (user-story phases only)
- Paths are relative to the framework repo root (the Rust workspace the implementation creates, per plan.md Project Structure)

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Cargo workspace + toolchain + test scaffolding

- [X] T001 Create Cargo workspace per plan.md: root `Cargo.toml` (workspace) + `crates/skillc/` (binary) + `crates/skillc-core/` (library), with `src/main.rs`, `src/cli.rs`, `src/lib.rs` stubs
- [X] T002 Add dependencies in `crates/skillc-core/Cargo.toml` (`serde`, `serde_norway`, `petgraph`, `pulldown-cmark`, `gix`, `sha2`, `thiserror`+`miette`/`ariadne`) and `crates/skillc/Cargo.toml` (`clap` derive); add `insta` as dev-dependency
- [X] T003 [P] Configure `rustfmt.toml` + clippy lints (deny warnings in CI), and an edition/MSRV pin (Rust 1.83, 2021 edition)
- [X] T004 [P] Create `tests/fixtures/` and `tests/integration/` directory scaffolding with a README describing the one-fixture-per-scenario convention

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core types, parsing, config, and pipeline/CLI skeleton that every user story builds on

**⚠️ CRITICAL**: No user-story work can begin until this phase is complete

- [X] T005 [P] Define the `Unit` sum type (`Skill | Block | Reference`) plus `Skill`/`Block`/`Reference`/`Alias`/`ImportRef`/`Source` structs per data-model.md in `crates/skillc-core/src/model.rs`
- [X] T006 [P] Define `Diagnostic` types: error-code enum (`include/missing`, `cycle`, `marker/undefined`, `dep/version-conflict`, `entry/missing`, `reference/missing`, etc.), severity, and exit-code mapping (0/1/2 per cli.md) in `crates/skillc-core/src/diagnostics.rs`
- [X] T007 Implement YAML frontmatter parsing (id/name/description/referenceMode/appliesTo/imports) with `serde_norway` in `crates/skillc-core/src/parse/frontmatter.rs`
- [X] T008 Implement `FrameworkConfig` load + types (`targets`, `agents`, `sharedReferences`, per-target `entry.mounts`, `defaultAgent`) per framework-config.md in `crates/skillc-core/src/config.rs`
- [X] T009 Implement catalog discovery + parse orchestration (walk catalog dir → build `Catalog` of `Unit`s with raw includes/markers/imports/references) as the `parse` stage in `crates/skillc-core/src/parse/mod.rs`
- [X] T010 Define the pipeline orchestration skeleton `parse→resolve→validate→shake→assemble→emit→install` as stage functions threading `Result<_, Diagnostic>` in `crates/skillc-core/src/lib.rs`
- [X] T011 Implement CLI skeleton: `clap` arg structs for `build`/`install` (all flags from cli.md) + dispatch into the lib pipeline + exit-code mapping, in `crates/skillc/src/cli.rs` and `crates/skillc/src/main.rs`

**Checkpoint**: Workspace compiles; `skillc build`/`install` parse args and invoke empty pipeline stages

---

## Phase 3: User Story 1 - Reusable blocks: change once, sync everywhere (Priority: P1) 🎯 MVP

**Goal**: A block authored once is `@include`d by many skills; editing the block re-syncs all includers on recompile, with no includer edits.

**Independent Test**: A block included by two skills; change the block's content, recompile — both skills' artifacts reflect the new content, neither includer file hand-edited. Missing block → fail; `@include` cycle → fail; unused block → warning.

### Tests for User Story 1

- [X] T012 [P] [US1] Create fixtures in `tests/fixtures/us1-reuse/` (block `pr-rules` included by skills `frontend-coding` + `git`; plus a missing-block fixture and an `@include`-cycle fixture)
- [X] T013 [P] [US1] Integration test in `tests/integration/us1_reuse.rs`: assert both includers inline current block content, change-once-sync, `error[include/missing]`, `error[cycle]`, and `warning[block/unused]`

### Implementation for User Story 1

- [X] T014 [US1] Parse `@include <block-id>` line directives from skill/block bodies in `crates/skillc-core/src/parse/include.rs`
- [X] T015 [US1] Parse standalone block files into `Block` units (content + nested `includes`) — extend `crates/skillc-core/src/parse/mod.rs`
- [X] T016 [US1] Build the `@include` dependency graph and cycle detection (`petgraph` `toposort`) in `crates/skillc-core/src/graph.rs`
- [X] T017 [US1] Assemble: inline block content verbatim into each includer (diamond = inline per-includer, blocks are not deduped nodes) in `crates/skillc-core/src/assemble.rs`
- [X] T018 [US1] Emit `error[include/missing]` (naming skill + block, FR-003), `error[cycle]` (naming the cycle, FR-004), and `warning[block/unused]` (FR-005) via the diagnostics layer

**Checkpoint**: A catalog using `@include` compiles with blocks inlined; missing/cyclic includes fail; unused blocks warn

---

## Phase 4: User Story 2 - Unambiguous references to imported units (Priority: P1)

**Goal**: `imports: { Alias: path }` + body `{{Alias}}` resolve to exactly that unit (incl. cross-repo); undefined alias fails; cross-repo skill deps are pulled in, deduped, version-conflict-checked.

**Independent Test**: Import `code-review` as `{{CodeReview}}`, reference it — resolves and the dep is bundled. `{{Foo}}` with no import → fail naming the alias. Diamond dep → included once. Conflicting versions → fail.

### Tests for User Story 2

- [X] T019 [P] [US2] Create fixtures in `tests/fixtures/us2-markers-deps/` (alias resolves; undefined `{{Foo}}`; cross-repo import via `<sourceId>:<subpath>`; diamond dedup; version-conflict; declared-but-unused import)
- [X] T020 [P] [US2] Integration test in `tests/integration/us2_markers_deps.rs`: assert resolution, `error[marker/undefined]`, cross-repo pull, single dedup copy, `error[dep/version-conflict]`, and `warning[import/unused]`

### Implementation for User Story 2

- [X] T021 [US2] Parse `{{Alias}}` markers and the `\{{` literal escape from bodies in `crates/skillc-core/src/parse/marker.rs`
- [X] T022 [US2] Resolve every marker to a declared import; undefined → `error[marker/undefined]` (FR-007); declared-but-unreferenced → `warning[import/unused]` (FR-012); in `crates/skillc-core/src/resolve/imports.rs`
- [X] T023 [US2] Cross-repo source resolution for `Repo(<id>)` imports in `crates/skillc-core/src/resolve/source.rs` — implemented over lockfile-pinned **pre-cloned local mirrors** (hermetic by construction); live network fetch via `gix` is deferred per research.md §4 and slots in behind the same seam (`gix` is intentionally not yet a dependency)
- [X] T024 [US2] `skills-lock.json` read/write + sha256 content hashing, with `--locked`/`--frozen` enforcement per framework-config.md, in `crates/skillc-core/src/resolve/lock.rs`
- [X] T025 [US2] Dependency dedup by stable id (FR-010) + conflicting-version detection → `error[dep/version-conflict]` (FR-011) in `crates/skillc-core/src/graph.rs` / resolve
- [X] T026 [US2] Assemble: land a pulled skill dependency as a top-level skill in the bundle (deduped), extending `crates/skillc-core/src/assemble.rs`

**Checkpoint**: `{{Alias}}` resolves, cross-repo deps pull + dedupe, undefined/conflict fail loudly

---

## Phase 5: User Story 3 - Per-target bundles via entry-point pull (Priority: P1)

**Goal**: Each target's `entry.mounts` seeds a closure traversal (pull); unreached units are orphans; per-target references are pruned to the compiled target.

**Independent Test**: Mount `frontend-coding` in `scm` entry but not `admin` — present for `scm`, absent for `admin`. A unit reached by no entry/import → orphan warning. Mounting a missing skill → fail. `references/<target>.md` retained only for that target.

### Tests for User Story 3

- [X] T027 [P] [US3] Create fixtures in `tests/fixtures/us3-entry-pull/` (entry mounts present in one target / absent in another; missing-mount; orphan skill; per-target + shared + stray reference files)
- [X] T028 [P] [US3] Integration test in `tests/integration/us3_entry_pull.rs`: assert membership-by-target, `warning[skill/orphan]`, `error[entry/missing]`, and reference pruning (foreign-target ref count = 0)

### Implementation for User Story 3

- [X] T029 [US3] Validate `entry.mounts[]` against the catalog; non-existent mount → `error[entry/missing]` naming entry + skill (FR-014a), in `crates/skillc-core/src/config.rs` / validate
- [X] T030 [US3] Shake: compute reachable subgraph from a target's entry closure and emit `warning[skill/orphan]` for units reached by no entry/import (FR-013/014b) in `crates/skillc-core/src/shake.rs`
- [X] T031 [US3] Parse + classify `references/<stem>.md` as per-target / shared / stray against config in `crates/skillc-core/src/model.rs` + parse
- [X] T032 [US3] Assemble: retain `references/<target>.md` + shared references, drop foreign-target reference files (FR-015) in `crates/skillc-core/src/assemble.rs`
- [X] T033 [US3] Support `appliesTo: all` auto-mount-on-all-targets opt-in (FR-014c) in shake/config

**Checkpoint**: Membership is driven by entry pull; orphans warn; missing mounts fail; references pruned per target

---

## Phase 6: User Story 4 - Conformance enforced by the compiler (Priority: P2)

**Goal**: The skill schema is enforced; any violation (missing frontmatter, bad reference naming, unresolved `@include`/`{{}}`, missing required per-target reference, stray file) fails the build naming the skill + rule.

**Independent Test**: Remove a required frontmatter field (or break an `@include`) → build fails naming the rule; fix → passes. `per-target` skill missing `references/<applicable-target>.md` → fail; non-applicable target → no assertion. Stray reference filename → reported.

### Tests for User Story 4

- [X] T034 [P] [US4] Create fixtures in `tests/fixtures/us4-conformance/` (missing required frontmatter; `per-target` skill missing the target's reference; stray/typo'd reference filename; shared-vs-target stem collision)
- [X] T035 [P] [US4] Integration test in `tests/integration/us4_conformance.rs`: assert each violation fails naming field/rule, the per-target assertion does NOT fire for non-applicable targets, and stray + collision are reported

### Implementation for User Story 4

- [X] T036 [US4] Implement the skill schema rules (required frontmatter fields, `referenceMode` enum, import-form well-formedness) in `crates/skillc-core/src/schema.rs`
- [X] T037 [US4] Implement the `validate` stage: run schema rules, assert every `@include`/`{{}}` resolves, report stray files, in `crates/skillc-core/src/validate.rs`
- [X] T038 [US4] `referenceMode: per-target` missing `references/<T>.md` for an applicable target → `error[reference/missing]`; assertion must not fire for non-applicable targets (FR-017)
- [X] T039 [US4] Stray/unrecognized reference filename report (FR-018) + shared-reference-vs-target-name collision warning (Edge Cases)

**Checkpoint**: Non-conforming catalogs fail at compile time with named rules; conformant ones pass

---

## Phase 7: User Story 5 - Inspectable, deterministic build artifact (Priority: P2)

**Goal**: `build` emits an independent, inspectable, byte-deterministic artifact per `(target, agent)` with a `manifest.json`; `install` is a separate idempotent step; multiple agent formats are selectable.

**Independent Test**: Compile twice from identical inputs → byte-identical artifacts; inspect `manifest.json` without installing; run `install` separately and re-run → identical destination.

### Tests for User Story 5

- [X] T040 [P] [US5] Integration test in `tests/integration/us5_artifact.rs`: byte-identical double `build --locked --frozen` (`insta` golden), `manifest.json` shape (skills with `reason`/`includes`/`imports`/`references` + warnings), empty-entry-valid artifact, and `install` idempotency

### Implementation for User Story 5

- [X] T041 [US5] Implement deterministic artifact emit (sorted keys, no timestamps/wall-clock/fs-order) writing the `dist/<target>/<agent>/skills/...` tree per artifact-layout.md in `crates/skillc-core/src/emit.rs`
- [X] T042 [US5] Generate `manifest.json` (per-skill `reason` mounted/pulled-by, `includes`, `imports`, `references`, plus `warnings`) in `crates/skillc-core/src/emit.rs`
- [X] T043 [P] [US5] Agent formatter trait + `claude` formatter in `crates/skillc-core/src/agent/mod.rs` and `crates/skillc-core/src/agent/claude.rs`
- [X] T044 [P] [US5] Additional agent formatters (`codex`, `cursor`, `gemini`, `copilot`) in `crates/skillc-core/src/agent/`
- [X] T045 [US5] Implement `install`: place a built artifact into `--dest` in the agent's format, idempotent on identical bytes (FR-023/025), in `crates/skillc-core/src/install.rs`

**Checkpoint**: Deterministic, inspectable artifacts emit per (target, agent); install is a separate idempotent step

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Cross-story flags, diagnostics quality, docs, and end-to-end validation

- [X] T046 [P] Wire `--all-targets` / `--all-agents` cartesian build over the registry in `crates/skillc/src/cli.rs`
- [X] T047 [P] Diagnostics polish: source-spanned, actionable messages (`miette`/`ariadne`) for every error code (FR-026), audited against the cli.md sample messages
- [X] T048 [P] Author `README.md` usage docs (build/install, flags, config, lockfile)
- [X] T049 Run all quickstart.md scenarios 1–7 end-to-end against the release binary and record results
- [X] T050 [P] Add the determinism CI job (`build --locked --frozen` twice + `diff -r`, fail on any byte difference) per cli.md determinism contract

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately
- **Foundational (Phase 2)**: Depends on Setup — BLOCKS all user stories
- **User Stories (Phase 3–7)**: All depend on Foundational. Because this is a compiler pipeline, the P1 stories layer in this order for a working build:
  - US1 (blocks/assemble) and US2 (markers/imports/deps) are largely independent at the parse/resolve layer and can proceed in parallel after Foundational
  - US3 (shake) consumes the graph from US1 + the import edges from US2 → start US3 after US1's `graph.rs` and US2's `resolve/imports.rs` exist
  - US4 (validate) consumes the schema + the resolution results of US1/US2 → start after US1/US2 resolve paths exist
  - US5 (emit/install) consumes the assembled bundle from US1/US2/US3 → start after assemble is in place
- **Polish (Phase 8)**: Depends on all desired user stories being complete

### Within Each User Story

- Tests (fixtures + integration) written FIRST and FAIL before implementation
- Parse before resolve; resolve before graph/shake; assemble before emit
- Story complete and its integration test green before moving to the next priority

### Parallel Opportunities

- Setup: T003, T004 in parallel
- Foundational: T005, T006 in parallel (independent files); T007–T011 sequence into the pipeline
- US1: T012, T013 (test setup) in parallel; impl T014–T018 mostly sequential (shared `assemble.rs`/graph)
- US2: T019, T020 in parallel; T021/T023/T024 touch different files and can overlap
- US5: T043, T044 (agent formatters) in parallel; T040 test up front
- Once Foundational is done, US1 and US2 can be staffed in parallel by two developers

---

## Parallel Example: User Story 2

```bash
# Test setup for US2 in parallel:
Task: "Create fixtures in tests/fixtures/us2-markers-deps/"
Task: "Integration test in tests/integration/us2_markers_deps.rs"

# Implementation across distinct files can overlap:
Task: "Parse {{Alias}} markers in crates/skillc-core/src/parse/marker.rs"
Task: "Cross-repo source fetch in crates/skillc-core/src/resolve/source.rs"
Task: "Lockfile read/write in crates/skillc-core/src/resolve/lock.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 only)

1. Complete Phase 1: Setup (workspace + deps)
2. Complete Phase 2: Foundational (model, parse, config, pipeline + CLI skeleton) — CRITICAL, blocks all
3. Complete Phase 3: User Story 1 — `@include` reuse, the framework's reason to exist
4. **STOP and VALIDATE**: change a block once → both includers re-sync; missing/cyclic include fail
5. This is a demonstrable MVP: reuse-without-copy-paste working end-to-end

### Incremental Delivery

1. Setup + Foundational → pipeline skeleton runs
2. + US1 → `@include` reuse (MVP)
3. + US2 → `{{Alias}}` pointers + cross-repo deps
4. + US3 → per-target entry-point membership + reference pruning
5. + US4 → schema conformance enforced
6. + US5 → deterministic inspectable artifact + install
7. Polish → cartesian builds, diagnostics, docs, CI determinism gate

### Parallel Team Strategy

After Foundational: Developer A on US1, Developer B on US2 (independent parse/resolve files); converge on US3 (shake needs both graphs), then US4 (validate) and US5 (emit) layer on the assembled bundle.

---

## Notes

- [P] = different files, no dependency on an incomplete task
- This is a compiler: stories are independently *testable* (via fixtures) but the pipeline genuinely layers — the Dependencies section records the real edges so parallel work doesn't stall on a missing stage
- Tests-first per plan.md (insta golden artifacts + per-scenario fixtures); verify each story's integration test fails before implementing
- Determinism (byte-identical output) is a hard requirement, asserted in US5 and gated in CI (T050)
