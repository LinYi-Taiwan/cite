# Phase 0 Research: Skill & Context Inspector

All Technical Context unknowns are resolved below. Each item: **Decision / Rationale / Alternatives
considered**. Nothing is left as NEEDS CLARIFICATION.

---

## §1 — Stack: reuse the existing Rust workspace and its two kept seeds

**Decision**: Build the inspector as a new `skill-inspector` binary crate inside the existing Cargo
workspace (Rust 1.83+, 2021 edition). Depend on `skillc-core` only to reuse (a) the `SKILL.md`
frontmatter parser in `crates/skillc-core/src/parse/frontmatter.rs` and (b) the self-contained
visualization `crates/skillc-core/assets/graph.html` plus the `GraphExport { nodes, edges }` JSON shape
from `graph_export.rs`. Do **not** extend the compiler pipeline.

**Rationale**: The pivot explicitly says keep the graph visualization and drop most of the compiler
("graph 視覺化留著，compiler 那一大半可以丟"). The graph viewer is already a 607-line self-contained
HTML force-directed explorer whose nodes carry exactly the fields the inventory needs (`id`, `kind`,
`description`, `source`, `flags`) — it is a ready-made seed for the inventory/overlap visual. The
frontmatter parser already reads the `SKILL.md` `name`/`description` the inventory is built from. Same
toolchain → zero new build/CI surface, reuse `serde_norway`/`serde_json`/`sha2`/`clap`/`insta` already
in `[workspace.dependencies]`.

**Alternatives considered**:
- *Node/TypeScript + Vite/React SPA* — matches "GUI" expectations and a rich UI, but throws away both
  kept seeds, adds a second toolchain, and the file-scan/mutation core is better served by Rust's
  robust fs + deterministic hashing. Rejected for MVP; the UI can still be enriched later as plain
  JS/HTML without a framework.
- *Extend `skillc` with an `inspect` subcommand* — couples a consumption tool to a compiler's data
  model (catalogs/targets/agents authored by *you*), which is the wrong domain (installed skills, not
  an authored catalog). Rejected: keep the crates separate, share only the two seeds.

---

## §2 — Where installed skills live (scan roots) and how sources are modeled

**Decision**: Model each origin as a `SourceProvider` (trait + registry). MVP ships the **Claude Code**
provider, which discovers these roots, each a `SkillSource`:
- user-level: `~/.claude/skills/`
- project-level: `<cwd>/.claude/skills/` (and parent dirs up to repo root)
- plugin-level: skill dirs inside installed plugins under the Claude config dir
Every root is walked for `<id>/SKILL.md` entries. Convergence note from prior verification: all the
agents in scope use the `skills/<id>/SKILL.md` layout, so one walker shape serves every provider; only
the **root discovery** differs per agent. Each source records `kind`, `root path`, and a
`readable/available` status so an unreadable root degrades gracefully (FR-edge: missing/unreadable
source).

**Rationale**: A trait+registry makes US5 (additional agents) a *new file*, not a rewrite, and keeps
the safe-by-default boundary uniform. Claude Code first because it is the user's daily driver and the
evidenced pain lives there (Assumptions).

**Alternatives considered**:
- *Hard-code a single `~/.claude/skills` path* — fails project-level + plugin skills (a big chunk of
  the overlap the user feels) and blocks US5. Rejected.
- *Config file listing roots* — more flexible but pushes setup burden onto the user for the common
  case. Rejected for MVP; providers auto-discover, with an optional override left as a later add.

> Implementation note (evidence): the exact plugin skill-root layout under the Claude config dir is
> verified against the live machine during implementation (a fixture mirrors whatever shape is found);
> the *plan* commits only to "the agent's documented skill locations", not to a guessed absolute path.

---

## §3 — Overlap detection: deterministic, offline, lexical

**Decision**: Two distinct mechanisms.
1. **Duplicate-by-identity** (FR-008): same skill `id` appearing under more than one root, and/or
   identical content hash (`sha2`). Exact, cheap, unambiguous.
2. **Similarity-based overlap** (FR-006/007): cluster skills by lexical similarity of
   `name + description` — token-set / character n-gram **Jaccard** combined with **TF-IDF cosine** over
   the corpus of all skill descriptions, above a tuned threshold. Each cluster carries a human-readable
   reason ("shared terms: code-review, react, lint"). Purely local, deterministic, no model, no network.

**Rationale**: The whole product thesis is *vendor-independent and offline* — pulling descriptions to a
remote embedding API would betray that and leak the user's skill list. Lexical similarity is fully
deterministic (required for `insta` snapshot tests and for "identical roots → identical clusters") and
is more than sufficient to catch the user's evidenced overlaps (three `speckit` families, ten-plus
code-review skills, the playwright/e2e cluster) because those share heavy literal vocabulary. Advisory
only (FR-009): clustering never triggers an action.

**Alternatives considered**:
- *Local embedding model* (e.g. a bundled sentence encoder) — better semantic grouping, but adds a
  large dependency/model file, nondeterminism risk, and slower cold scan. Deferred as a future
  precision upgrade behind the same cluster interface.
- *Remote embeddings API* — best quality, but violates offline/privacy thesis and adds a vendor
  dependency the product exists to avoid. Rejected.

---

## §4 — Disable / remove without breaking the agent

**Decision**: Disable is a **per-source-provider strategy**, preferring the agent's *native per-project
scoping mechanism* over a file move. The driving pain is: "I disable a skill in project A and discover
it was global, so I broke it everywhere." The fix is to scope the disable to a single folder, which a
file move cannot do for a globally-installed skill.

- **Disable — Tier 1 (preferred): native per-project config.** Where the agent exposes a per-project
  skill toggle, the tool writes that instead of touching the skill's files. For **Claude Code** this is
  the **`skillOverrides`** setting written to the project's **`.claude/settings.local.json`**: a map of
  `skill-name → state`, where state is one of `"on" | "name-only" | "user-invocable-only" | "off"`; a
  skill absent from the map is treated as `"on"`. Setting a user-level/global skill to `"off"` in a
  project's `settings.local.json` deactivates it **for that folder only**, leaving other folders
  untouched and the skill's files on disk intact. Re-enable = remove the key (or set `"on"`) → lossless,
  reversible (FR-011), and natively per-folder (the core pain point). Source:
  <https://code.claude.com/docs/en/skills> (§"Override skill visibility from settings"). The `/skills`
  TUI writes the same key — the inspector writes the same file, so the two stay consistent.
- **Disable — Tier 2 (fallback): quarantine-move.** For sources that have **no** native per-project
  toggle — notably **Claude plugin skills**, which `skillOverrides` explicitly does **not** affect
  (managed via `/plugin`), and any future agent lacking a scoped toggle — the tool falls back to moving
  the skill directory out of its active root into a tool-owned **quarantine** dir, recording the move
  (original root, id, hash, timestamp) in `inspector-state.json`. **This fallback is global in effect**
  (the agent stops seeing the skill everywhere), so the UI MUST label it as a global disable, distinct
  from a Tier-1 per-folder disable, rather than silently pretending it is folder-scoped. Re-enable moves
  it back to the exact original location → lossless, reversible.
- **Remove** = require an explicit confirmation step (FR-012), take a backup copy (into a tool-owned
  trash) *then* delete from the active root. Restorable from backup; never a bare unconfirmed delete.
- All of the above are the **only** disk writers, isolated in `action/`. They run exclusively through the
  `serve` action API on explicit user click — never during `scan`/analysis (FR-013, FR-021, SC-005).

**Rationale**: A file move is portable but **cannot express folder scope** for a globally-installed
skill — the exact behaviour the user is complaining about. Where the agent itself defines a per-project
override (Claude's `skillOverrides` in `settings.local.json`), writing that is both safer (the user's
authored skill files are never touched) and *correct* for the per-folder requirement. The quarantine
move is retained only as the portable fallback for sources that genuinely have no scoped toggle, and is
honestly surfaced as global when used. This makes disable a property of each `SourceProvider`, not a
single hard-coded file operation.

**Alternatives considered**:
- *Quarantine-move as the primary mechanism for all sources* (the prior decision) — rejected: it is
  global-by-construction, so it reproduces the user's pain (disabling a global skill kills it in every
  folder) instead of solving it. Demoted to the Tier-2 fallback.
- *Write a `disabled: true` into the skill's own frontmatter* — mutates the user's authored file and
  applies globally, not per-folder. Rejected (unsafe + wrong scope).
- *Rename in place (e.g. `SKILL.md.disabled`)* — fragile, global, clutters the user's own dirs.
  Rejected.

**Open (deferred, not blocking)**: the equivalent per-project mechanism for **other agents** (Codex,
Cursor, …) is not yet researched — each `SourceProvider` resolves its own Tier-1 strategy, and absent
one it uses the Tier-2 fallback. Tracking this as a per-provider research item, not an MVP blocker.

---

## §5 — UI delivery: local web server (interactive) + static HTML (snapshot)

**Decision**: `skill-inspector scan` emits a read-only artifact (JSON, and a static self-contained HTML
snapshot reusing `graph.html`). `skill-inspector serve` starts a small embedded HTTP server that serves
`assets/inspector.html` (the graph viewer extended with an inventory list, cluster panels, and action
controls) plus a tiny JSON **action API** the UI calls to disable/enable/remove/label. Browser is the
GUI surface.

**Rationale**: The user asked for visualization *and* "GUI 操作" (actions). A static HTML cannot mutate
disk, so the action path needs a live local backend — an embedded HTTP server keeps it local-only,
dependency-light, and offline, while `graph.html` gives the visual for free. Static export still serves
the pure-inventory/overlap snapshot (shareable, no server) for the read-only case.

**Alternatives considered**:
- *Native desktop GUI* (egui/Tauri) — heavier, platform packaging burden, and discards the HTML seed.
  Rejected for MVP.
- *TUI* — fast to build but weak for the graph/overlap *visualization* the user explicitly wants.
  Rejected as the primary surface (could be an extra later).

---

## §6 — Runtime context transparency (US4): what each agent exposes

**Decision**: Split US4 into the part the tool can compute and the part the vendor gates.
- **Computable now** (FR-016): "eligible/triggerable" and "active" — derived from on-disk state (which
  skills are in active roots vs quarantined, and which match the current project context). Fully
  deliverable.
- **Best-effort, vendor-gated** (FR-017/018): "what the agent actually loaded this turn." For Claude
  Code, session transcripts under `~/.claude/projects/**/**.jsonl` may record skill/context loads; the
  `context/claude_transcript.rs` reader parses them **read-only** where present. When an agent exposes
  no such record, the `ContextLoadRecord` is returned with status `unavailable` — explicitly, never
  fabricated (SC-007).

**Rationale**: Honesty boundary is a hard gate (FR-018): the tool must distinguish verified on-disk
facts from data it cannot obtain. Computing eligibility/active from disk needs no vendor cooperation, so
US4 still ships real value even where transcripts are absent or schema-unstable.

**Alternatives considered**:
- *Infer "loaded" by re-running the agent's trigger logic* — would be a guess presented as fact;
  forbidden by FR-018. Rejected.
- *Require a vendor API for context* — none is guaranteed; would block all of US4. Rejected in favor of
  best-effort transcript parsing + explicit `unavailable`.

---

## Resolved Technical Context summary

| Field | Resolution |
|---|---|
| Language/Version | Rust 1.83+, existing workspace (§1) |
| Primary deps | serde/serde_norway, serde_json, clap, walkdir, sha2, embedded HTTP server; hand-rolled lexical similarity (§1,§3,§5) |
| Storage | fs only: read skill roots + transcripts; write quarantine + `inspector-state.json` (§2,§4,§6) |
| Testing | cargo test + insta snapshots over fixture skill-root trees (§3,§4) |
| Target platform | macOS + Linux; CLI + local web UI (§5) |
| Project type | local desktop tool (CLI + local web UI), one new crate (§1) |
| Performance | cold scan < 30 s for O(10²) skills; instant filter (§3) |
| Constraints | safe-by-default, reversible disable, advisory overlap, never-fabricate, offline (§3,§4,§6) |
| Scale | one machine/user; O(10²)–O(10³) skills; Claude-first, pluggable agents (§2) |
