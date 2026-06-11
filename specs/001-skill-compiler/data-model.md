# Phase 1 Data Model: Skill Compiler

Entities are derived from the spec's Key Entities + Functional Requirements. The compiler's
in-memory model is a directed graph of **Units** over a **Catalog**, compiled per `(Target,
Agent)` into an **Artifact**. Validation rules are listed per entity.

---

## Unit (sum type)

The central node. Its *kind* (not the import site) decides what it becomes in the artifact
(spec Assumptions → "Unit kind & placement").

```
Unit = Skill | Block | Reference
```

### Skill
An authored unit: a `SKILL.md` conforming to the schema, plus optional `references/`,
`@include`s, and named imports. A graph node that enters a bundle by being **mounted in a
target entry** or **pulled as a dependency**.

| Field | Type | Notes / Rules |
|---|---|---|
| `id` | string (slug) | Unique within catalog; stable dedup key (FR-010). |
| `frontmatter` | Frontmatter | Required fields enforced by schema (FR-016). |
| `imports` | map<Alias, ImportRef> | Alias → unit (possibly cross-repo). FR-006. |
| `includes` | list<BlockRef> | `@include <block>` directives found in body. |
| `markers` | list<Alias> | `{{Alias}}` occurrences in body; each MUST resolve to an `imports` key (FR-007). |
| `references` | list<Reference> | Files under the skill's `references/`. |
| `referenceMode` | enum `per-target` \| `optional` \| `none` | FR-017. |
| `body` | markdown | Source text pre-assembly. |

**Rules**: every `marker` ∈ `imports` keys else hard fail (FR-007). Schema conformance
(required frontmatter, reference naming, well-formed `@include`/`{{}}`) else hard fail (FR-016).
A skill dependency lands as a **top-level skill** in the artifact, deduplicated (FR-010).

### Block
A reusable content fragment authored once, **inlined** into skills via `@include`
(change-once-sync-everywhere — the defining differentiator).

| Field | Type | Notes / Rules |
|---|---|---|
| `id` | string | Unique; the name used in `@include <id>`. |
| `content` | markdown | Inlined verbatim into each includer (FR-001). |
| `includes` | list<BlockRef> | Blocks may include blocks (graph edge; cycle-checked, FR-004). |

**Rules**: `@include` of a missing block → hard fail naming skill + block (FR-003). A block no
skill includes → unused-block **warning** (FR-005). A block inlines into its includer; it is
*not* a standalone artifact node.

### Reference
A `references/<name>.md` belonging to a skill.

| Field | Type | Notes / Rules |
|---|---|---|
| `stem` | string | `<name>` of the file. |
| `kind` | derived: `per-target` \| `shared` | `per-target` iff `stem` is a registered target; else `shared`. |
| `host_skill` | Skill id | A reference lands under its host skill in the artifact. |

**Rules**: for a skill in target *T*'s bundle, retain `references/<T>.md` + all shared
references; **drop** other targets' reference files (FR-015). A reference whose stem is neither
a registered target nor a declared shared reference → stray/typo report (FR-018, FR-014's
"can't hide"). A `referenceMode: per-target` skill applicable to *T* with no `references/<T>.md`
→ hard fail (FR-017); the assertion does **not** fire for non-applicable targets.

---

## Alias / Named import

A frontmatter `Alias → ImportRef` binding referenced in the body via `{{Alias}}`.

| Field | Type | Notes |
|---|---|---|
| `alias` | string | Key in `imports`. |
| `target_ref` | ImportRef | path (local) or source-qualified (cross-repo). |
| `resolved_unit` | Unit ref | filled at resolve; null → hard fail (FR-007/009). |

**Rules**: undefined alias used in body → hard fail naming skill + alias (FR-007). Import
declared but never referenced (no `{{}}`, no `@include`) → unused-import **warning** (FR-012).

## ImportRef / Source

Where a unit resolves from, including external repos.

| Field | Type | Notes |
|---|---|---|
| `source` | enum `Local(path)` \| `Repo(SourceId)` | FR-008 cross-repo. |
| `subpath` | string | Path within the source. |
| `version` | string (optional) | For repo sources; conflict → hard fail (FR-011). |

**Source / lock entry** (`skills-lock.json`):

| Field | Type | Notes |
|---|---|---|
| `id` | string | Logical source name. |
| `url` | string | Repo URL. |
| `commit` | string (sha) | Pinned ref for reproducibility (FR-022). |
| `content_hash` | sha256 | Dedup key + drift signal (FR-010/022). |

---

## Dependency graph / closure

Directed graph; nodes = Units, edges = `@include` (Skill/Block → Block) and import (Skill →
Skill/Reference, possibly cross-repo).

**Rules**:
- MUST be **acyclic** — `@include` cycle or dependency cycle → hard fail naming the cycle
  (FR-004). (petgraph `toposort` error.)
- A node reached via multiple paths is included **once** for skill dependencies (dedup, FR-010);
  a block's *content* still inlines per-includer as authored (diamond reuse, Edge Cases).
- Conflicting versions of the same dependency → surface + fail by default (FR-011); never silent.

---

## Target

A compilation destination (e.g. `admin`, `shop`, `scm`, `sl-feature`), enumerated in config.

| Field | Type | Notes |
|---|---|---|
| `name` | string | Registered in FrameworkConfig (FR-019/020). |
| `entry` | Entry | Mount list of top-level skills. |
| `default_agent` | Agent (optional) | Per-target default (FR-020). |

## Entry point

A target's mount list; the compiler traverses the `@include`/import closure from it (pull).

| Field | Type | Notes |
|---|---|---|
| `mounts` | list<Skill id> | Top-level skills the target wants. |

**Rules**: entry mounts a non-existent skill → hard fail naming entry + skill (FR-014a). A
unit reachable from **no** entry and **no** import → orphan **warning** (FR-014b). Empty entry
→ empty-but-valid bundle (Edge Cases). A skill MAY opt into auto-mount-on-all-targets (FR-014c).

## Agent

An output format/destination (claude, codex, cursor, gemini, copilot), **orthogonal** to target.

| Field | Type | Notes |
|---|---|---|
| `name` | string | Selects the emit formatter (FR-024). |

## Framework config

The single authoritative file (Vite-config analog): target registry (each target + entry) +
per-target defaults. Validation authority for entry mounts and reference filenames (FR-019/020).

| Field | Type | Notes |
|---|---|---|
| `targets` | map<name, Target> | The registry; not inferred (FR-020). |
| `agents` | list<Agent> | Supported output formats. |
| `shared_references` | list<stem> (optional) | Stems explicitly declared shared (collision check, Edge Cases). |

## Build artifact (`dist`-like)

The inspectable, deterministic, self-contained output for one `(target, agent)`.

| Property | Rule |
|---|---|
| self-contained | zero unresolved `@include`/`{{}}`/deps in any non-empty bundle (FR-009/SC-002). |
| deterministic | byte-identical for identical `(catalog, config, target, agent)` (FR-022/SC-007). |
| pruned | foreign-target reference count = 0 (FR-015/SC-006). |
| inspectable | standalone directory, diffable pre-install (FR-021). |

---

## State / pipeline transitions

```
parse     : source files            → Catalog (Units with raw includes/markers/imports)
resolve   : Catalog                 → graph (aliases→units, cross-repo fetch, lock)   [FR-006..011]
validate  : graph                   → graph | HardFail (schema, every @include/{{}} resolves, stray files) [FR-016..018]
shake     : graph + Target.entry    → reachable subgraph + orphan warnings            [FR-013/014b]
assemble  : reachable subgraph      → bundle (blocks inlined, deps deduped, refs pruned) [FR-001/010/015]
emit      : bundle + Agent          → Artifact (deterministic bytes)                   [FR-021/022]
install   : Artifact                → agent destination (separate step)               [FR-023]
```

Any stage may raise a `Diagnostic` (hard fail → non-zero exit, naming unit/rule per FR-026; or
warning surfaced per FR-027).
