# Implementation Plan: Codex Agent Support (Per-Agent Tabs)

**Branch**: `003-codex-skill-support` | **Date**: 2026-06-16 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/003-codex-skill-support/spec.md`

## Summary

Promote **Codex** from the generic `other-agent` bucket of `002-skill-inspector` into a **first-class
agent** with its own **agent-switcher tab**. The feature is additive and reuses the existing inventory /
overlap / activation / action machinery: it (1) adds a `CodexProvider` implementing the existing
`SourceProvider` trait, discovering Codex's three real source kinds — **global/user** (`$CODEX_HOME/
skills/`, with `.system` built-ins), **plugin** (one group per installed marketplace plugin, skills at
`<plugin>/skills/<id>/SKILL.md`), and **project** — and (2) adds an **agent tab** to `inspector.html`
that scopes the whole view to one `agent` at a time (the export already stamps `agent` on every skill
and source).

**Premise correction baked in**: the original ask assumed Codex has no plugins; inspecting the installed
Codex CLI (0.137.0) disproved that — Codex has plugins + marketplaces with an `enabled` toggle in
`~/.codex/config.toml`. So Codex maps almost 1:1 onto the existing Claude provider shape rather than a
reduced "global+skill only" model.

**Disable strategy** (mirrors `002`'s tiered model): Codex **plugin on/off** writes the `enabled` flag
in `[plugins."<name>@<marketplace>"]` of `~/.codex/config.toml` → surfaces as `DisabledPlugin` (global,
all projects), the direct analogue of Claude's `enabledPlugins`. For an **individual Codex skill**, if
provider research confirms a native per-folder toggle it is Tier-1; otherwise the existing reversible
**Tier-2 quarantine** fallback applies, surfaced honestly as a global disable (FR-015). Scanning never
mutates; only explicit `serve` actions write.

## Technical Context

**Language/Version**: Rust 1.83+ (stable, 2021 edition) — same workspace/toolchain as `002`; no new
language surface.

**Primary Dependencies**: All already in the workspace.
- `serde` + `serde_norway` — parse `SKILL.md` frontmatter (shared `skill_md.rs`); read/write the
  inspector state manifest.
- A TOML reader/writer for `~/.codex/config.toml` — Codex stores `[marketplaces.*]` and
  `[plugins."<name>@<mkt>"].enabled` in TOML (Claude used JSON). Decision (research.md §3): add a
  minimal `toml`/`toml_edit` dependency for **format-preserving** edits of the plugin `enabled` flag, or
  restrict writes to a surgical line-level edit — chosen in research to keep the user's hand-written
  config comments/ordering intact.
- `serde_json` — unchanged: inventory/overlap export to `inspector.html`, action API payloads.
- `walkdir` / std recursion — enumerate Codex skill roots (same walker as Claude).
- `sha2` — content hash per skill (shared).
- Embedded `tiny_http`-class server — unchanged; gains Codex action endpoints.

**Storage**: Filesystem + Codex config only. **Reads**: `$CODEX_HOME` (default `~/.codex/`) — `skills/`
(incl. `.system/`), `config.toml` (`[marketplaces.*]`, `[plugins.*]`), installed-plugin skill dirs under
the plugins cache; plus the current project's Codex skill location (path TBD by research). **Writes**
(only on explicit action): (1) the plugin `enabled` flag in `~/.codex/config.toml`; (2) per-skill
disable — Tier-1 native folder toggle if one exists, else Tier-2 quarantine move; (3) the tool-owned
`inspector-state.json` (quarantine records, labels). No database. No network.

**Testing**: `cargo test` (unit + integration), `insta` snapshots. New fixtures: a synthetic
`$CODEX_HOME` tree (global `skills/` with a `.system` built-in, a `config.toml` with one enabled + one
disabled plugin, a plugin skills dir) and a project Codex skill dir. New integration tests:
`scan_codex.rs` (three source kinds discovered + attributed to the `codex` agent; plugin-off skills
flagged `DisabledPlugin`; `.system` marked built-in), `codex_plugin_toggle.rs` (enabled-flag write
round-trips losslessly and preserves the rest of `config.toml`), and an extension of the existing
cross-agent overlap test to include a Codex member. Action tests run against a temp `$CODEX_HOME`; scan
asserts zero mutation.

**Target Platform**: Local developer machine (macOS + Linux). Same single binary + local web UI as `002`.
Offline.

**Project Type**: Local desktop tool = CLI + local web UI; extends the existing `skill-inspector` crate
(no new crate, no new deployable).

**Performance Goals**: Unchanged from `002` — cold `scan` full inventory < 30 s; UI search/filter and
the new tab switch feel instant (< 1 s; the tab switch is a client-side filter over already-loaded data).

**Constraints**: Same safe-by-default / honest-boundary gates as `002` (FR-016, FR-018, FR-019). Codex
config edits MUST be format-preserving and reversible; built-in `.system` skills MUST NOT be offered for
destructive removal (FR-008); a global-only disable MUST be labeled global before applying (FR-015).

**Scale/Scope**: One machine, one user. Codex adds O(10¹–10²) skills across global + N plugins + project.
Agents remain O(1–5).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

The project constitution (`.specify/memory/constitution.md`) is the **unratified template** — every
principle slot is a placeholder with no version/ratification date. There are therefore **no
project-specific gates to evaluate** (identical situation to `001` and `002`).

In the absence of ratified principles, the plan holds itself to the safety/honesty constraints the spec
mandates, which double as sensible default gates:

| Default gate | Source | Status |
|---|---|---|
| Safe-by-default: read/analyze never mutates | FR-016, FR-018 | ✅ Codex discovery + plugin-state read are pure; only `serve` action endpoints write |
| Reversible plugin on/off, no file deletion | FR-012 | ✅ flips `enabled` flag in `config.toml`; plugin files untouched |
| Reversible per-skill disable, honest scope | FR-013, FR-014, FR-015 | ✅ Tier-1 native folder toggle if it exists, else Tier-2 quarantine labeled global |
| Built-ins not destructively removable | FR-008 | ✅ `.system` skills flagged built-in; remove action refused for them |
| Overlap advisory, spans agents | FR-011 | ✅ reuses `002` clustering; Codex members included, no auto-prune |
| On-disk truth each scan | FR-010 | ✅ plugin state re-read from `config.toml` every scan, no cache |
| Offline / local-only | spec Assumptions | ✅ no outbound network; reads local Codex home/config only |
| Simplicity / YAGNI | (no constitution) | ✅ no new crate; one provider + one config writer + a client-side tab |

**Result**: PASS (no gates defined; no unjustified complexity). Re-checked post-design below.

**Post-design re-check (after Phase 1)**: PASS. The data model adds no new entity — `SourceKind` already
has `User`/`Project`/`Plugin`, and `SkillState` already has `DisabledPlugin`; Codex reuses them. The one
new external write target (`config.toml`) is demanded by FR-012. Complexity Tracking is empty.

## Project Structure

### Documentation (this feature)

```text
specs/003-codex-skill-support/
├── plan.md              # This file (/speckit-plan output)
├── research.md          # Phase 0 — Codex skill roots, plugin/marketplace model, config.toml disable, project-skill path, tab UI
├── data-model.md        # Phase 1 — what Codex reuses (Skill/SkillSource/SourceKind/SkillState) + the agent-tab view model
├── quickstart.md        # Phase 1 — runnable validation per user story (scan Codex, toggle a plugin, switch tabs)
├── contracts/           # Phase 1 — Codex source-provider, codex config-write action, agent-tab UI contract
│   ├── codex-source-provider.md
│   ├── codex-action-api.md
│   └── agent-tab-ui.md
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created here)
```

### Source Code (repository root)

No new crate. Changes are localized to the existing `skill-inspector` crate: a new Codex provider, a
Codex config writer in the action layer, and the agent-tab in the UI asset. The `SourceProvider`
trait/registry, the shared `skill_md.rs` walker, the export shape, and the action dispatch are reused
as-is.

```text
crates/skill-inspector/
├── assets/
│   └── inspector.html              # MODIFY: add agent-switcher tabs (filter the already-loaded view by `agent`);
│                                   #         Codex view reuses the per-source-kind grouping; no plugin section is
│                                   #         hidden (Codex has plugins) — built-in `.system` skills shown as built-in
└── src/
    ├── scan/
    │   ├── source.rs               # MODIFY: register CodexProvider in with_defaults() (drop/keep OtherAgent demo)
    │   ├── codex.rs                # NEW: CodexProvider — global/user (+.system), per-plugin (parse config.toml
    │   │                           #      [marketplaces]/[plugins] + plugin skills dir), project roots; plugin
    │   │                           #      enabled→DisabledPlugin mapping; .system built-in flag
    │   └── (claude.rs, skill_md.rs, other_agent.rs unchanged except registry)
    ├── action/
    │   ├── mod.rs                  # MODIFY: dispatch Codex disable to the right strategy (plugin toggle vs skill)
    │   ├── codex_config.rs         # NEW: format-preserving read/write of [plugins."<name>@<mkt>"].enabled in
    │   │                           #      ~/.codex/config.toml (the Codex analogue of overrides.rs/enabledPlugins)
    │   └── quarantine.rs           # REUSE: Tier-2 fallback for a Codex per-skill global disable
    └── model.rs                    # REUSE: SourceKind {User,Project,Plugin}, SkillState {…,DisabledPlugin}; a
                                    #        small built-in flag on Skill/SkillSource if research says surface it
tests/                              # FLAT layout — each file is a [[test]] block in Cargo.toml
├── fixtures/codex_home/            # NEW: synthetic $CODEX_HOME (skills/.system, config.toml, plugin skills)
├── scan_codex.rs                   # NEW: source kinds + agent attribution + plugin-off + built-in flag (insta)
├── codex_plugin_toggle.rs          # NEW: enabled-flag write round-trips, config.toml otherwise byte-stable
├── overlap_clusters.rs             # MODIFY: cross-agent Codex member + Codex duplicate-identity case
└── action_roundtrip.rs             # MODIFY: Codex quarantine round-trip + built-in remove refused
```

**Structure Decision**: Extend the existing `skill-inspector` crate; do **not** add a crate. Codex is a
new `SourceProvider` (`scan/codex.rs`) plus a new action writer (`action/codex_config.rs`), mirroring the
Claude provider + `overrides.rs` pair. The agent tab is a **client-side** filter in `inspector.html` over
the existing per-`agent`-stamped export, so it needs no new server endpoint and no export schema change.
This keeps the safe-by-default boundary structural (only `action/` writes) and proves the trait/registry
extends without a rewrite — exactly the US5 design intent of `002`.

## Complexity Tracking

> No constitution gates are defined and no gate is violated; no complexity to justify.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| — | — | — |
