# Contract: Action API (the only disk writers)

Exposed by `serve` on loopback. Every endpoint mutates **only** in response to an explicit request and
**only** under `action/`. `scan` has no equivalent. All actions are reversible or backed-up.

## `POST /api/disable`  → reversible (FR-011), folder-scoped where possible (FR-023/FR-024)

Request: `{ "skill_key": "claude-code/claude:user/code-review", "scope": "folder", "folder": "/Users/me/proj" }`

- `scope` is `"folder"` (default, preferred) or `"global"`. `folder` is required when `scope="folder"`.
- **Tier-1 (`scope="folder"`, source has a native per-project toggle):** write
  `skillOverrides["<name>"] = "off"` into `<folder>/.claude/settings.local.json` (create the file/key if
  absent, preserving other keys); append a `FolderOverrideRecord`. **No skill files are moved or
  deleted.** Response: `{ "ok": true, "state": "disabled-in-folder", "folder": "/Users/me/proj" }`.
- **Tier-2 (`scope="global"`, or the source has no scoped toggle — e.g. plugin skills):** move the skill
  dir from its active root into the tool quarantine; append a `QuarantineRecord`. The server MUST set
  `"effective_scope": "global"` in the response so the UI can warn the user (FR-024). Response:
  `{ "ok": true, "state": "disabled-global", "effective_scope": "global" }`.
- If `scope="folder"` is requested but the source has no per-folder mechanism, the server does **not**
  silently apply a global change: it returns `{ "ok": false, "error": "folder_scope_unsupported",
  "would_be_scope": "global" }` so the UI can confirm a global disable explicitly (FR-024).

## `POST /api/enable`  → reverses the matching disable (FR-011)

Request: `{ "skill_key": "…", "folder": "/Users/me/proj" }` (`folder` required to undo a Tier-1 disable)

Effect: **Tier-1** → remove the `skillOverrides` key (or restore `previous_value`) in that folder's
`settings.local.json`; clear the `FolderOverrideRecord`. **Tier-2** → move the quarantined dir back to
`original_path`; remove its `QuarantineRecord`. Lossless either way.
Response: `{ "ok": true, "state": "active" }`.

## `POST /api/remove`  → destructive, requires confirmation (FR-012)

Request: `{ "skill_key": "…", "confirm": true }`

Effect: if `confirm != true` → **no change**, `{ "ok": false, "error": "confirmation_required" }`. If
confirmed → copy the dir to tool trash (backup), then delete from the active root. Response:
`{ "ok": true, "state": "removed", "backup": "/…/trash/…" }`.

## `POST /api/label`  → organize (FR-015)

Request: `{ "skill_key": "…", "labels": ["keep", "review"] }`. Effect: set labels in `InspectorState`.

## Failure & atomicity (FR-014)

- Any action that cannot complete (e.g. target not writable) returns `{ "ok": false, "error": "…" }` and
  leaves the setup in its **prior, consistent state** — no half-moves. A move that fails mid-way is
  rolled back.
- After any successful action the server returns enough for the UI to update without a full rescan, and
  a subsequent `scan` reflects the same on-disk reality (FR-005).
- Endpoints bind to loopback only; nothing leaves the machine.
