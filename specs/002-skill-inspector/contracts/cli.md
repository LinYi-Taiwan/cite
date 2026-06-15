# Contract: CLI surface (`skill-inspector`)

Single binary, two subcommands. Read and write are separated at the command level: `scan` never
mutates; mutation happens only via `serve`'s action API (see `action-api.md`).

## `skill-inspector scan`

Pure-read. Discovers sources, builds inventory + overlap clusters, emits an artifact.

| Flag | Default | Meaning |
|---|---|---|
| `--agent <id>` (repeatable) | `claude-code` | Which agent providers to scan (US5; default = primary agent). |
| `--project <path>` | cwd | Project root for project-level + activation context. |
| `--format <json\|html>` | `json` | `json` → inventory export (see `inventory-export.md`); `html` → static self-contained snapshot reusing `graph.html`. |
| `--out <path>` | stdout | Output destination. |

**Behavior**: MUST NOT write to any skill root or `inspector-state.json` (FR-013, FR-021, SC-005).
Unreadable sources are reported in the export's `sources[]` with `availability`, scan still succeeds
(edge). Exit non-zero only on tool failure, never because a skill was malformed (FR-003).

## `skill-inspector serve`

Starts the local web UI + action API.

| Flag | Default | Meaning |
|---|---|---|
| `--agent <id>` (repeatable) | `claude-code` | As above. |
| `--project <path>` | cwd | As above. |
| `--port <n>` | auto | Local port; bind loopback only (offline/local-only). |

**Behavior**: serves `assets/inspector.html` + JSON endpoints. The initial page load returns the same
inventory export as `scan --format json`. Disk mutation happens ONLY through the action endpoints in
`action-api.md`, each requiring an explicit client request. Binds to localhost only — nothing leaves the
machine.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success (including "some sources unreadable" and "skills with incomplete metadata"). |
| non-0 | Tool-level failure (e.g. cannot write `--out`, port unavailable). |
