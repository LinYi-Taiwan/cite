# skillc — the Skill Compiler

A framework + compiler for authoring AI-agent skills whose defining purpose is
**reuse without copy-paste**. It compiles a catalog of skills, reusable blocks, and
references into self-contained, inspectable, byte-deterministic artifacts per
`(target, agent)`, driven by an entry-point pull model (a webpack/Vite analog) over an
`@include` (content reuse) + `{{Alias}}` (resolved pointer) authoring layer with
npm-style cross-repo dependency resolution.

The compiler is also the validator: non-conforming or unresolvable input does not compile.

A complete runnable example lives in [`examples/team/`](examples/team/) — two targets,
a shared block, per-target references, committed `dist/`.

> The installed binary is **`cite`**. (`skillc` is the framework/crate name; the command
> you run is `cite`.)

## Use cases

Reach for `cite` when skill/prompt content is being copy-pasted across repos, agents, or
projects and drifting out of sync. Concretely:

- **One standard, many agents.** Author a coding-standard skill once and ship it to
  Claude, Codex, Cursor, Gemini, and Copilot — each gets the layout it expects, from the
  same source (`cite build --all-agents`, then `cite install`).
- **Change-once, sync-everywhere shared content.** A commit convention, review checklist,
  or glossary lives in one block; every skill pulls it with `@include`. Edit the block,
  recompile, all consumers update — no hand-copying.
- **Per-project variants of the same skill.** One skill, different notes per project via
  per-target references — `cite` keeps only the current target's reference and prunes the rest.
- **Cross-repo reuse.** Import a skill or block from another catalog repo (`imports:` +
  `skills-lock.json`); it's pulled into the bundle and deduped, npm-style.
- **Review what agents actually receive.** Builds are byte-deterministic, so a committed
  `dist/` diffs like a lockfile in PR review — you see exactly what every agent will load.
- **Know the blast radius before editing.** `cite why <id>` shows every skill/target that
  depends on a block before you touch it; `cite graph` renders the whole catalog.

## Quick start

Try it against the bundled example (no setup beyond the binary):

```bash
# Compile the example's `web` target for the `claude` agent
cite build --target web --agent claude --catalog examples/team --out /tmp/cite-demo

# The @include block is already inlined into the artifact, {{Alias}} pointers resolved
cat /tmp/cite-demo/web/claude/skills/frontend-coding/SKILL.md

# Validate the whole catalog without writing anything (the CI / pre-commit gate)
cite check --catalog examples/team

# Who depends on the shared block?
cite why commit-format --catalog examples/team
```

Then point `--catalog` at your own directory laid out as below, or copy `examples/team/`
as a starting skeleton.

## Build

```bash
cargo build --release      # -> target/release/cite
```

Toolchain: Rust (stable). The crates declare an MSRV of 1.83; current dependency
versions in the registry require a newer stable to build (the build here uses stable
1.96). Pin older dependency versions if a strict 1.83 build is required.

## Catalog layout

```text
<catalog>/
├── skillc.config.yaml        # the framework config (target registry, agents, sharedReferences)
├── skills-lock.json          # pins cross-repo sources (optional)
├── skills/<id>/SKILL.md      # one skill per directory
│              references/<stem>.md
├── blocks/<id>.md            # reusable @include blocks
└── sources/<id>/...          # pre-cloned cross-repo source mirrors (optional)
```

### `SKILL.md`

```markdown
---
id: frontend-coding              # required, unique slug
name: Frontend Coding Standards  # required
description: ...                 # required (used for triggering)
referenceMode: per-target        # per-target | optional | none
appliesTo: all                   # optional: auto-mount on every target
imports:
  CodeReview: ../code-review            # local path
  SharedGit:  shared-skills:git@1.2     # cross-repo: <sourceId>:<subpath>[@version]
---

Body markdown.

@include pr-rules                # inline a block's CONTENT (change-once-sync-everywhere)

When reviewing, use {{CodeReview}} for the checklist.   # resolved pointer
```

- **`@include <block>`** inlines the block's content verbatim into every includer; editing
  the block once re-syncs all includers on recompile. Missing block → `error[include/missing]`;
  cycle → `error[cycle]`; a block no skill includes → `warning[block/unused]`.
- **`{{Alias}}`** resolves to a declared import (including cross-repo units, which are pulled
  into the bundle and deduped) and **renders as a relative markdown link** the reading agent
  can follow — `[code-review](../code-review/SKILL.md)` — not a bare name. Undefined →
  `error[marker/undefined]`; declared-but-unused import → `warning[import/unused]`. `\{{` is
  a literal escape.

### `referenceMode` semantics

| Value | Meaning |
|---|---|
| `per-target` | `references/<target>.md` is **required** for every applicable target — missing → `error[reference/missing]`. |
| `optional` (default) | References emitted when present; nothing required. |
| `none` | Same emission as `optional`; declares the skill doesn't rely on references. |

Reference filenames must be a registered target name or a `sharedReferences` stem —
anything else is `error[reference/stray]`. Shared reference files live next to the skill's
other references (`skills/<id>/references/<stem>.md`); "shared" refers to the *stem* being
target-independent (survives per-target pruning), not to a shared directory.

## `skillc.config.yaml`

```yaml
targets:
  admin:
    entry:
      mounts: [frontend-coding, git]   # top-level skills; each MUST exist
    defaultAgent: claude               # optional
  scm:
    entry:
      mounts: [git, frontend-coding]
agents: [claude, codex, cursor, gemini, copilot]
sharedReferences: [glossary, conventions]
```

Membership is **entry-point pull**: each target's `entry.mounts` seed a closure over
`@include`/import edges. A skill reached by no entry and no import is an
`warning[skill/orphan]`. Per-target references (`references/<target>.md`) are retained only
when compiling that target; foreign-target references are pruned.

## Commands

### `cite build`

```bash
cite build --target <T> --agent <A> [--catalog <dir>] [--config <file>] [--out <dir>]
             [--all-targets] [--all-agents] [--locked] [--frozen]
```

| Flag | Meaning |
|---|---|
| `--target <T>` | Target name (must exist in config). Required unless `--all-targets`. |
| `--agent <A>` | Agent format (must exist in config). Required unless `--all-agents`. |
| `--catalog <dir>` | Catalog root (default `.`). |
| `--config <file>` | Framework config (default `skillc.config.yaml` in the catalog). |
| `--out <dir>` | Artifact output root (default `dist`). |
| `--all-targets` / `--all-agents` | Cartesian build over the registry (duplicate diagnostic lines across combos are printed once). |
| `--locked` | CI integrity: every used cross-repo source must carry a `contentHash` pin in `skills-lock.json`. A present pin is **always** verified against the mirror's bytes (drift = hard error), with or without the flag. |
| `--frozen` | Use the lockfile only; never touch the network. |

Produces one artifact directory per `(target, agent)`:

```text
dist/<target>/<agent>/
├── manifest.json          # what's in the bundle and why
└── skills/<id>/SKILL.md    # @includes inlined, {{Alias}} resolved
                references/<stem>.md
```

**Exit codes**: `0` success (warnings allowed), `1` hard failure, `2` usage error.

### `cite check`

```bash
cite check [--target <T>] [--catalog <dir>] [--config <file>] [--locked] [--frozen]
```

Validates the whole catalog — schema, includes, markers, imports, entries, per-target
references — **without writing any artifact**. Default scope is every registered target.
This is the CI / pre-commit gate; exit codes match `build`.

### `cite why`

```bash
cite why <skill-or-block-id> [--catalog <dir>] [--config <file>]
```

The reverse-dependency query — answers what a maintainer must know before touching a unit:

```text
$ cite why pr-rules
block `pr-rules`
  directly included by skills: frontend-coding, git
  directly included by blocks: (none)
  transitively inlined into skills: frontend-coding, git
  ships in targets:
    admin: frontend-coding (mounted), git (mounted)
    scm: git (mounted)
```

Works for blocks, catalog skills, and external (cross-repo pulled) skills. The answer is
computed from the compiler's own resolution — not a text grep — so it matches what `build`
ships. Unknown ids exit `2` and print the catalog inventory.

### `cite graph`

```bash
cite graph [--format html|json|dot|mermaid] [--target <T>] [--focus <id>] [--depth <N>]
             [--out <file>] [--open] [--catalog <dir>] [--config <file>]
```

The whole-catalog dependency graph: every target / skill / block / reference as nodes,
every `mounts` / `imports` / `includes` / `hosts` edge between them. Layers are derived
from structure (mounted ⇒ template, has imports ⇒ organism, none ⇒ molecule, block ⇒
atom); cross-repo pulled units keep their derived layer and carry `external: true`.
Health flags (`orphan` skills, `unused` blocks, `stray` references) are attached to the
nodes, so the graph doubles as a catalog health dashboard.

The default `html` format writes a self-contained viewer to `<catalog>/dist/graph.html`
(no server, no dependencies — `--open` launches it). It opens in **Explorer** mode:
search/pick one skill and walk an expandable dependency tree — what it imports (and
transitively), or flip direction to see what imports it — instead of staring at the whole
graph at once. The **Graph** tab keeps the layered swim-lane overview (per-target filter,
search, click-through detail panel). Both share the same click → inline `why` detail panel. `json` is the canonical machine-readable export (stdout by default) — diff it
across commits to see what a PR makes a target ship; `dot` pipes into graphviz and
`mermaid` pastes into a PR description. `--target` restricts to one bundle's closure;
`--focus <id> --depth <N>` cuts the neighborhood around one unit. Like `why`, the answer
is computed from the compiler's own resolution, so it matches what `build` ships.

### `cite install`

```bash
cite install --artifact dist/<target>/<agent> --dest <agent-dir> [--agent <A>]
```

Places a built artifact into an agent destination (separate, idempotent step).

**Stale-skill pruning**: install records what it placed in `.skillc-receipt.json` at the
destination root, keyed by `<target>/<agent>`. The next install for the same key removes
skills the previous install placed that the new artifact no longer carries — unmounting a
skill in the catalog actually retires it from the agent. Hand-authored skills (never in
the receipt) are not touched; a skill still owned by another target installed into the
same destination is kept.

All five registered agents load skills in the same Agent Skills layout the artifact
already uses (`skills/<id>/SKILL.md`), so `--dest` is just each agent's skills root:

| Agent | Project-level `--dest` | User-level `--dest` | Convention source |
|---|---|---|---|
| claude | `.claude` | `~/.claude` | Claude Code skills dirs (also read by Cursor, per its docs) |
| codex | `.agents` (repo root) | `~/.agents` | <https://developers.openai.com/codex/skills> |
| cursor | `.cursor` | — | <https://cursor.com/docs/context/skills> |
| gemini | `.gemini` (or `.agents` alias) | `~/.gemini` | <https://geminicli.com/docs/cli/skills/> |
| copilot | `.github` | — | GitHub Docs: Agent Skills at `.github/skills/<name>/SKILL.md` |

e.g. `cite install --artifact dist/web/cursor --dest path/to/repo/.cursor` yields
`path/to/repo/.cursor/skills/<id>/SKILL.md`.

## Team adoption

The intended workflow for a team running one skill catalog across N repos/machines:

1. **One catalog repo** holds skills/blocks/config; it is the single source of truth.
   Skill edits go through normal PR review there.
2. **Commit `dist/`** in the catalog repo. Builds are byte-deterministic, so the diff of
   `dist/` in a PR shows exactly what every agent will receive — reviewable like a
   lockfile. (Regenerate with `cite build --all-targets --all-agents` before pushing;
   never hand-edit `dist/`, rebuilds silently overwrite it.)
3. **CI gate** on the catalog repo:

   ```yaml
   - run: cite check --locked --frozen            # validates everything, writes nothing
   - run: cite build --all-targets --all-agents --locked --frozen
   - run: git diff --exit-code dist/                # committed dist must match the sources
   ```

4. **Consumers** (developer machines, other repos' CI) install from the committed
   artifact: `cite install --artifact dist/<target>/<agent> --dest <agent-root>`.
   Re-running install is idempotent and retires skills the catalog dropped (see the
   receipt above) — so "update" is just pull + install.
5. Before refactoring a shared block or removing a skill, run `cite why <id>` to see
   the blast radius.

## Determinism

Same `(catalog, config, target, agent)` + same `skills-lock.json` ⇒ byte-identical
artifact. The CI job (`.github/workflows/ci.yml`) builds twice with `--locked --frozen`
and `diff -r`s the outputs, failing on any byte difference.

## Implementation notes

- The pipeline is `parse → resolve → validate → shake → assemble → emit → install`
  (`crates/skillc-core/src/`), with a thin `clap` CLI in `crates/skillc`.
- Cross-repo sources are **pre-cloned**: the lockfile pins each source to a local mirror
  `path`. Network fetch via `gix` is a documented future extension (see
  `resolve/source.rs`) — keeping CI hermetic and builds deterministic.
