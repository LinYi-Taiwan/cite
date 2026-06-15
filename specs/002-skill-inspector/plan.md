# Implementation Plan: Skill & Context Inspector

**Branch**: `002-skill-inspector` | **Date**: 2026-06-15 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/002-skill-inspector/spec.md`

## Summary

A **consumption-side inspector + control panel** for people who *use* AI coding agents. It scans
every skill installed on the machine across multiple sources (the agent's user-level skill dir, the
project's `.claude/skills/`, installed plugins, and — later — other agents), builds one unified,
searchable **inventory** (US1), groups look-alike skills into **overlap clusters** (US2), lets the
user **act safely** — reversible disable, confirmed remove, label/group (US3) — and surfaces
**activation + best-effort runtime context transparency** (US4) across **multiple agents** (US5).

**Technical approach**: Reuse the existing Rust workspace. The compiler half of `skillc` is *not*
extended; instead the two assets the pivot explicitly keeps — the `SKILL.md` frontmatter parser
(`crates/skillc-core/src/parse/frontmatter.rs`) and the self-contained force-directed visualization
(`crates/skillc-core/assets/graph.html`, fed by the `GraphExport { nodes, edges }` JSON contract from
`graph_export.rs`) — become the seeds of a new `skill-inspector` crate. The inspector is a single Rust
binary offering: a read-only `scan` (build inventory + clusters, emit JSON / static HTML snapshot) and
an interactive `serve` (local web UI + a tiny local HTTP API that performs the disk-mutating
disable/remove/restore actions on explicit user request). Overlap detection is **deterministic,
offline, lexical** (no LLM, no network) — this is the property that makes the tool vendor-independent.
Scanning never mutates; only explicit actions through `serve` write to disk. **Disable is a
per-source-provider strategy that prefers the agent's native per-project scoping** so a globally-installed
skill can be turned off for *one folder only* (the core pain): for Claude Code this writes the
`skillOverrides` map into the project's `.claude/settings.local.json`. Sources with no native scoped
toggle (e.g. Claude plugin skills) fall back to a reversible quarantine-move tracked in a tool-owned
manifest, surfaced honestly as a *global* disable (research.md §4).

## Technical Context

**Language/Version**: Rust 1.83+ (stable, 2021 edition) — same toolchain as the existing workspace;
reuses its frontmatter parser and graph visualization (research.md §1).

**Primary Dependencies**:
- `serde` + `serde_norway` — parse `SKILL.md` YAML frontmatter (name/description) and read/write the
  tool's quarantine + labels manifest. Already the workspace's YAML stack (research.md §1).
- `serde_json` — emit the inventory/overlap export consumed by `graph.html`, and the local API payloads.
- `clap` (derive) — CLI surface: `skill-inspector scan` / `serve`.
- `walkdir` (or std `read_dir` recursion) — enumerate skill roots across sources (research.md §2).
- `sha2` — content hash per skill for duplicate-by-identity detection and safe-remove backups (reused
  from workspace deps).
- A minimal embedded HTTP server (`tiny_http`-class) — serve the local web UI and the action API for
  `serve`; chosen over a full async framework to keep the binary small and the tool offline/local-only
  (research.md §5).
- Overlap similarity is hand-rolled (token-set / n-gram Jaccard + TF-IDF cosine) — no ML dependency
  (research.md §3).

**Storage**: Filesystem only. **Reads**: each agent's skill roots (e.g. `~/.claude/skills/`, project
`./.claude/skills/`, plugin skill dirs) and each agent's per-project settings (e.g. Claude
`./.claude/settings.local.json` for current `skillOverrides` state). **Writes** (only on explicit
action): (1) **per-project agent config** — primary disable path: the `skillOverrides` map in the target
folder's `.claude/settings.local.json` (Claude); (2) a **quarantine directory** for the fallback
global-disable of sources lacking a scoped toggle (e.g. plugin skills); plus (3) a tool-owned
`inspector-state.json` manifest (quarantine records, labels, groups). For runtime-context transparency
(US4), **read-only** parsing of agent session transcripts where they exist (e.g. Claude Code
`~/.claude/projects/**/**.jsonl`). No database.

**Testing**: `cargo test` (unit + integration). Fixture skill-root trees under `tests/fixtures/`
exercising each acceptance scenario/edge case (multi-source, malformed frontmatter, duplicate ids,
unreadable source, no-skills, no-overlap). `insta` snapshots for deterministic inventory + cluster
output and for the `GraphExport` JSON. Action tests run against temp-dir fixture roots and assert
disable→re-enable round-trips losslessly and that scanning never mutates.

**Target Platform**: Local developer machine (macOS + Linux). Single binary; the interactive UI is a
local web server reachable in a browser. Offline by default — no outbound network.

**Project Type**: Local desktop tool = CLI + local web UI (one new crate in the existing Rust
workspace).

**Performance Goals**: Interactive, not a hot path. Cold `scan` → full inventory visible in **< 30 s**
(SC-001) for a realistic machine of O(10²) installed skills; search/filter in the UI feels instant
(< 1 s, FR-004). Determinism over throughput: identical roots → identical inventory + cluster output.

**Constraints**: **Safe-by-default** — `scan`/analysis never change the user's setup (FR-013, FR-021,
SC-005); only explicit `serve` actions write, disable is reversible/lossless (FR-011), remove requires
confirmation + backup (FR-012). **Honest boundary** — overlap is advisory only (FR-009); runtime
context data that an agent does not expose is marked *unavailable*, never fabricated (FR-018, SC-007).
Offline/local-only (no skill list leaves the machine).

**Scale/Scope**: One machine, one user. O(10²)–O(10³) installed skills across O(1–5) sources/agents.
MVP source = the user's primary agent (Claude Code); additional agents (Cursor, …) are pluggable
source providers added incrementally (US5, Assumptions).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

The project constitution (`.specify/memory/constitution.md`) is the **unratified template** — every
principle slot is a placeholder with no version/ratification date. There are therefore **no
project-specific gates to evaluate and nothing to violate** (identical situation to `001`).

In the absence of ratified principles, the plan holds itself to the safety/honesty constraints the
**spec itself** mandates, which double as sensible default gates:

| Default gate | Source | Status |
|---|---|---|
| Safe-by-default: read/analyze never mutates | FR-013, FR-021, SC-005 | ✅ `scan` is pure-read; only `serve` action endpoints write |
| Reversible disable, confirmed+backed-up remove | FR-011, FR-012 | ✅ Tier-1 `skillOverrides` write (folder-scoped, reversible) / Tier-2 quarantine-move + restore manifest; remove backs up then deletes |
| Per-folder disable of a global skill | FR-023, FR-024 | ✅ Tier-1 writes `skillOverrides` to the folder's `settings.local.json` — global skill off in one folder only, files untouched; Tier-2 fallback labeled global |
| Overlap is advisory, never auto-prunes | FR-009 | ✅ clustering produces suggestions only; no action without user click |
| Never fabricate vendor-gated data | FR-018, SC-007 | ✅ context-load record carries explicit `unavailable` status |
| Offline / local-only (privacy) | Assumptions | ✅ no outbound network; similarity is local lexical, no API |
| Simplicity / YAGNI | (no constitution) | ✅ one new crate, fs-only, embedded server — no DB/daemon/cloud |

**Result**: PASS (no gates defined; no unjustified complexity). Re-checked post-design below.

**Post-design re-check (after Phase 1)**: PASS. The data model introduces no entity not demanded by a
functional requirement; the one new crate adds no extra deployable. Complexity Tracking is empty.

## Project Structure

### Documentation (this feature)

```text
specs/002-skill-inspector/
├── plan.md              # This file (/speckit-plan output)
├── research.md          # Phase 0 — stack reuse, scan roots, overlap algo, disable mechanism, context data, UI delivery
├── data-model.md        # Phase 1 — entities: Skill, SkillSource, OverlapCluster, ActivationState, ContextLoadRecord, InspectorState
├── quickstart.md        # Phase 1 — runnable validation scenarios per user story
├── contracts/           # Phase 1 — CLI, source-provider, inventory/overlap export, action API, disabled-state
│   ├── cli.md
│   ├── source-provider.md
│   ├── inventory-export.md
│   ├── action-api.md
│   └── inspector-state.md
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created here)
```

### Source Code (repository root)

A new `skill-inspector` crate is added to the existing Cargo workspace. The compiler crates
(`skillc`, `skillc-core`) are left intact; the inspector **depends on** `skillc-core` only for the two
reused seeds (frontmatter parsing + the graph/asset pipeline), and otherwise owns its own scan /
overlap / action logic.

```text
Cargo.toml                          # workspace manifest — add "crates/skill-inspector" to members
crates/
├── skillc/                         # (unchanged) compiler binary
├── skillc-core/                    # (unchanged) — reused: parse/frontmatter.rs, assets/graph.html, graph_export contract
└── skill-inspector/                # NEW: the inspector binary + its core
    ├── Cargo.toml
    ├── assets/
    │   └── inspector.html          # the control-panel UI — graph.html (copied seed) + inventory list + cluster panels + action controls
    └── src/
        ├── main.rs                 # clap dispatch: `scan`, `serve`
        ├── cli.rs                  # arg structs (source selection, output format, port)
        ├── model.rs                # Skill, SkillSource, OverlapCluster, ActivationState, ContextLoadRecord
        ├── scan/
        │   ├── mod.rs              # orchestrates: discover sources → walk roots → parse → inventory (FR-001..005)
        │   ├── source.rs           # SourceProvider trait + registry (pluggable per agent — US5/FR-019)
        │   ├── claude.rs           # Claude Code provider: user/project/plugin skill roots
        │   └── skill_md.rs         # parse SKILL.md frontmatter (reuses skillc-core), metadata-completeness flag (FR-003)
        ├── overlap.rs              # lexical similarity → clusters; duplicate-by-identity via hash (FR-006..010)
        ├── activation.rs           # eligible/active per context (FR-016)
        ├── context/
        │   ├── mod.rs              # ContextLoadRecord assembly; availability status (FR-017/018)
        │   └── claude_transcript.rs# best-effort parse of session transcripts where present
        ├── action/
        │   ├── mod.rs              # disable / enable / remove / label — the ONLY writers (FR-011..014); dispatches disable to the source's strategy
        │   ├── overrides.rs        # Tier-1 per-folder disable: read/write skillOverrides in <folder>/.claude/settings.local.json (Claude); reversible, folder-scoped
        │   └── quarantine.rs       # Tier-2 fallback global disable (plugin skills / no scoped toggle): reversible move + restore; inspector-state.json manifest
        ├── export.rs               # inventory + clusters → JSON for the UI (extends GraphExport shape)
        ├── server.rs               # `serve`: embedded HTTP — static UI + action API endpoints
        └── state.rs                # InspectorState (quarantine records, labels, groups) read/write
tests/
├── fixtures/                       # synthetic skill-root trees per scenario/edge case
└── integration/
    ├── scan_inventory.rs           # multi-source scan, malformed/duplicate/unreadable handling (insta)
    ├── overlap_clusters.rs         # known-overlap grouping + no-false-grouping (insta)
    └── action_roundtrip.rs         # disable→enable lossless; scan never mutates; remove backs up
```

**Structure Decision**: One new binary crate (`skill-inspector`) in the existing workspace, reusing
`skillc-core`'s frontmatter parser and `graph.html` visualization as seeds (the two assets the pivot
keeps). Read (`scan`) and write (`serve` actions) are separated at the module boundary —
`scan/`+`overlap.rs`+`activation.rs`+`context/` are pure-read; only `action/` writes — so the
safe-by-default gate is structural, not just disciplined. Source providers are a trait+registry so
adding an agent (US5) is a new file, not a rewrite. No second deployable, no DB, no daemon (YAGNI).

## Complexity Tracking

> No constitution gates are defined and no gate is violated; no complexity to justify.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| — | — | — |
