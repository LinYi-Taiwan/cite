# Phase 0 Research: Codex Agent Support

All findings below were verified against the **installed Codex CLI 0.137.0** on this machine
(`~/.local/bin/codex`, `~/.codex/`). Where a fact could not be confirmed on-disk it is marked
**UNCONFIRMED** and carried as a planning risk rather than assumed.

## §1 — Premise correction: Codex DOES have plugins

**Decision**: Model Codex with the same three source kinds as Claude (global/user, plugin, project),
not the "global + skill only" model the original request assumed.

**Rationale**: Direct inspection contradicts the assumption:
- `codex plugin` subcommand exists: `add | list | remove | marketplace`
  [ref: tool:Bash `codex plugin --help`].
- `~/.codex/config.toml` carries `[marketplaces."<name>"]` and `[plugins."<name>@<marketplace>"]` with
  an `enabled = true/false` flag [ref: ~/.codex/config.toml — `[plugins."self@claude-dotfiles-codex"]` /
  `enabled = true`].
- `codex plugin list` enumerates a marketplace (`openai-curated`) of installable plugins
  [ref: tool:Bash `codex plugin list`].

**Alternatives considered**: Ship the reduced model and treat plugins as out-of-scope. Rejected: a single
installed plugin ships 15+ skills (verified below), so hiding plugins would make the Codex inventory
materially incomplete and dishonest — the exact failure `002` is built to avoid.

## §2 — Codex skill roots (where skills live)

**Decision**: `CodexProvider` discovers four roots, reusing the shared `skills/<id>/SKILL.md` walker.

| Kind | Root | Verified |
|---|---|---|
| global/user | `$CODEX_HOME/skills/` (default `~/.codex/skills/`), holds Codex's bundled built-ins under `.system/` | ✅ [ref: tool:Bash `ls ~/.codex/skills` → `.system/`; binary strings `Installs into $CODEX_HOME/skills/<skill-name>`] |
| personal/user | `$HOME/.agents/skills/` — the documented personal skill location | ✅ [ref: <https://developers.openai.com/codex/skills> "User: `$HOME/.agents/skills`"; tool:Bash `ls ~/.agents/skills` → `find-skills/`] |
| plugin | `~/.codex/plugins/cache/<marketplace>/<plugin>/<version>/skills/<id>/SKILL.md`, one group per **installed** plugin | ✅ [ref: tool:Bash `find ~/.codex/plugins` → `…/claude-dotfiles-codex/self/0.1.1/skills/git/SKILL.md` (15+ skills)] |
| project | `<repo>/.agents/skills/` — Codex scans `.agents/skills` from the cwd up to the repo root | ✅ [ref: <https://developers.openai.com/codex/skills> "Repository (CWD/Parent/Root): `.agents/skills`"; tool:Bash `ls ~/Desktop/repos/awesome-frontend-skills-main/.agents/skills` → `agent-browser/`] |

> Note: the live 0.137.0 binary ALSO loads `<repo>/.codex/skills` (an undocumented legacy root —
> `codex debug prompt-input` lists it as `r0` even in a clean repo). cite deliberately does **not** scan
> it: the official docs list only `.agents/skills` for repo skills, so cite tracks the documented
> contract rather than an undocumented back-compat path that may be removed. Skills placed in
> `.codex/skills` will not appear in the inspector (move them to `.agents/skills`).

**Rationale**: All four are concretely confirmed (official docs + live on-disk data). The
`skills/<id>/SKILL.md` layout is identical to Claude's, so the existing `skill_md.rs` parser is reused
unchanged (matches the convergence note in `002` research.md §2 and the auto-memory "all agents use
`skills/<id>/SKILL.md`").

**Correction (project skills were real all along)**: An earlier draft of this section marked the project
kind **UNCONFIRMED** because it looked for `.codex/skills/` in local repos and found none. That was the
wrong path — Codex's project (and personal) skills live under **`.agents/skills`**, not `.codex/skills`.
Both `~/.agents/skills` and `<repo>/.agents/skills` exist on this machine and are documented, so the
"Codex has no project skills" conclusion was a tool blind spot, not a Codex fact. The provider now emits:
a `codex:user` source for `<codex_home>/skills` (built-ins), a `codex:user-agents` source for
`<home>/.agents/skills`, and a `codex:project` source for `<project_root>/.agents/skills`. cite carries a
single project scope (`ctx.project_root` = the cwd / `--project`), which equals the repo root in the
common case and so covers Codex's cwd→repo-root walk; faithfully walking every intermediate ancestor is a
later refinement, not an MVP blocker.

The provider emits, per scan: `codex:user` (`<codex_home>/skills`, built-ins), `codex:user-agents`
(`<home>/.agents/skills`), `codex:project` (`<project_root>/.agents/skills`), and one
`codex:plugin:<name>` per installed plugin. cite carries a single project scope
(`ctx.project_root` = the cwd / `--project`), equal to the repo root in the common case; faithfully
walking every intermediate ancestor is a later refinement, not an MVP blocker.

**Built-ins (`.system`)**: The `.system/` subtree holds Codex's own built-in skills (skill-creator,
plugin-creator, skill-installer, openai-docs, imagegen) [ref: tool:Bash `find ~/.codex/skills/.system`].
Decision: surface them with a **built-in flag** so the UI can mark them and the action layer can refuse a
destructive remove on them (FR-008), rather than dropping them or treating them as user-authored.

## §3 — Disable / enable mechanisms

**Decision**: Two strategies, dispatched by source kind, mirroring `002`'s tiered model.

1. **Plugin on/off (primary, well-supported)** → write the `enabled` flag in
   `[plugins."<name>@<marketplace>"]` of `~/.codex/config.toml`. This is the direct analogue of Claude's
   `enabledPlugins` and maps a plugin's skills to `SkillState::DisabledPlugin` (global; affects all
   projects) [ref: ~/.codex/config.toml]. The config key is `"<plugin>@<marketplace>"`
   (e.g. `"self@claude-dotfiles-codex"`) — the provider must preserve that exact key when writing.

2. **Individual skill disable** → **Codex's native `[[skills.config]]` switch** (CORRECTED — see below).
   Write a `[[skills.config]]` entry to `~/.codex/config.toml` selecting the skill by EITHER its
   `SKILL.md` `path` or its `name`, with `enabled = false`. Codex drops that skill from its loaded set,
   leaving the skill's files untouched — reversible by removing the entry. This is the direct Codex
   analogue of Claude's per-skill toggle, and it supersedes the quarantine-move fallback for Codex
   skills. The provider maps a `[[skills.config]] enabled=false` skill to `SkillState::DisabledGlobal`.

**Correction (Codex DOES have a per-skill toggle)**: An earlier draft of this section concluded "no
native per-folder/per-skill toggle found" and fell back to a Tier-2 quarantine *move*. That was wrong —
it missed `[[skills.config]]`. Verified empirically:
- Official docs document the syntax: `[[skills.config]]` / `path = ".../SKILL.md"` / `enabled = false`,
  "disable a skill without deleting it" [ref: <https://developers.openai.com/codex/skills>].
- The binary carries `core-skills/src/config_rules.rs`, `skills.enabled`, a **path selector / name
  selector**, and an app-server `SkillsConfigWriteParams/Response` API [ref: tool:Bash strings codex →
  "ignoring skills.config entry without a path or name selector", "Path-based selector. Name-based selector"].
- Live proof it takes effect: `cd <repo> && codex debug prompt-input -c 'skills.config=[{path="…/project-demo/SKILL.md",enabled=false}]'`
  drops `project-demo` from the rendered skill list (present→absent); the same with `name="project-demo"`
  also drops it [ref: tool:Bash `codex debug prompt-input` baseline=1 → with-disable=0]. A hand-written
  `[[skills.config]]` block in a copied `config.toml` reproduces the drop, confirming the block shape cite
  writes is honored.

**Scope caveat**: `[[skills.config]]` lives in the user-level `~/.codex/config.toml`, so the disable is
**global** (all projects). Codex's docs state there is no per-repository enable/disable mechanism; a
project-layer `<repo>/.codex/config.toml` exists for sandbox/MCP/hooks/model defaults but `skills.config`
is not documented for it. For a PROJECT skill this is immaterial — the skill exists only in that repo, so
a global disable affects only that repo in practice.

**Rationale**: One format-preserving `toml_edit` write, fully reversible, no file movement — strictly
better than the quarantine move it replaces (which dirtied the user's repo by relocating a project skill's
directory). Plugin on/off (mechanism 1) is unchanged.

**Alternatives considered**: (a) the old Tier-2 quarantine move — rejected, it relocates files and is
needless now that the native switch is confirmed; (b) editing a skill's own `disable-model-invocation`
frontmatter — rejected, it mutates the authored file and conflates "author says don't auto-invoke" with
"user turned this off".

## §4 — Writing `config.toml` safely (format preservation)

**Decision**: Use a **format-preserving** TOML edit (e.g. `toml_edit`) to flip only the `enabled` value,
leaving the user's comments, key ordering, and unrelated tables byte-stable; assert this in
`codex_plugin_toggle.rs`.

**Rationale**: `~/.codex/config.toml` is a hand-edited file with marketplaces, project trust levels, and
TUI settings [ref: ~/.codex/config.toml]. Claude's analogue was JSON written via `serde_json`; Codex is
TOML, and a naive `toml::to_string` round-trip would reorder/strip the user's file. A surgical edit keeps
the write reversible and non-destructive (FR-012, FR-016, FR-017).

**Alternatives considered**: (a) full `serde` TOML round-trip — rejected, drops comments/order; (b)
regex/line patch — rejected as brittle against nested-table quoting (`"name@mkt"` keys contain `@` and
`.`). `toml_edit` is the standard format-preserving choice.

## §5 — Agent tab UI delivery

**Decision**: The agent switcher is a **client-side** tab in `inspector.html` that filters the
already-loaded inventory by the existing `agent` field; no new server endpoint, no export schema change.

**Rationale**: Every `Skill` and `SkillSource` already carries `agent`
[ref: crates/skill-inspector/src/model.rs:33, 83], and the key is `agent/source_id/id`
[ref: crates/skill-inspector/src/model.rs:43]. So scoping the view to Codex is a pure front-end filter
over data the `scan`/export already produces — the tab switch is instant (< 1 s) and the safe-by-default
boundary is untouched (no read path changes). Per-source-kind grouping (the existing left-column layout)
lives **inside** each agent tab; for Codex that means global + plugin (+ project when present), with no
"plugin section hidden" special-case.

**Alternatives considered**: A server-side per-agent endpoint — rejected as needless (data is already
client-side) and it would add a write/read surface for no benefit.

## Open items carried to design / tasks

- **Codex project skill path** (§2): conditional source kind; ships absent until confirmed. Not a blocker.
- **Native per-project skill toggle** (§3): none found; Tier-2 quarantine fallback used. Re-evaluate if a
  Codex config key appears in a future version.
