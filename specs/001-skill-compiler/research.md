# Phase 0 Research: Skill Compiler

All NEEDS CLARIFICATION items from Technical Context are resolved below. Each entry follows
Decision / Rationale / Alternatives considered.

---

## 1. Implementation language — Rust vs Go

**Decision**: **Rust** (stable 1.83+, 2021 edition).

**Rationale**: The user explicitly delegated the choice ("編譯看要用 golang or rust 都可以").
The artifact being built is *literally a compiler*, and Rust's core idioms map 1:1 onto the
problem:
- **Sum types** model the three unit kinds (`Block | Skill | Reference`) and the agent
  formats directly; the spec says "what an imported/included unit becomes is determined by the
  unit's own kind" — an `enum Unit { Block(..), Skill(..), Reference(..) }` makes that a
  total, exhaustively-matched dispatch instead of runtime tagging.
- **Exhaustive `match` + `Result`/`enum DiagnosticKind`** fit the ~12 distinct hard-failure
  classes the spec enumerates (missing block, include cycle, undefined alias, unresolved
  import, version conflict, missing entry skill, stray file, missing required reference, …).
  The compiler must fail loudly and name the rule (FR-026); an exhaustive error enum makes
  "did we handle every failure mode" a compile-time check.
- **Determinism by construction** (FR-022/SC-007): no implicit map iteration order leaking
  into output if we use ordered collections; `gix` gives hermetic fetch without shelling out.
- Single static binary, trivial CI distribution.

**Alternatives considered**:
- **Go** — entirely viable and faster to write; excellent CLI ergonomics (`cobra`), mature
  YAML/markdown libs, single binary. Rejected only on fit: Go lacks sum types and exhaustive
  matching, so the unit-kind dispatch and the large closed set of error variants become
  interface type-switches / sentinel errors that the compiler can't prove are complete — for a
  tool whose entire value proposition is "fail loudly on every malformed case," that
  exhaustiveness guarantee is worth the slower iteration. The decision is a genuine toss-up;
  Go would be the pick if speed-to-first-binary outranked type-level failure coverage.
- **TypeScript/Node** (the npm/Vite mental model's native ecosystem) — rejected: the user
  opened the door to a native binary, and determinism + single-binary CI distribution are
  cleaner without a Node runtime. The React/Vite/npm analogy is conceptual, not a stack mandate.

---

## 2. YAML frontmatter & config parsing

**Decision**: `serde` + **`serde_norway`** for all frontmatter and the framework config.

**Rationale**: Frontmatter and the Vite-config-like framework file are YAML; `serde` derive
gives typed deserialization straight into the schema structs (`Skill`, `FrameworkConfig`),
turning "missing required frontmatter field" (FR-016) into a deserialization error we can
re-message. The original `serde_yaml` is **deprecated/archived** by its author, and the
popular `serde_yml` fork carries **RUSTSEC-2025-0068** (unsound, unmaintained). `serde_norway`
is the actively-maintained `serde_yaml` fork.

**Alternatives considered**: `serde_yaml_ng` (also a maintained fork, but depends on the
unmaintained `unsafe-libyaml`); `serde-saphyr` / `yaml-rust2` (pure-Rust, lower-level, no
drop-in serde DOM). `serde_norway` chosen for the closest maintained drop-in to the familiar
serde_yaml API. Pin the exact version at implementation; re-check RustSec at that time.

**Sources**:
- [serde-yaml deprecation thread (rust-lang forum)](https://users.rust-lang.org/t/serde-yaml-deprecation-alternatives/108868)
- [RUSTSEC-2025-0068 — serde_yml unsound/unmaintained](https://rustsec.org/advisories/RUSTSEC-2025-0068.html)

---

## 3. Dependency graph: closure, cycles, dedup

**Decision**: **`petgraph`** for the `@include`/import graph.

**Rationale**: The spec's central data structure is "the transitive set reachable via
`@include`/imports; must be acyclic and fully resolvable" (Key Entities → Dependency closure).
`petgraph` provides `algo::toposort` which **returns an error on a cyclic graph** — exactly
FR-004 (`@include` cycle) and dependency-cycle detection in one call, with the cycle node in
hand to name in the diagnostic. A directed graph also gives us reachability-from-entry for the
shake stage (FR-013) and natural dedup (a node included via multiple paths is one node →
FR-010 diamond dedup) for skill-dependency nodes.

**Alternatives considered**: hand-rolled DFS with a visited/in-stack set (fine, but
re-implements toposort + SCC and is more error-prone to get the cycle-reporting right);
`graph-cycles` crate (enumerates *all* cycles — useful only if we want to report every cycle
rather than fail on the first, a possible later enhancement). `petgraph` chosen as the
standard, well-tested base.

**Sources**:
- [petgraph `algo::toposort` (errors on cycle)](https://docs.rs/petgraph/latest/petgraph/algo/fn.toposort.html)
- [`graph-cycles` crate](https://crates.io/crates/graph-cycles)

---

## 4. Cross-repo dependency fetching (npm-style)

**Decision**: **`gix`** (gitoxide) to fetch external skill sources, pinned through a
`skills-lock.json`; content-addressed by commit + path hash (`sha2`).

**Rationale**: FR-008 requires importing a skill from another repo and pulling it (and its
peers) into the bundle; FR-011 requires deterministic resolution with **loud conflict failure**
(no silent version pick); FR-022 requires byte-identical output. A lockfile pinning each
external source to an exact commit makes fetch reproducible; `gix` is pure-Rust so the build
doesn't depend on a system `git` and stays hermetic/CI-friendly. Hashing fetched content gives
the deterministic dedup key for FR-010 and a drift signal for FR-022.

**Alternatives considered**: shelling out to `git` (`std::process::Command`) — simpler but
introduces an external dependency and nondeterminism risk (user git config, credential prompts);
rejected for hermeticity. Reusing the prior system's `skills-lock.json` format verbatim — the
spec marks the source-pinning mechanism as a planning decision (Assumptions); we adopt the
*concept* (a lockfile) but design the schema fresh as part of the greenfield replacement, not
inherit the old structure.

**Open at implementation**: exact lockfile schema (fields per source: url, ref/commit, subpath,
content-hash) is specified in [contracts/framework-config.md](./contracts/framework-config.md);
auth for private repos is out of scope for the first cut (assume reachable/public or
pre-cloned), to be revisited.

---

## 5. Authoring syntax: `@include` and `{{Alias}}`

**Decision**: Keep the spec's proposed spellings — `@include <block>` (line-level directive)
and `{{Alias}}` (inline marker) — with frontmatter `imports: { Alias: <path> }`. Parse markdown
with `pulldown-cmark` for structural splicing where needed; treat `@include` as a line
directive resolved before/around markdown rendering, and `{{Alias}}` as an inline token scanned
in body text.

**Rationale**: The spec fixes the *semantics* (inline-content reuse; resolved pointer; static
link-check) and explicitly marks the *spelling* as non-final ("Syntax is proposed, not final").
The proposed forms are unambiguous and easy to scan deterministically. `{{ }}` and `@include`
don't collide with common markdown; a literal-escape hatch (e.g. `\{{` ) is a small detail for
the schema contract.

**Alternatives considered**: component-tag syntax (`<Include block="…"/>`) — heavier, invites
an HTML-ish parser; `[[WikiLink]]` for pointers — collides with some markdown supersets.
Deferred; the three semantics are what matter and are locked.

---

## 6. Determinism strategy

**Decision**: (a) ordered collections (`BTreeMap`/sorted `Vec`) anywhere iteration feeds output;
(b) no wall-clock/random/env in the artifact; (c) `gix` pinned fetch; (d) `insta` golden
snapshots in CI to *prove* byte-stability and fail on drift.

**Rationale**: Directly satisfies FR-022 / SC-007 and makes "CI fails on drift" a real gate
rather than an aspiration. Sorting is the cheapest way to kill HashMap-iteration nondeterminism.

**Alternatives considered**: hashing the artifact and comparing a single digest (used *in
addition* for a fast CI check, but snapshots give a readable diff when it breaks).

---

## Resolved unknowns summary

| Technical Context item | Resolution |
|---|---|
| Language/Version | Rust 1.83+ (§1) |
| Frontmatter/config parser | serde + serde_norway (§2) |
| Graph/closure/cycles | petgraph (§3) |
| Cross-repo fetch + lock | gix + skills-lock.json + sha2 (§4) |
| Syntax spelling | spec's `@include` / `{{Alias}}` retained (§5) |
| Determinism | ordered collections + pinned fetch + insta snapshots (§6) |

No remaining NEEDS CLARIFICATION.
