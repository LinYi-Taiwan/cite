# Contract: Inventory export (JSON consumed by the UI)

Emitted by `scan --format json` and by `serve` on initial load. A **superset** of the existing
`GraphExport { version, nodes, edges }` shape so `graph.html` renders it with minimal change; extra
top-level keys carry inventory/overlap/activation/context.

```jsonc
{
  "version": 1,
  "generated_for": { "agents": ["claude-code"], "project": "/abs/project" },

  "sources": [
    { "id": "claude:user", "agent": "claude-code", "kind": "user",
      "root": "/Users/me/.claude/skills", "availability": "readable" },
    { "id": "claude:project", "agent": "claude-code", "kind": "project",
      "root": "/abs/project/.claude/skills", "availability": "missing" }
  ],

  "skills": [
    { "id": "code-review", "name": "Code Review", "description": "Review a diff…",
      "source_id": "claude:user", "agent": "claude-code", "path": "/Users/me/.claude/skills/code-review",
      "content_hash": "sha256:…", "state": "active", "metadata_complete": true, "labels": [] }
  ],

  "clusters": [
    { "id": "c-abc", "kind": "similarity", "members": ["claude-code/claude:user/code-review",
      "claude-code/claude:plugin:devkit/devkit.typescript.code-review"],
      "reason": "shared terms: code-review, typescript, diff", "score": 0.82 },
    { "id": "c-def", "kind": "duplicate-identity",
      "members": ["claude-code/claude:user/qa-playwright", "claude-code/claude:project/qa-playwright"],
      "reason": "identical content hash" }
  ],
  "clusters_empty_reason": null,            // string when clusters == [] (FR-010), e.g. "no overlaps found"

  "activation": [
    { "skill_key": "claude-code/claude:user/code-review", "context": "/abs/project",
      "eligible": true, "active": true }
  ],

  "context_loads": [
    { "turn_ref": "session-…#3", "availability": "present",
      "loaded_skill_keys": ["claude-code/claude:user/code-review"] },
    { "turn_ref": "cursor-session", "availability": "unavailable", "loaded_skill_keys": [] }
  ],

  // graph.html compatibility — nodes/edges projected from skills + clusters
  "nodes": [ { "id": "claude-code/claude:user/code-review", "kind": "skill", "layer": "claude:user",
               "external": false, "description": "Review a diff…", "source": "claude:user", "flags": [] } ],
  "edges": [ { "from": "…/code-review", "to": "…/devkit.typescript.code-review", "kind": "overlap" } ]
}
```

## Invariants

- Every `skills[]` entry carries `source_id` + `agent` + `state` (SC-006). Incomplete metadata ⇒
  `metadata_complete=false`, still listed (FR-003).
- `clusters[]` has no singletons; when empty, `clusters_empty_reason` is a non-null string (FR-010).
- A `context_loads[]` entry with `availability:"unavailable"` MUST have empty `loaded_skill_keys`
  (FR-018, SC-007) — never fabricated.
- `nodes`/`edges` are a derived projection for the viewer; `skills`/`clusters` are the source of truth.
- The whole document is deterministic for identical roots (stable ordering by key) so it can be
  snapshot-tested.
