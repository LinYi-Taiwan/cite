# Quickstart end-to-end results (T049)

Run: 2026-06-09T13:50:30Z | binary: target/release/skillc

## Scenario 1 — Reuse: change once, sync everywhere (US1)
```
warning[block/unused]: block `experimental` is included by no skill
skillc: built /tmp/qs/s1/admin/claude
exit=0
-- pr-rules text present in BOTH includers:
/tmp/qs/s1/admin/claude/skills/git/SKILL.md
/tmp/qs/s1/admin/claude/skills/frontend-coding/SKILL.md
```

## Scenario 2 — Unresolved @include fails loudly
```
error[include/missing]: skill `frontend-coding` @includes block `does-not-exist` which does not exist
exit=1
```

## Scenario 3 — Cross-repo dependency pulled + deduped (US2)
```
warning[import/unused]: skill `reviewer` declares import `LegacyHelper` but never references it
skillc: built /tmp/qs/s3/scm/claude
exit=0
-- manifest skills (id : reason):
   auditor : mounted
   code-review : pulled-by:auditor
   reviewer : mounted
```

## Scenario 4 — Entry-point membership + orphans (US3)
```
warning[skill/orphan]: skill `orphan-skill` is reached by no entry or import
skillc: built /tmp/qs/s4/scm/claude
scm exit=0
warning[skill/orphan]: skill `orphan-skill` is reached by no entry or import
skillc: built /tmp/qs/s4/admin/claude
admin exit=0
-- frontend-coding present for scm? 1
-- frontend-coding present for admin? 0
```

## Scenario 5 — Per-target reference pruning (US3)
```
-- scm build, frontend-coding references:
glossary.md
scm.md
(foreign-target admin.md must be absent above)
```

## Scenario 6 — Determinism / CI drift gate (US5)
```
BYTE-IDENTICAL (exit=0)
```

## Scenario 7 — Inspect, then install (US5)
```
-- manifest readable pre-install:
{
  "target": "admin",
  "agent": "claude",
  "skills": [
install exit=0
-- installed tree:
/tmp/qs/agent-home/skills/frontend-coding/SKILL.md
/tmp/qs/agent-home/skills/git/SKILL.md
re-install exit=0 (idempotent)
```
