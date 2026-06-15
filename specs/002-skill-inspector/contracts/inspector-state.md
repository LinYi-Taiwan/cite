# Contract: inspector-state.json (tool-owned persistence)

The single tool-owned file recording reversible mutations + user organization. Written **only** by
`action/`; read by `scan`/`serve` to reflect `Skill.state` and `labels`. Lives in a tool config dir
(not inside any skill root, so the user's roots stay clean).

```jsonc
{
  "version": 1,
  "quarantine": [
    {
      "skill_key": "claude-code/claude:plugin:foo/code-review",
      "id": "code-review",
      "agent": "claude-code",
      "original_root": "/Users/me/.claude/plugins/foo/skills",
      "original_path": "/Users/me/.claude/plugins/foo/skills/code-review",
      "quarantine_path": "/Users/me/.config/skill-inspector/quarantine/claude-code/code-review",
      "content_hash": "sha256:…",
      "disabled_at": "2026-06-15T10:00:00Z"
    }
  ],
  "folder_overrides": [
    {
      "skill_key": "claude-code/claude:user/code-review",
      "agent": "claude-code",
      "folder": "/Users/me/proj",
      "settings_path": "/Users/me/proj/.claude/settings.local.json",
      "previous_value": null,
      "new_value": "off",
      "disabled_at": "2026-06-15T10:00:00Z"
    }
  ],
  "labels": {
    "claude-code/claude:user/qa-playwright": ["keep"]
  }
}
```

## Invariants

- A `QuarantineRecord` holds everything needed to restore the (Tier-2 / global) skill to its **exact**
  original location (FR-011) and to detect drift (hash mismatch on restore ⇒ warn, don't clobber). Used
  only for sources with no native per-folder toggle (e.g. plugin skills).
- A `FolderOverrideRecord` records a Tier-1 per-folder disable: the tool set `skillOverrides["<name>"]`
  in `<folder>/.claude/settings.local.json`. The **settings file is the source of truth**; this record
  exists for undo (`previous_value`) and drift detection. The tool MUST preserve any other keys in that
  settings file and never move/delete the skill's own files for a Tier-1 disable.
- A skill is `disabled-global` **iff** a matching `quarantine` record exists; it is `disabled-in-folder`
  for a given context **iff** that folder's `settings.local.json` has `skillOverrides["<name>"] = "off"`
  (or another hidden state). `scan` derives `Skill.state` from this file ∩ on-disk reality ∩ the target
  folder's settings. If the user (or the `/skills` TUI) manually changed the override or restored/deleted
  a quarantined dir, the scan reconciles to the real state (FR-005) and flags the stale record.
- This file is the only persisted mutation record; removing it loses bookkeeping but never corrupts a
  skill (quarantined dirs remain on disk, restorable manually).
- Timestamps are the only nondeterministic field; they are excluded from inventory-export snapshot tests.
