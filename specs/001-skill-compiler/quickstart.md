# Quickstart: Skill Compiler validation

Runnable scenarios that prove the framework's defining behaviors end-to-end. Each maps to spec
acceptance scenarios / success criteria. Details live in the contracts and data model — this is
a run/validation guide, not implementation.

## Prerequisites

- Rust toolchain (stable 1.83+), `cargo`.
- Build the CLI: `cargo build --release` → `target/release/skillc`.
- A fixture catalog (see `tests/fixtures/`) + a `skillc.config.yaml`
  ([framework-config.md](./contracts/framework-config.md)).

## Scenario 1 — Reuse: change once, sync everywhere (US1 / SC-001)

```bash
# A block `pr-rules` is @included by skills `frontend-coding` and `git`.
skillc build --target admin --agent claude --out dist
grep -rl "<pr-rules marker text>" dist/admin/claude/skills/frontend-coding dist/admin/claude/skills/git
# Edit ONLY blocks/pr-rules.md, rebuild:
skillc build --target admin --agent claude --out dist
```
**Expected**: both skills' `SKILL.md` contain the block's *new* content inlined; neither
includer file was hand-edited (FR-001/002). Editing 1 file updated N skills.

## Scenario 2 — Unresolved markers/blocks fail loudly (US1/US2 / SC-005)

```bash
skillc build --target admin --agent claude   # fixture has a bad @include and a bad {{Alias}}
echo "exit=$?"
```
**Expected**: `exit=1` with messages naming the rule and unit, e.g.
`error[include/missing]: skill ... @includes block ...` and
`error[marker/undefined]: ... {{Foo}} ... no import declares Foo` (FR-003/007/026).

## Scenario 3 — Cross-repo dependency pulled + deduped (US2 / SC-003)

```bash
skillc build --target scm --agent claude --out dist
cat dist/scm/claude/manifest.json | jq '.skills[] | {id, reason}'
```
**Expected**: a skill imported via `{{CodeReview}}` from another repo appears under
`skills/` with `reason: "pulled-by:..."`, pulled in automatically (FR-008), and appears
**once** even if reached by two paths (FR-010). A conflicting version fixture → `exit=1`
`error[dep/version-conflict]` (FR-011).

## Scenario 4 — Entry-point membership + orphans (US3 / SC-004)

```bash
skillc build --target scm   --agent claude --out dist   # entry mounts frontend-coding
skillc build --target admin --agent claude --out dist   # entry does NOT
ls dist/scm/claude/skills   | grep frontend-coding       # present
ls dist/admin/claude/skills | grep frontend-coding || true   # absent
```
**Expected**: present for `scm`, absent for `admin` (FR-013). A skill reached by no entry/import
emits `warning[skill/orphan]` on stderr (FR-014b). Mounting a missing skill → `exit=1`
`error[entry/missing]` (FR-014a).

## Scenario 5 — Per-target reference pruning (US3/US4 / SC-006)

```bash
skillc build --target admin --agent claude --out dist
ls dist/admin/claude/skills/*/references
```
**Expected**: each skill keeps `references/admin.md` + shared references and **no other
target's** reference file (FR-015); foreign-target reference count = 0 (SC-006). A
`referenceMode: per-target` skill missing `references/admin.md` → `exit=1`
`error[reference/missing]` (FR-017).

## Scenario 6 — Determinism / CI drift gate (US5 / SC-007)

```bash
skillc build --target admin --agent claude --out dist_a --locked --frozen
skillc build --target admin --agent claude --out dist_b --locked --frozen
diff -r dist_a dist_b && echo "BYTE-IDENTICAL"
```
**Expected**: `BYTE-IDENTICAL` (FR-022). The CI job runs this diff (or diffs against a
committed artifact) and fails on any difference.

## Scenario 7 — Inspect, then install (US5 / FR-023)

```bash
# Inspect the artifact WITHOUT installing:
cat dist/admin/claude/manifest.json
# Then place it (separate step):
skillc install --artifact dist/admin/claude --dest /tmp/agent-home
```
**Expected**: artifact is readable/diffable pre-install (FR-021); install places it in the
agent's format; re-running install with identical bytes is idempotent (FR-023/025).

## Coverage map

| Scenario | Spec |
|---|---|
| 1 | US1, FR-001/002, SC-001 |
| 2 | US1/US2, FR-003/007/026, SC-005 |
| 3 | US2, FR-008/010/011, SC-003 |
| 4 | US3, FR-013/014a/014b, SC-004 |
| 5 | US3/US4, FR-015/017, SC-006 |
| 6 | US5, FR-022, SC-007 |
| 7 | US5, FR-021/023/025 |
