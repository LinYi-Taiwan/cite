# Contract: Framework config + lockfile

The single authoritative config (Vite-config analog). It is the **only** place targets are
registered; the registry is not inferred (FR-019/020). Entry mounts and per-target reference
filenames validate against it.

## `skillc.config.yaml`

```yaml
# Target registry — each target has an entry that mounts its top-level skills.
targets:
  admin:
    entry:
      mounts: [frontend-coding, git]      # top-level skills; each MUST exist (FR-014a)
    defaultAgent: claude                  # per-target default (FR-020), optional
  shop:
    entry:
      mounts: [frontend-coding, glossary]
  scm:
    entry:
      mounts: [git, frontend-coding]
  sl-feature:
    entry:
      mounts: [feature-key]

# Supported agent output formats (FR-024). Orthogonal to target.
agents: [claude, codex, cursor, gemini, copilot]

# Reference stems explicitly declared shared (always retained, never pruned).
# A stem that is ALSO a registered target name is a collision → reported (Edge Cases).
sharedReferences: [glossary, conventions]
```

**Validation rules**:
- Every `targets.<t>.entry.mounts[]` MUST resolve to a catalog skill (FR-014a) → else
  `error[entry/missing]`.
- A reference filename `references/<stem>.md` is **per-target** iff `<stem>` ∈ `targets` keys,
  **shared** iff `<stem>` ∈ `sharedReferences`, else **stray** → reported (FR-018).
- `defaultAgent` (if set) MUST ∈ `agents`.
- A `sharedReferences` stem equal to a `targets` key → collision warning (Edge Cases): it stops
  being shared once the target is registered; the explicit registry makes this detectable.

## `skills-lock.json`

Pins every cross-repo source to an exact commit for reproducible, deterministic builds
(FR-008/011/022). Generated/updated by `build`; treated as read-only under `--frozen`/`--locked`.

```json
{
  "version": 1,
  "sources": {
    "shared-skills": {
      "url": "https://github.com/org/shared-skills.git",
      "commit": "a1b2c3d4e5f6...",
      "contentHash": "sha256:..."
    }
  }
}
```

| Field | Type | Rule |
|---|---|---|
| `sources.<id>.url` | string | Repo URL for an import's `Repo(<id>)` source. |
| `sources.<id>.commit` | sha | Exact pinned commit → reproducibility (FR-022). |
| `sources.<id>.contentHash` | sha256 | Dedup key (FR-010) + drift detector (FR-022). |

**Rules**:
- `--frozen`: never hit the network; every `Repo` import MUST already be pinned else hard fail.
- `--locked`: fail (`exit 1`) if a build would change the lockfile (CI integrity).
- Same dependency required at two `commit`s/versions via different paths → `error[dep/version-conflict]`,
  never silently resolved (FR-011).
