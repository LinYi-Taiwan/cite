# Contract: Codex Source Provider

Implements the existing `SourceProvider` trait (`crates/skill-inspector/src/scan/source.rs:20`).
Read-only. Registered in `Registry::with_defaults` (source.rs:52) as the first-class Codex provider,
replacing the generic `other-agent` treatment for Codex.

## Identity

- `agent()` → `"codex"` (stable label; used as the `agent` field on every emitted skill/source and as
  the agent-tab id).

## `sources(ctx) -> Vec<SkillSource>`

`ctx.home` = user home (Codex home = `$CODEX_HOME` if set, else `<home>/.codex`).
`ctx.project_root` = current project.

Emits, each via `classify_availability` (never hard-errors):

1. **global/user** — exactly one source:
   - `id = "codex:user"`, `kind = User`, `root = <codex_home>/skills`.
   - Skills directly under `skills/` are user-authored; skills under `skills/.system/` are **built-in**
     and MUST be emitted with `built_in = true`.

1b. **personal/user** — exactly one source:
   - `id = "codex:user-agents"`, `kind = User`, `root = <home>/.agents/skills`.
   - The documented personal skill location (<https://developers.openai.com/codex/skills>), bound to the
     real `$HOME` (NOT `$CODEX_HOME`). No `.system/` subtree here.

2. **plugin** — one source **per installed plugin** declared in `<codex_home>/config.toml`:
   - Parse `[plugins."<name>@<marketplace>"]` tables. For each, `id = "codex:plugin:<name>"`
     (bare name, pre-`@`), `kind = Plugin`.
   - `root` = the plugin's skills dir under the plugin cache, i.e.
     `<codex_home>/plugins/cache/<marketplace>/<name>/<version>/skills` (resolve `<version>` = the
     installed version dir present in cache).
   - A plugin with `enabled = false` is still emitted (its skills are shown), but every skill under it
     MUST resolve to `SkillState::DisabledPlugin` (see action contract / model).
   - Robust to a missing/garbled `config.toml` → emit zero plugin sources (mirror
     `claude.rs::discover_plugins` resilience, claude.rs:30).

3. **project** — exactly one source (research §2: confirmed `.agents/skills`):
   - `id = "codex:project"`, `kind = Project`, `root = <project_root>/.agents/skills`.
   - Codex scans `.agents/skills` from the cwd up to the repo root; cite emits the one project scope it
     carries (`ctx.project_root`), which equals the repo root in the common case. No `.system/` here.
   - cite does NOT scan `<project_root>/.codex/skills` (an undocumented legacy root the binary still
     loads) — it tracks the documented `.agents/skills` contract only (research §2 note).

## Plugin-enabled lookup (for state mapping)

A helper (analogue of `claude.rs::resolve_plugin_full_key`, claude.rs:121) MUST, given a bare
`codex:plugin:<name>` source id, recover the full `"<name>@<marketplace>"` config key and read its
`enabled` flag, so the scanner can map the plugin's skills to `Active` vs `DisabledPlugin`. Deterministic
on a name collision (sorted-key first-match, same rule as Claude).

## Guarantees

- **Read-only**: no method writes to disk (safe-by-default, FR-016/FR-018).
- **Deterministic**: identical Codex home + project → identical sources/skills ordering (enables `insta`).
- **Graceful**: every root classified `Readable | Missing | Unreadable`; a missing Codex home yields an
  empty-but-valid result, not an error (edge case: Codex not installed).
