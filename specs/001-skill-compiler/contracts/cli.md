# Contract: `skillc` CLI

The compiler's external interface is a CLI. Four subcommands: compile (`build`) is fully
separate from place (`install`) per the two-stage decision (FR-021/023); `check` is the
validate-only gate; `why` is the read-only reverse-dependency query. Non-interactive,
exits non-zero on any hard failure (FR-022).

## `skillc build`

Compile one or more `(target, agent)` combinations into inspectable artifacts.

```
skillc build --target <T> --agent <A> [--catalog <dir>] [--config <file>] [--out <dir>]
             [--all-targets] [--all-agents] [--locked] [--frozen]
```

| Flag | Required | Meaning |
|---|---|---|
| `--target <T>` | one of target/all-targets | Target name; must exist in config (FR-019) else hard fail. |
| `--agent <A>` | one of agent/all-agents | Agent format; must exist in config (FR-024). |
| `--catalog <dir>` | no (default `.`) | Root of the skill catalog. |
| `--config <file>` | no (default `skillc.config.{yaml,…}`) | Framework config (FR-019). |
| `--out <dir>` | no (default `dist/`) | Artifact output root. |
| `--all-targets` / `--all-agents` | no | Cartesian build over the registry. Duplicate diagnostic lines across combos are printed once, with a suppressed-count summary. |
| `--locked` | no | CI integrity: every used cross-repo source must carry a `contentHash` pin. A present pin is always verified against the mirror bytes (drift = `error[source/unavailable]`), flag or not. |
| `--frozen` | no | Use lockfile only; never touch network. |

**Output**: one artifact directory per `(target, agent)` under `--out` (see
[artifact-layout.md](./artifact-layout.md)). Warnings to stderr; artifact bytes deterministic.

**Exit codes**: `0` success (warnings allowed); `1` hard failure (any rule in
[skill-schema.md](./skill-schema.md) or unresolved `@include`/`{{}}`/dependency, missing entry
skill, version conflict, missing required reference, cycle); `2` usage error (unknown
target/agent, missing config).

**Hard-failure messages** MUST name the offending unit/alias/target and the rule (FR-026), e.g.:
```
error[include/missing]: skill `frontend-coding` @includes block `pr-rules` which does not exist
error[marker/undefined]: skill `git` references {{CodeReview}} but no import declares `CodeReview`
error[entry/missing]: target `admin` entry mounts skill `does-not-exist`
error[dep/version-conflict]: `code-review` required at 1.2 (via admin) and 2.0 (via scm)
error[cycle]: @include cycle: a → b → a
```

## `skillc check`

Validate the catalog without writing any artifact (CI / pre-commit gate).

```
skillc check [--target <T>] [--catalog <dir>] [--config <file>] [--locked] [--frozen]
```

Runs `parse → graph → resolve → validate` once plus the per-target `shake` + required-
reference checks for every registered target (or just `--target <T>`). Exact-duplicate
diagnostics from per-target re-analysis are deduped. Exit codes match `build`; nothing is
written to disk.

## `skillc why`

Explain a unit's reverse dependencies, computed from the compiler's own resolution.

```
skillc why <id> [--catalog <dir>] [--config <file>]
```

`<id>` may be a block, a catalog skill, or an external (cross-repo pulled) skill. Reports:
direct includers (skills + blocks), transitive shipping skills, importing skills with their
alias, and per-target membership with reasons (`mounted` / `pulled-by:<skill>`). Unknown id
→ exit `2` with the catalog inventory. Read-only.

## `skillc install`

Place a previously-built artifact into an agent destination (separate step, FR-023).

```
skillc install --artifact <dir> --dest <dir> [--agent <A>]
```

| Flag | Required | Meaning |
|---|---|---|
| `--artifact <dir>` | yes | A directory produced by `build`. |
| `--dest <dir>` | yes | Agent's destination root. |
| `--agent <A>` | no | Override/confirm output format; defaults to the artifact's recorded agent. |

**Behavior**: copies/places the artifact in the agent's output format; runtime loading of the
placed skills is unchanged (FR-025). Idempotent: re-installing identical artifact bytes yields
an identical destination.

**Pruning**: install records placed skill ids in `<dest>/.skillc-receipt.json` keyed by
`<target>/<agent>`. A later install for the same key removes skills the previous install
placed that the new artifact dropped. Ids never recorded in the receipt (hand-authored
skills) are never removed; ids still owned by another key into the same destination are
kept. Receipt entries are re-validated as safe path segments before any removal.

## Determinism contract (applies to `build`)

- Same `(catalog, config, target, agent)` + same `skills-lock.json` ⇒ byte-identical artifact
  (FR-022/SC-007). A CI job runs `build --locked --frozen` twice (or builds + diffs against a
  committed artifact) and fails on any byte difference.
