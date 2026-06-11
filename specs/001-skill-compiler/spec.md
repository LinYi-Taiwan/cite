# Feature Specification: Skill Compiler

**Feature Branch**: `001-skill-compiler`

**Created**: 2026-06-09

**Status**: Draft

**Input**: User description: "做一個 skill 的框架 + compiler，像 React（component 抽出來複用 / 組裝）+ Vite（編譯成目標看得懂的產物）+ npm（跨 repo 依賴一起裝）。決定性差異必須體現在『寫法』上——可複用片段 `@include` 進來、改一次全部同步；內文要引用某個 import 的 skill/reference 時用 `{{Alias}}` 明確指向（自然語言講『用 code review md』太含糊，誰知道哪個）。不然大家回去手寫 markdown 守規範就好，幹嘛做框架。痛點：A/B/C 多個 repo 有共用 skill，手寫只能複製貼上，必然 drift、難維護。"

> **Vision (read this first)**: A greenfield **framework + compiler** for authoring AI-agent skills, whose entire reason to exist is one thing hand-written markdown cannot do: **reuse without copy-paste**. The model is the union of three familiar tools:
> - **React/Vue (the defining differentiator)** — a skill is *assembled* from reusable pieces, not hand-written whole. A shared block (`@include`) is authored once and pulled into many skills; change it once and every skill that uses it re-compiles in sync. Hand-writing means copy-pasting and inevitable drift; this is the line that justifies a framework at all.
> - **Vite/ES6 (incl. entry points)** — each target has an **entry** that mounts its top-level skills; `skillc build` traverses the dependency closure from that entry (like a webpack entry / React `app.js`) and compiles it into a self-contained, inspectable `dist/` artifact. Skills no entry reaches are dead code (reported as orphans); broken references don't compile.
> - **npm (incl. peer deps)** — a skill can depend on another skill, **across repos**; the dependency (and its peers) is pulled in and installed alongside.
>
> **The two authoring affordances that make it a framework (not a linter):**
> - **`@include <block>`** — inline a reusable *block's content* into this skill (reuse a passage; e.g. the shared PR-description rules). Change the block once → all includers sync.
> - **`{{Alias}}`** — an unambiguous, compiler-resolved *pointer* in the body to an imported skill/reference (reuse a *reference to a unit*). "Use the code review md" is ambiguous prose; `{{CodeReview}}` resolves to exactly the imported unit, and a typo / missing import is a build error.
>
> **Scope note**: Build-time only. The consuming agent's runtime lazy-load / progressive disclosure is unchanged — this feature changes how skills are *authored and assembled into a bundle*, and guarantees the bundle is complete, conformant, and link-checked before it ships.
>
> **Relationship to today's system**: The existing installer in `~/Desktop/repos/awesome-frontend-skills` (`bin/skillz`, `catalog/profiles/*`, whole-dir `cp -R`, hand-maintained whitelists) is prior art being **replaced**, cited only to motivate the problem. The new framework does not inherit its structure.

## Problem & Context

Sharing skills across many repos is unmanageable today because there is **no reuse mechanism** for skill content — only file copying governed by hand-maintained lists. Four failure classes, all silent:

1. **No reuse → copy-paste drift (the core pain).** When the same passage (PR rules, commit conventions, a self-test flow) belongs in several skills or several repos, the only option is to duplicate it. Pulling a shared skill "out" doesn't help, because nothing keeps the copies in sync. Editing the canonical intent means hand-editing every copy and missing some. There is no "change once, sync everywhere."
2. **No unambiguous cross-references.** A skill's prose refers to other skills/references in natural language ("use the code review md"); nothing resolves or verifies that the referent exists, so references rot silently.
3. **Membership by human memory.** Which repo gets which skill is a per-repo whitelist or whole-group inclusion; a new skill is silently missing from a repo until someone edits a list.
4. **No conformance & reference bloat.** Whole directories are copied (`cp -R`), so a repo gets references for other targets it never uses, and nothing checks a skill is well-formed or that a promised reference exists. Install succeeds; the bundle is wrong; it breaks at runtime with no signal.

The throughline: **reuse, cross-references, membership, and conformance all depend on human memory, and every failure is silent.** The compiler turns each into either deterministic assembly/resolution or a hard, loud build failure — the way a real bundler refuses to ship an unresolved import.

## Clarifications

### Session 2026-06-09

- Q: Framework boundary — extend the existing `bin/skillz` installer, or build something new? → A: **Greenfield framework + compiler that replaces the old installer/profile/group system.** Design from scratch for maintainability; do not inherit the old architecture.
- Q: How strict is the "規範" the compiler enforces? → A: **The compiler is also a validator**: it defines a skill schema and **fails the build** on any violation — "doesn't conform → doesn't compile."
- Q: Does compile write straight into agent dirs, or produce an inspectable artifact? → A: **Two-stage** — compile produces an independent, inspectable `dist/`-like artifact (per target+agent), diffable in CI; **install** is a separate step that places it.
- Q: Where does the list of valid targets come from? → A: **A single authoritative framework config** (Vite-config-like) enumerates targets (each with its entry); entry mounts and reference filenames validate against it.
- Q: How are skills shared and assembled, and is dependency resolution in scope? → A: **An assembly + dependency model.** Cross-repo skill dependencies (npm-style peer deps) are pulled in and installed alongside; this supersedes the original "no dependency resolution" / "no external-source management" non-goals.
- Q: What is the *defining* authoring difference vs hand-written markdown, and how are in-body references written? → A: **Reusable-block assembly is the defining differentiator.** Two distinct, compiler-resolved affordances: **`@include <block>`** inlines a reusable block's *content* (change once → all includers sync); **`{{Alias}}`** is an unambiguous *pointer* to an imported skill/reference (disambiguation; "use the code review md" is unresolvable prose, `{{CodeReview}}` is not). Both are statically resolved — an unresolved `@include` or `{{}}` is a build error. (An earlier idea to treat `{{}}` purely as a JS-like evaluated symbol was reframed: its real job is disambiguation + link-checking, alongside `@include` for content reuse.)
- Q: How is membership expressed — each skill declaring its targets (`appliesTo` push), or each target pulling skills from an entry? → A: **Entry-point pull (like a webpack entry / React `app.js`).** Each target has an entry that mounts its top-level skills; the compiler traverses the `@include`/import closure from the entry, so dependencies are pulled in automatically and unreached skills are dead code (orphan warning). This replaces per-skill `appliesTo` as the primary membership mechanism (`appliesTo` may survive only as an opt-in "auto-mount on all targets"). Bonus: pull dissolves the earlier membership-vs-dependency precedence ambiguity — an imported dependency is simply pulled into whatever bundle's entry reaches it.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Reusable blocks: change once, sync everywhere (Priority: P1)

As a skill author, I extract a passage that recurs across several skills (PR rules, commit conventions, a self-test flow) into a **block**, and each skill pulls it in with `@include`. When the passage changes, I edit the block once; every skill that includes it re-compiles in sync — I never touch the includers.

**Why this priority**: This is the framework's reason to exist — the one thing hand-written markdown cannot do. Without it the framework is just a linter over conventions and isn't worth building. Everything else (membership, validation, artifacts) supports delivering this safely.

**Independent Test**: Create a block included by two skills. Change the block's content, recompile — both skills' artifacts reflect the new content, and neither includer file was hand-edited.

**Acceptance Scenarios**:

1. **Given** block `B` is `@include`d by skills `X` and `Y`, **When** compiling, **Then** `X` and `Y` artifacts both contain `B`'s current content (inlined).
2. **Given** `B`'s content is changed, **When** recompiling, **Then** `X` and `Y` update in sync with zero edits to `X` or `Y`.
3. **Given** a skill `@include`s a block that does not exist, **When** compiling, **Then** the build fails naming the skill and the missing block.
4. **Given** an `@include` cycle (A includes B includes A), **When** compiling, **Then** the build fails naming the cycle.
5. **Given** a block that no skill includes, **When** compiling, **Then** an unused-block warning is emitted (non-fatal).

---

### User Story 2 - Unambiguous references to imported units (Priority: P1)

As a skill author, when my prose needs to point at another skill or reference (not inline its content), I declare it in `imports` under an alias and write `{{Alias}}` in the body. The compiler resolves `{{Alias}}` to exactly that unit — including units imported **from another repo** — and fails if it's undeclared or unresolvable.

**Why this priority**: Resolves the "which md do you mean?" ambiguity and is the second reuse affordance (reference, vs `@include`'s content). It also carries cross-repo dependency: the unit a `{{Alias}}` points at may be pulled in as a peer dependency.

**Independent Test**: Import skill `code-review` as `{{CodeReview}}` and reference it in the body. Compile — it resolves and (if it's a skill dependency) `code-review` is bundled. Reference `{{Foo}}` with no import — the build fails naming the undefined alias.

**Acceptance Scenarios**:

1. **Given** `imports: { CodeReview: ../code-review }` and a body reference `{{CodeReview}}`, **When** compiling, **Then** the marker resolves to that unit and (for a skill dependency) it is included in the bundle.
2. **Given** an imported unit lives in **another repo/source**, **When** compiling, **Then** it is fetched/resolved and installed alongside (peer-dependency semantics).
3. **Given** a body reference `{{Foo}}` with no matching import, **When** compiling, **Then** the build fails naming the skill and the undefined alias.
4. **Given** the same dependency is pulled via multiple paths (diamond), **When** compiling, **Then** it is included exactly once (deduplicated).
5. **Given** the same dependency is required at conflicting versions, **When** compiling, **Then** the conflict is surfaced (fail by default), never silently resolved.
6. **Given** an import declared but never referenced (no `{{}}`, no `@include`), **When** compiling, **Then** an unused-import warning is emitted.

---

### User Story 3 - Per-target bundles via entry-point pull (Priority: P1)

As a maintainer, each target has an **entry** that mounts the top-level skills that target wants; compiling a target traverses the `@include`/import closure from its entry, pulling dependencies in automatically. Skills no entry reaches are dead code, surfaced as orphans — never a hand-maintained whitelist, never a silent drop.

**Why this priority**: Replaces the brittle whitelist with a single readable entry per target (like a webpack entry / React `app.js`), and is the membership axis, orthogonal to reuse/dependency. Required for any real multi-repo install.

**Independent Test**: Mount `frontend-coding` in the `scm` entry but not the `admin` entry. Compile `scm` — present. Compile `admin` — absent. A skill reached by neither entry nor any import is reported as an orphan.

**Acceptance Scenarios**:

1. **Given** the `admin` entry mounts `frontend-coding` and `git`, **When** compiling `admin`, **Then** the bundle contains those plus their transitive `@include`/import closure, and nothing not reachable from the entry.
2. **Given** a skill imported by a mounted skill (even from another repo), **When** compiling, **Then** it is pulled into the bundle automatically without being listed in the entry.
3. **Given** an entry mounts a skill that does not exist, **When** compiling, **Then** the build fails naming the entry and the missing skill.
4. **Given** a skill reached by no entry and no import, **When** compiling, **Then** an orphan (unused-skill) warning is emitted.
5. **Given** a skill in a target's bundle with a per-target reference `references/<target>.md`, **When** compiling that target, **Then** the bundle keeps `references/<target>.md` and drops other targets' reference files.

---

### User Story 4 - Conformance is enforced by the compiler (Priority: P2)

As a maintainer, a skill that violates the framework schema — missing required frontmatter, malformed import, bad reference naming, unresolved `@include`/`{{}}`, or a promised-but-missing per-target reference — fails the build with a message naming the skill and the rule, instead of shipping broken.

**Why this priority**: This is what makes it a framework rather than a convention; it keeps the reuse/membership axes honest over time. Depends on schema (config) + the resolution graph, so P2.

**Independent Test**: Remove a required frontmatter field (or break an `@include`) and compile — build fails naming the rule. Fix it — passes.

**Acceptance Scenarios**:

1. **Given** a skill missing a required frontmatter field, **When** compiling, **Then** the build fails naming the field.
2. **Given** a skill with `referenceMode: per-target` applying to `scm` but no `references/scm.md`, **When** compiling `scm`, **Then** the build fails naming the missing reference; **and** for a target it does not apply to, no such assertion fires.
3. **Given** a reference filename whose stem is neither a registered target nor a declared shared reference, **When** compiling, **Then** it is reported (stray/typo'd files can't hide).

---

### User Story 5 - Inspectable, deterministic build artifact (Priority: P2)

As a maintainer, compiling produces an independent `dist`-like artifact per `(target, agent)` I can inspect and diff before install; install is a separate step; identical inputs yield identical artifacts so CI fails on drift.

**Why this priority**: The Vite-like property that makes the whole system testable/CI-gateable; layers on top of assembly/resolution, so P2.

**Independent Test**: Compile twice from identical inputs → byte-identical artifacts; inspect without installing; run a separate install step.

**Acceptance Scenarios**:

1. **Given** identical `(catalog, config, target, agent)`, **When** compiling twice, **Then** the artifacts are byte-identical.
2. **Given** a successful compile, **When** I inspect the output, **Then** there is a standalone artifact directory readable/diffable without being installed.
3. **Given** a compiled artifact, **When** I run install, **Then** it is placed into the agent's directory in the agent's format, runtime loading unchanged.

---

### Edge Cases

- **Unresolved `@include`**: included block missing → hard fail naming skill + block.
- **`@include` cycle** / **dependency cycle**: hard fail naming the cycle.
- **Undefined `{{Alias}}`**: body references an alias with no import → hard fail.
- **Cross-repo source unavailable / version conflict**: hard fail naming importer, unit, source; conflicting versions fail by default (no silent pick).
- **Diamond reuse**: a block inlined via two paths, or a dependency pulled twice → block content inlines per includer as authored; a skill dependency dedups to one bundled copy.
- **Membership × dependency interaction**: dissolved by the pull model — an imported dependency is automatically pulled into whatever bundle's entry transitively reaches it; there is no `appliesTo` conflict to arbitrate.
- **Entry mounts a missing skill** → hard fail naming entry + skill.
- **Orphan skill** (reached by no entry and no import) → warning, not silent.
- **Unknown target** in an entry / reference filename → validated against config → hard fail.
- **Empty entry**: a target whose entry mounts nothing yields an empty-but-valid bundle.
- **Shared reference vs target-name collision**: a shared reference whose stem later matches a newly registered target stops being shared — the explicit config registry makes this detectable.
- **Determinism**: no dependence on wall-clock, filesystem order, or network nondeterminism in the artifact.
- **Self-containment**: a non-empty bundle must contain zero unresolved `@include`/`{{}}`/dependencies; an empty target yields an empty-but-valid artifact.

## Requirements *(mandatory)*

### Functional Requirements

**Reusable blocks — assembly (`@include`)**

- **FR-001**: A reusable block MUST be authorable once and `@include`-able by many skills; compiling MUST inline the block's content into each including skill's output.
- **FR-002**: Editing a block once MUST propagate to every skill that includes it on recompile, with no edits to the including skills (single source of truth; no copy-paste).
- **FR-003**: An `@include` of a non-existent block MUST fail the build, naming the skill and the missing block.
- **FR-004**: The compiler MUST detect and fail on `@include` cycles, naming the cycle.
- **FR-005**: The compiler MUST warn on a block that no skill includes (non-fatal).

**References & dependencies — pointers (`{{Alias}}`) and imports**

- **FR-006**: A skill MUST be able to declare named imports of other skills/references (alias → path) in frontmatter, and reference them in the body via `{{Alias}}`.
- **FR-007**: The compiler MUST statically resolve every `{{Alias}}` to a declared import; an undefined alias MUST fail the build, naming the skill and the alias.
- **FR-008**: Imports MUST be able to cross repo/source boundaries; importing a skill from another repo MUST pull it (and its dependencies) into the bundle and install it alongside (peer-dependency semantics).
- **FR-009**: A compiled non-empty bundle MUST contain zero unresolved references/dependencies; any unresolvable import MUST fail the build, naming importer, import, and source.
- **FR-010**: When the same dependency is pulled via multiple paths (diamond), the compiler MUST include it exactly once (deduplicate).
- **FR-011**: When the same dependency is required at conflicting versions, the compiler MUST surface the conflict (fail by default), never silently choose.
- **FR-012**: The compiler MUST warn on an import that is never referenced (`{{}}` or `@include`).

**Membership (per-target entry-point pull)**

- **FR-013**: Each target MUST have an entry that mounts its top-level skills; the compiler MUST derive a target's bundle by traversing the `@include`/import dependency closure from that entry (pull), with no hand-maintained whitelist as source of truth.
- **FR-014**: Dependencies reached via `@include`/import from a mounted skill MUST be pulled into the bundle automatically, without being listed in the entry (including cross-repo dependencies).
- **FR-014a**: An entry that mounts a non-existent skill MUST fail the build, naming the entry and the missing skill.
- **FR-014b**: A skill/block/reference reachable from no entry and no import MUST be reported as an orphan (unused) warning, so dead skills are visible rather than silently shipped or missing.
- **FR-014c**: A skill MAY opt into auto-mounting on all targets (the equivalent of "applies to all"); entry mounting remains the primary mechanism.
- **FR-015**: For each skill in a target's bundle, the compiler MUST retain `references/<target>.md`, retain shared references, and drop other targets' reference files.

**Conformance (the framework schema)**

- **FR-016**: The framework MUST define a skill schema (required frontmatter, reference naming, structure, `@include`/import/`{{}}` forms); the compiler MUST validate every skill and fail on any violation, naming the skill and the rule.
- **FR-017**: A skill MUST be able to declare `referenceMode` (`per-target` | `optional` | `none`); for `per-target`, a missing `references/<target>.md` for an applicable target MUST fail the build; the assertion MUST NOT fire for non-applicable targets.
- **FR-018**: The compiler MUST report stray/unrecognized files rather than silently ignoring them.

**Configuration & targets**

- **FR-019**: A single authoritative framework config MUST enumerate valid targets (each with its entry); entry mounts and per-target reference filenames MUST validate against the registry.
- **FR-020**: The config MUST be the explicit place targets (and per-target defaults such as default agent) are added — the target registry is not inferred.

**Build artifact, determinism & install**

- **FR-021**: Compilation MUST take `(catalog, config, target, agent)` and produce an independent, inspectable build artifact for that combination, separate from installation.
- **FR-022**: Compilation MUST be deterministic — identical inputs produce a byte-identical artifact — and runnable non-interactively in CI, exiting non-zero on any hard failure.
- **FR-023**: Installation MUST be a separate step placing a compiled artifact into the agent's destination in the agent's output format.
- **FR-024**: The framework MUST support multiple agent output formats (e.g. claude, codex, cursor, …) selectable per build without changing skill source.
- **FR-025**: The emitted bundle MUST remain loadable by the consuming agent exactly as before — runtime lazy-load / progressive disclosure unchanged.

**Diagnostics**

- **FR-026**: Every hard failure MUST produce an actionable message identifying the skill/block/alias/target and the rule violated.
- **FR-027**: The compiler MUST surface non-fatal anomalies (unused block/import, orphan skills, empty entry, name collisions) as visible warnings.

### Key Entities *(include if feature involves data)*

- **Skill**: An authored unit (`SKILL.md` conforming to the schema) plus optional `references/`, `@include`s, and named imports. A node of the dependency graph; it enters a target's bundle by being mounted in that target's entry or pulled in as a dependency.
- **Block**: A reusable content fragment authored once and inlined into skills via `@include` (the "component/partial"). The mechanism behind change-once-sync-everywhere.
- **Reference**: A `references/<name>.md` belonging to a skill — per-target when `<name>` is a registered target (prunable), shared otherwise (always retained).
- **Named import / alias**: A frontmatter `alias → path` binding to another skill/reference (possibly cross-repo), referenced in the body via `{{Alias}}` and statically resolved.
- **Skill-dependency edge**: A dependency from a skill to another skill, possibly across a repo/source boundary (peer dependency); the dependency is bundled and installed alongside.
- **Dependency graph / closure**: The transitive set reachable via `@include`/imports; must be acyclic and fully resolvable.
- **Source / repo**: An origin a skill/dependency resolves from, including external repos.
- **Target**: A compilation destination (e.g. `admin`, `shop`, `scm`, `sl-feature`), enumerated in config, each with an **entry** that mounts its top-level skills; drives membership (via entry pull) + reference selection.
- **Entry point**: A target's mount list of top-level skills; the compiler traverses the `@include`/import closure from it to build that target's bundle (webpack-entry / React-`app.js` analog). Skills no entry reaches are orphans.
- **Agent**: An output format/destination (claude, codex, cursor, …), orthogonal to target.
- **Framework config**: The single authoritative file defining the target registry (each target + its entry) + per-target defaults; validation authority for entry mounts and reference names.
- **Schema**: The framework's definition of a conformant skill, enforced by the compiler.
- **Build artifact (`dist`-like)**: The inspectable, deterministic, self-contained output for one `(target, agent)`, produced before and independently of install.
- **Compiler pipeline**: `resolve (declarations + @include graph + dependency graph, incl. cross-repo) → validate (schema, resolve every @include/{{}}) → shake (per target) → assemble (inline blocks, collect dependencies → self-contained) → emit (artifact) → install (separate)`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A passage shared by N skills is authored once; changing it requires editing **1** file and recompiling, after which all N skills reflect the change — vs N hand-edits today, with misses undetected.
- **SC-002**: 100% of compiled non-empty bundles are self-contained — zero unresolved `@include`, `{{}}`, or dependencies in any shipped artifact.
- **SC-003**: A skill can reuse another skill/reference/block by one declaration; the dependency is pulled in automatically with no manual copying, including across repos.
- **SC-004**: Each target's membership is readable from a single entry; a skill reachable from no entry and no import is flagged as an orphan — so neither accidental inclusion nor omission is silent (vs a brittle hand-maintained whitelist).
- **SC-005**: 100% of schema/resolution violations (missing field, bad import, unresolved `@include`/`{{}}`, missing required per-target reference, stray file) fail at compile time; zero reach a shipped artifact.
- **SC-006**: For every compiled bundle, the count of foreign-target reference files is 0.
- **SC-007**: Compilation is deterministic — identical inputs yield a byte-identical artifact on 100% of runs — enabling a CI check that fails on drift.
- **SC-008**: A maintainer can answer "what is in this bundle and why (which includes, imports, target)" from declarations + config alone, for 100% of bundles.
- **SC-009**: The skills each existing repo receives under the framework is a verified superset-or-equal of today's (no migration regression), provable by diffing old vs new membership.

## Assumptions

- **Greenfield replacement**: The framework + compiler replaces the existing `bin/skillz` / profile / group / `cp -R` installer; the old system is prior art / migration source only. Designed from scratch for maintainability (user direction).
- **Two reuse affordances, distinct roles**: `@include <block>` inlines a block's *content* (reuse a passage; change-once-sync-everywhere — the defining differentiator); `{{Alias}}` is a resolved *pointer* to an imported skill/reference (reuse a reference; disambiguation + link-check). Both build on the frontmatter `imports`/block declarations and are statically resolved.
- **Three orthogonal concerns**: reuse (`@include`/`{{}}`/imports), membership (entry-point pull), and conformance (schema). Bundling = from a target's entry, traverse the dependency closure (assemble blocks, resolve imports incl. cross-repo), deduplicate, prune non-target references, emit.
- **Cross-repo dependency resolution & versioning are in scope** (supersedes original non-goals). The exact source-pinning mechanism (reuse/replace/extend the existing `skills-lock.json`) is a planning decision; the requirement is deterministic resolution and loud conflict failure.
- **Syntax is proposed, not final**: `@include <block>`, `{{Alias}}`, and the frontmatter `imports` map are illustrative; exact spelling (`@include` vs a component tag; `{{}}` vs `@`/`[[]]`) is a planning detail. The requirements (inline-reuse, resolved-pointer, static link-check) are fixed.
- **Unit kind & placement**: what an imported/included unit becomes in the artifact is determined by the unit's own kind, not the import site — a *block* inlines into its includer; a *skill* dependency lands as a top-level skill (deduped); a *reference* lands under its host skill. (Detail for planning.)
- **Membership = entry-point pull** (replaces per-skill `appliesTo`): each target's entry mounts top-level skills; dependencies are pulled in automatically; unreached skills are warned as orphans. `appliesTo` survives only as an optional "auto-mount on all targets" opt-in. **Shared reference** = a reference whose stem is not a registered target.
- **Two-stage build** (compile → artifact, then install); CI operates on the artifact. **Runtime unchanged** (progressive disclosure out of scope).
- **Environment**: Seed targets `admin`/`shop`/`scm`/`sl-feature` and agents `claude`/`codex`/`cursor`/`gemini`/`copilot` populate the config but the framework is not limited to them. This `cite` repo holds the planning artifacts; the framework owns skill authoring + compilation going forward.
