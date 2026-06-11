# Contract: Build artifact layout

`build` produces one independent, inspectable artifact directory per `(target, agent)`,
separate from install (FR-021), deterministic (FR-022), self-contained (FR-009), pruned
(FR-015).

## Layout

```
dist/
└── <target>/
    └── <agent>/
        ├── manifest.json          # what's in the bundle and why (FR-021, SC-008)
        ├── skills/
        │   └── <skill-id>/
        │       ├── SKILL.md        # agent frontmatter re-rendered; @include INLINED; {{Alias}} resolved
        │       └── references/
        │           ├── <target>.md # per-target reference for THIS target only (FR-015)
        │           └── <shared>.md # shared references retained
        └── ...                     # one dir per skill in the closure (deduped)
```

## `SKILL.md` frontmatter

Parsing strips the **authoring** frontmatter (`id` / `referenceMode` / `appliesTo` /
`imports`) — those are framework-internal and meaningless to the agent runtime. The emitted
`SKILL.md` therefore needs the **agent's own** frontmatter re-rendered, or it is not a
loadable skill. Each agent formatter decides which fields to render; the cross-agent default
(and Claude's contract) is the [Agent Skills](https://agentskills.io/specification) loader
minimum:

```yaml
---
name: <skill id>          # the slug — lowercase/hyphen, MUST == the skill directory name
description: <skill description>
---
```

Per the Agent Skills spec, `name` must be 1–64 chars, lowercase alphanumeric + hyphens, and
match the parent directory — so it is the skill **`id`** (which is the directory name), NOT
the human-readable authoring `name` (a CJK/Titlecase title would fail validation). The human
title survives as the body's first heading. `description` is non-empty, ≤1024 chars.

Rendered via the YAML serializer (correct quoting/escaping) for deterministic bytes (FR-022).

## `manifest.json`

Lets a maintainer answer "what is in this bundle and why" from declarations + config alone
(SC-008), and is the determinism anchor.

```json
{
  "target": "admin",
  "agent": "claude",
  "skills": [
    {
      "id": "frontend-coding",
      "reason": "mounted",                 // "mounted" | "pulled-by:<skill-id>"
      "includes": ["pr-rules"],            // blocks inlined into this skill
      "imports": { "CodeReview": "code-review" },
      "references": ["admin.md", "glossary.md"]
    },
    {
      "id": "code-review",
      "reason": "pulled-by:frontend-coding",
      "includes": [],
      "imports": {},
      "references": []
    }
  ],
  "warnings": [
    "block/unused: experimental-block",
    "import/unused: frontend-coding -> LegacyHelper"
  ]
}
```

## Guarantees (assert in tests)

| Guarantee | Rule | Spec |
|---|---|---|
| Self-contained | non-empty bundle has zero unresolved `@include`/`{{}}`/dep | FR-009, SC-002 |
| Deterministic | identical inputs → byte-identical tree (sorted keys, no timestamps) | FR-022, SC-007 |
| Pruned | foreign-target reference files = 0 | FR-015, SC-006 |
| Deduped | each skill dependency appears once under `skills/` | FR-010 |
| Inspectable | readable/diffable without install | FR-021 |
| Empty-valid | a target with an empty entry → empty `skills/`, still a valid artifact | Edge Cases |
| Traceable | every skill has a `reason` (mounted / pulled-by) | SC-008 |

## Install (separate step)

`skillc install --artifact dist/<target>/<agent> --dest <agent-dir>` places the artifact into
the agent's destination in the agent's format; runtime loading unchanged (FR-023/025). The
`<agent>` formatter decides the on-disk shape at the destination (e.g. claude skills dir vs
codex layout) — the *artifact* above is the canonical, agent-neutral-but-agent-tagged
intermediate.
