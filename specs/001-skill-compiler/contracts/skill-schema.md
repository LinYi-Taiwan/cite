# Contract: Skill schema + authoring grammar

The framework's definition of a conformant skill. The compiler validates every skill against
this and **fails the build on any violation**, naming the skill and the rule (FR-016). Syntax
spellings are the spec's proposed forms (non-final per spec Assumptions); semantics are fixed.

## `SKILL.md` shape

```markdown
---
id: frontend-coding              # required, unique slug
name: Frontend Coding Standards  # required
description: ...                 # required (used for triggering)
referenceMode: per-target        # one of: per-target | optional | none  (FR-017)
appliesTo: all                   # optional opt-in auto-mount on all targets (FR-014c)
imports:                         # optional alias → unit (FR-006)
  CodeReview: ../code-review            # local path
  SharedGit:  shared-skills:git         # cross-repo: <sourceId>:<subpath> (FR-008)
---

Body markdown.

@include pr-rules                # inline a block's CONTENT (FR-001); `pr-rules` must exist (FR-003)

When reviewing, use {{CodeReview}} for the checklist.   # resolved pointer (FR-007)
```

### Frontmatter rules (hard fail unless noted)

| Field | Required | Rule |
|---|---|---|
| `id` | yes | Unique across catalog; missing/dup → fail naming the field/skill (FR-016). |
| `name` | yes | Non-empty. |
| `description` | yes | Non-empty. |
| `referenceMode` | no (default `optional`) | ∈ {per-target, optional, none} (FR-017). |
| `appliesTo` | no | `all` opts into auto-mount on every target (FR-014c). |
| `imports` | no | Map alias→ref; alias is a valid `{{}}` identifier; ref resolves (FR-006/009). |

## `@include <block>` — content reuse (the differentiator)

- A line directive: `@include <block-id>`.
- Inlines the block's **content** verbatim into this skill's output (FR-001); editing the block
  once propagates to all includers on recompile, no includer edits (FR-002).
- Missing block → `error[include/missing]` naming skill + block (FR-003).
- Cycle (A includes B includes A) → `error[cycle]` naming the cycle (FR-004).
- A block no skill includes → `warning[block/unused]` (FR-005, non-fatal).
- Diamond: a block inlined via two paths inlines its content per includer as authored
  (Edge Cases) — blocks are content, not deduped nodes.

## `{{Alias}}` — resolved pointer

- An inline marker referencing an `imports` alias.
- Every `{{Alias}}` MUST match a declared import; undefined → `error[marker/undefined]` naming
  skill + alias (FR-007).
- Resolves to exactly the imported unit, including cross-repo units (FR-008). A skill dependency
  it points at is pulled into the bundle (FR-008) and deduped (FR-010).
- An import declared but never used by `{{}}` or `@include` → `warning[import/unused]` (FR-012).
- Literal escape: `\{{` emits a literal `{{` (not treated as a marker).

## References (`references/<stem>.md`)

- `<stem>` is a **registered target** → per-target reference: retained only when compiling that
  target, dropped for others (FR-015); foreign-target reference count in any bundle = 0 (SC-006).
- `<stem>` ∈ config `sharedReferences` → shared: always retained.
- otherwise → **stray** file reported (FR-018) — typo'd/orphan files cannot hide.
- `referenceMode: per-target` skill applicable to target *T* but missing `references/<T>.md`
  → `error[reference/missing]` for *T*; the assertion does NOT fire for non-applicable targets
  (FR-017).

## Conformance summary (every one is a hard fail → exit 1)

1. Missing required frontmatter field (FR-016).
2. `@include` of a non-existent block (FR-003).
3. `@include` / dependency cycle (FR-004).
4. `{{Alias}}` with no matching import (FR-007).
5. Unresolvable import / cross-repo source unavailable (FR-009).
6. Same dependency at conflicting versions (FR-011).
7. Entry mounts a non-existent skill (FR-014a).
8. `per-target` reference missing for an applicable target (FR-017).
9. Stray/unrecognized reference filename (FR-018, reported).
10. Unknown target/agent vs config registry (FR-019).

## Non-fatal warnings (surfaced, never silent — FR-027)

- Unused block (FR-005), unused import (FR-012), orphan skill reached by no entry/import
  (FR-014b), empty entry, shared-reference vs target-name collision.
