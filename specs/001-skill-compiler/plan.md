# Implementation Plan: Skill Compiler

**Branch**: `001-skill-compiler` | **Date**: 2026-06-09 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-skill-compiler/spec.md`

## Summary

A greenfield **framework + compiler** (`skillc`) for authoring AI-agent skills whose
defining purpose is reuse-without-copy-paste. It compiles a catalog of skills, reusable
blocks, and references into self-contained, inspectable per-`(target, agent)` artifacts,
driven by an entry-point pull model (webpack/React-`app.js` analog) over an `@include`
(content reuse) + `{{Alias}}` (resolved pointer) authoring layer with npm-style cross-repo
dependency resolution. The compiler is also the validator: non-conforming or unresolvable
input does not compile.

**Technical approach**: A classic compiler pipeline — `parse → resolve → validate → shake →
assemble → emit → install` — implemented in **Rust** as a single deterministic CLI binary.
The unit kinds (Block / Skill / Reference) map naturally to a Rust sum type, the ~12 distinct
hard-failure classes to exhaustively-matched error variants, and the `@include`/import closure
to a `petgraph` directed graph with built-in toposort/cycle detection. Output is byte-identical
for identical inputs, runnable non-interactively in CI (non-zero exit on any hard failure).

## Technical Context

**Language/Version**: Rust 1.83+ (stable, 2021 edition). Chosen over Go per user direction
("編譯看要用 golang or rust 都可以") — see [research.md](./research.md) §1 for the decision.

**Primary Dependencies**:
- `clap` (derive) — CLI surface: `skillc build` / `skillc install`.
- `serde` + `serde_norway` — frontmatter + framework-config (de)serialization. `serde_yaml`
  is deprecated/archived and `serde_yml` carries RUSTSEC-2025-0068; `serde_norway` is the
  maintained fork (research.md §2).
- `petgraph` — `@include`/import dependency graph: closure traversal, `toposort`, cycle
  detection (research.md §3).
- `pulldown-cmark` — markdown structure awareness where the assembler must splice content.
- `gix` (gitoxide) — cross-repo source fetching for npm-style dependencies (pure-Rust, no
  shelling out to `git`, keeps builds hermetic/deterministic).
- `sha2` — content hashing for the lockfile + deterministic dedup keys.
- `miette` or `thiserror` + `ariadne` — actionable, source-spanned diagnostics (FR-026).

**Storage**: Filesystem only. Inputs: skill catalog (markdown + frontmatter), one framework
config file, a `skills-lock.json` for pinned cross-repo sources. Outputs: a `dist/`-like
artifact tree per `(target, agent)`. No database.

**Testing**: `cargo test` (unit + integration). Golden-file / snapshot tests (`insta`) for
deterministic artifact bytes; fixture catalogs under `tests/fixtures/` exercising each
acceptance scenario and edge case from the spec.

**Target Platform**: Cross-platform CLI (macOS + Linux), single static binary; CI-runnable
headless. No GUI, no network at build time except explicit dependency fetch.

**Project Type**: Compiler / CLI tool (single Rust workspace).

**Performance Goals**: Not a hot path — correctness and determinism dominate. Target: a
catalog of ~hundreds of skills compiles in < 2 s on a laptop. Determinism is the hard
requirement, not throughput (SC-007).

**Constraints**: Byte-identical output for identical `(catalog, config, target, agent)`
(FR-022, SC-007) — no wall-clock, no filesystem-order, no network nondeterminism in the
artifact. Non-zero exit on any hard failure. Self-contained bundles (zero unresolved
`@include`/`{{}}`/deps — FR-009, SC-002).

**Scale/Scope**: Seed registry of ~4 targets (admin/shop/scm/sl-feature) × ~5 agents
(claude/codex/cursor/gemini/copilot), open-ended; catalogs of O(10²) skills/blocks. Single
maintainer-operable; replaces the prior `bin/skillz` installer.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

The project constitution (`.specify/memory/constitution.md`) is the **unratified template** —
all principle slots are placeholders, no version/ratification date. There are therefore **no
project-specific gates to evaluate**, and nothing to violate.

In the absence of ratified principles, the plan voluntarily holds itself to the engineering
constraints the **spec itself** mandates, which double as sensible default gates:

| Default gate | Source | Status |
|---|---|---|
| Deterministic, CI-runnable, non-interactive CLI | FR-022, SC-007 | ✅ designed in (snapshot tests, hermetic fetch) |
| Compiler-as-validator: non-conforming → non-compiling | FR-016, SC-005 | ✅ validate stage is mandatory, not optional |
| Self-contained artifacts (no unresolved refs) | FR-009, SC-002 | ✅ assemble stage asserts closure-complete |
| Simplicity / YAGNI — no premature service/daemon | (no constitution) | ✅ single binary, fs-only, no DB/server |
| Test-first on the resolution/validation core | (no constitution) | ✅ fixtures-per-scenario before impl (Phase 1 quickstart) |

**Result**: PASS (no gates defined; no unjustified complexity). Re-checked post-design below.

**Post-design re-check (after Phase 1)**: PASS. The data model introduces no entity not
required by a functional requirement; the single-crate-workspace structure adds no project
beyond the one CLI. No entries in Complexity Tracking.

## Project Structure

### Documentation (this feature)

```text
specs/001-skill-compiler/
├── plan.md              # This file (/speckit-plan output)
├── research.md          # Phase 0 output — language + crate decisions
├── data-model.md        # Phase 1 output — entities & the dependency graph
├── quickstart.md        # Phase 1 output — runnable validation scenarios
├── contracts/           # Phase 1 output — CLI / config / skill-schema / artifact contracts
│   ├── cli.md
│   ├── framework-config.md
│   ├── skill-schema.md
│   └── artifact-layout.md
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created here)
```

### Source Code (repository root)

The framework is a standalone Rust workspace. (Per the spec, the `cite` repo holds planning
artifacts; the framework itself owns skill authoring + compilation going forward — the tree
below is the layout the implementation creates, conventionally at the repo that hosts the
framework.)

```text
Cargo.toml                     # workspace manifest
crates/
├── skillc/                    # binary crate — CLI entrypoint
│   └── src/
│       ├── main.rs            # clap dispatch: `build`, `install`
│       └── cli.rs             # arg structs, (target, agent) selection
└── skillc-core/              # library crate — the compiler pipeline
    └── src/
        ├── lib.rs             # pipeline orchestration: parse→resolve→validate→shake→assemble→emit
        ├── config.rs          # FrameworkConfig: target registry + entries + agent defaults (FR-019/020)
        ├── schema.rs          # Skill schema types + conformance rules (FR-016/017/018)
        ├── model.rs           # Unit sum type (Block|Skill|Reference), aliases, edges
        ├── parse/
        │   ├── frontmatter.rs # YAML frontmatter (serde_norway)
        │   ├── include.rs     # `@include <block>` extraction
        │   └── marker.rs      # `{{Alias}}` extraction
        ├── resolve/
        │   ├── imports.rs     # alias → unit resolution (FR-006/007)
        │   ├── source.rs      # cross-repo fetch via gix + skills-lock.json (FR-008/011)
        │   └── lock.rs        # skills-lock.json read/write, content hashing
        ├── graph.rs           # petgraph closure, toposort, cycle detection (FR-004/010)
        ├── validate.rs        # run schema rules, stray-file report, unresolved markers (FR-016/018)
        ├── shake.rs           # per-target reachability from entry; orphan detection (FR-013/014b)
        ├── assemble.rs        # inline blocks, dedup deps, prune foreign references (FR-001/015)
        ├── emit.rs            # deterministic artifact writer (FR-021/022)
        ├── install.rs         # place artifact into agent destination (FR-023)
        ├── agent/             # per-agent output formatters (FR-024)
        │   ├── mod.rs
        │   ├── claude.rs
        │   └── ...            # codex, cursor, gemini, copilot
        └── diagnostics.rs     # error/warning types, actionable messages (FR-026/027)

tests/
├── fixtures/                  # mini catalogs: one per acceptance scenario / edge case
└── integration/              # end-to-end compile+assert (golden artifacts via insta)
```

**Structure Decision**: Single Cargo **workspace** with two crates — a thin `skillc` binary
(CLI/clap) over a `skillc-core` library that holds the whole pipeline. This keeps the pipeline
unit-testable without the CLI shell and leaves a clean seam if a second front-end (e.g. a
`build.rs`-style API or a watch mode) is ever added, while adding no extra deployable project
(YAGNI — one binary ships). Pipeline stages are modules, not crates, until a stage proves it
needs independent versioning.

## Complexity Tracking

> No constitution gates are defined and no gate is violated; no complexity to justify.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| — | — | — |
