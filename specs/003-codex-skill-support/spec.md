# Feature Specification: Codex Agent Support (Per-Agent Tabs)

**Feature Branch**: `003-codex-skill-support`

**Created**: 2026-06-16

**Status**: Draft

**Input**: User description: "我要支援 codex，所以我們應該要有個 tab 去切換控制 codex 的 skill，但是 codex 沒有 plugin的概念？所以可能可以支援相對應的，主要還是 skill 跟全局 skill"

---

## Overview *(context — not a spec section)*

The Skill & Context Inspector (`002-skill-inspector`) already treats agents other than the primary
one as a generic `other-agent` source — a single undifferentiated bucket with "no per-repo toggle".
This feature promotes **Codex** from that generic bucket into a **first-class agent** with its own
**tab** in the interface, so the user can switch the whole view to Codex and inventory / control only
Codex's skills.

**Premise correction (evidence-driven).** The triggering request assumed "Codex has no plugin concept,
so mainly skill + global skill". Inspecting the installed Codex CLI (0.137.0) showed this is **not
true** for current Codex: it has a full **plugin + marketplace** system — a `codex plugin`
subcommand, `[marketplaces.*]` and `[plugins."<name>@<marketplace>"]` blocks (with an `enabled` flag)
in `~/.codex/config.toml`, and plugin skills shipped under `<plugin>/skills/<id>/SKILL.md`. Codex's
real skill model is therefore **almost identical to Claude Code's**: a user/global set, plugin sets,
and a project set. This spec is written to that verified reality, not the original assumption — so the
Codex tab carries the **same three source kinds** the Claude tab does, rather than hiding plugins.

This is an **additive** feature on top of `002-skill-inspector`: it reuses the same inventory, overlap,
and control model, the same source-provider trait/registry, and the same safe-by-default /
honest-boundary constraints. It adds (a) an agent-switching tab surface and (b) a first-class Codex
source provider, replacing the generic `other-agent` treatment for Codex.

---

## Clarifications

### Session 2026-06-16

- Q: When Codex is detected on the machine but the tool finds zero Codex skills, what happens to the Codex tab? → A: Hide the Codex tab entirely; tab presence is keyed on at least one Codex skill being found, not on Codex merely being installed.
- Q: When both Claude and Codex tabs are present, which agent's view is active by default on open? → A: The primary (Claude) tab is active by default; Codex is one explicit click away.
- Q: On a tab switch, whose search/filter state applies — shared across tabs or independent per agent? → A: Per-agent independent state; each tab searches/filters only its own agent's skills and restores its own last query on return (a query is never carried onto another agent).

---

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Switch the view to Codex via a tab (Priority: P1)

As someone who uses both Claude Code and Codex, I want a tab (or equivalent agent switcher) at the top
of the inspector so I can switch the entire view to **Codex** and see only Codex's skills, instead of
Codex skills being lumped into a generic "other agents" bucket.

**Why this priority**: This is the smallest change that delivers the user's explicit ask ("一個 tab 去切換
控制 codex 的 skill"). Without an agent switcher, Codex skills have nowhere to live as a first-class set.
Everything else (Codex source kinds, controls) hangs off having this view.

**Independent Test**: On a machine with both Claude and Codex skills, open the inspector, click the
Codex tab, and confirm the view now shows Codex's skills (and only Codex's), clearly labeled as the
Codex agent, with the Claude view one click away.

**Acceptance Scenarios**:

1. **Given** an inspector populated with skills from more than one agent, **When** I open it, **Then** I
   see an agent switcher (tabs) listing each detected agent, including Codex, with one agent's view shown
   at a time.
2. **Given** the agent switcher, **When** I select the Codex tab, **Then** the inventory updates to show
   only Codex's skills, and the active tab clearly indicates I am viewing Codex.
3. **Given** I have a search/filter active on one agent's tab and switch to Codex, **When** I switch back
   to the other agent's tab, **Then** that tab's own prior search/filter is restored unchanged — and the
   Codex tab meanwhile only ever searched/filtered Codex's own skills, never inheriting the other tab's query.

---

### User Story 2 - See Codex's skills organized by its real source kinds (Priority: P1)

As a Codex user, when I am on the Codex tab I want to see Codex's skills organized the way Codex
actually works — a **global / user-level** set, **plugin** sets (from installed marketplace plugins),
and a **current-project** set — each clearly labeled, so the view is an honest mirror of what Codex
will actually load.

**Why this priority**: Getting the source kinds right is what makes the Codex view trustworthy. The
original "global + skill only, no plugins" framing was wrong (see Overview); showing Codex without its
plugin skills would hide a large part of what Codex loads (e.g. a single installed plugin can ship 15+
skills). P1 because it is inseparable from the Codex view being correct at all.

**Independent Test**: On a machine with Codex global skills and at least one installed Codex plugin,
open the Codex tab and confirm a global group and a per-plugin group each appear with the right skills;
if the current project has Codex project skills, confirm a project group appears too.

**Acceptance Scenarios**:

1. **Given** Codex skills installed at the global/user level, in installed plugins, and (if present) in
   the current project, **When** I view the Codex tab, **Then** the skills are grouped by source kind
   (global, plugin, project), each entry showing name, description, source kind, on-disk location, and
   enabled/disabled state.
2. **Given** an installed Codex plugin that ships several skills, **When** I view the Codex tab, **Then**
   each of that plugin's skills appears, attributed to that plugin.
3. **Given** a Codex plugin that is present but switched off (its `enabled` flag is false), **When** I
   view the Codex tab, **Then** its skills are shown as disabled-because-plugin-off rather than active or
   hidden.
4. **Given** a Codex skill with missing or malformed metadata, **When** the Codex inventory is built,
   **Then** it still appears with a "metadata incomplete" indicator rather than being dropped.
5. **Given** Codex skills also serve a purpose covered by a Claude skill, **When** overlap analysis runs
   across the unified set, **Then** those can be grouped into the same cross-agent overlap cluster (the
   existing cross-agent overlap behavior continues to apply).

---

### User Story 3 - Disable / re-enable Codex skills, scoped correctly (Priority: P2)

As a Codex user, from the Codex tab I want to act on Codex skills — disable a redundant skill, or
switch a whole plugin off — scoped correctly, and told plainly when a disable can only take effect
globally, and reversible so I never lose the skill's files.

**Why this priority**: Visibility (US1+US2) is the larger unmet need and is independently valuable;
in-tool control is the natural next step and fulfills the "控制" half of the ask. P2 because the user
can still benefit from a correct Codex inventory before any Codex write actions exist, and because the
exact per-skill Codex scoping mechanism needs provider research that should not block the Codex view
shipping.

**Independent Test**: From the Codex tab, switch an installed Codex plugin off via its `enabled` flag
and confirm its skills show as disabled; switch it back on and confirm they return. Disable an
individual Codex skill and confirm it is deactivated reversibly, with the tool stating the scope
(folder vs global) it actually applied.

**Acceptance Scenarios**:

1. **Given** an installed Codex plugin, **When** I switch it off from the Codex tab, **Then** the tool
   sets that plugin's `enabled` flag off, the plugin's skills show as disabled-because-plugin-off in all
   projects, the change is reversible, and the plugin's files are not deleted.
2. **Given** a Codex skill and a project context where Codex provides per-project scoping, **When** I
   disable it scoped to this folder, **Then** it is deactivated for this project only and remains active
   in other folders.
3. **Given** a Codex skill whose only available disable affects Codex globally, **When** I disable it,
   **Then** the tool tells me the disable is global (affects all folders) before applying it, and does
   not imply a folder-scoped change it cannot make.
4. **Given** a disabled Codex skill or plugin, **When** I re-enable it, **Then** it returns to active
   state with no data loss.
5. **Given** any Codex action that writes to disk, **When** it executes, **Then** it happens only in
   response to my explicit action, never as a side effect of viewing the Codex tab or analyzing overlap.
6. **Given** a Codex action fails (e.g. config not writable), **When** it errors, **Then** the tool
   reports the failure clearly and leaves the Codex setup in its prior consistent state.

---

### Edge Cases

- **Codex not installed / no Codex skills found**: The Codex tab is not shown at all when zero Codex
  skills are found, even if Codex itself is installed — tab presence is keyed on at least one Codex
  skill being discovered, never on Codex merely being present. (No empty-state Codex panel is rendered.)
- **Only one agent present**: If only Codex (or only Claude) is detected, the agent switcher still makes
  the active agent obvious rather than disappearing into an unlabeled single view.
- **Built-in `.system` Codex skills**: Codex ships built-in skills under a `.system` location. The tool
  surfaces them honestly (or marks them as built-in) rather than mixing them indistinguishably with the
  user's own global skills, and never offers a destructive remove on a built-in it cannot restore.
- **Same skill id appears in two Codex sources** (e.g. global and a plugin, or global and project): each
  instance is shown and flagged as a duplicate-by-identity within Codex, distinct from similarity overlap.
- **A Codex source location is missing or unreadable**: The tool reports which Codex source it could not
  read and still shows the Codex skills it could read.
- **Codex provides no per-project per-skill disable mechanism**: Any such disable on the Codex tab MUST
  be surfaced as global; the tool MUST NOT silently apply a global change while implying it was
  folder-scoped.
- **A Codex skill or plugin is toggled outside the tool** (user hand-edits `config.toml`): on next scan
  the Codex inventory reflects the real on-disk state, not a stale cached state.
- **Switching tabs mid-action**: An in-flight action started on one agent's tab completes against that
  agent regardless of a tab switch; the result is reflected when that tab is next viewed.

## Requirements *(mandatory)*

### Functional Requirements

**Agent switching (US1)**

- **FR-001**: System MUST provide an agent switcher (tabbed surface) that lists each detected agent —
  including Codex — and shows exactly one agent's skill view at a time. An agent tab appears only when at
  least one skill is found for that agent; an installed-but-skill-less agent (e.g. Codex present with zero
  skills) MUST NOT contribute a tab.
- **FR-002**: System MUST, when an agent tab is selected, scope the inventory shown to that agent's
  skills only, and MUST make the currently active agent unambiguous in the interface. On open, the
  primary (Claude) tab MUST be the active default when present; Codex is reached by an explicit switch.
- **FR-003**: System MUST detect Codex as a distinct agent and label its skills as originating from
  Codex, rather than placing them in an undifferentiated "other agents" bucket.
- **FR-004**: System MUST keep view state (active search / filter) independent per agent tab: each tab's
  search and filters apply only to that agent's skills, are retained when switching away, and are restored
  when returning — a query entered on one agent's tab MUST NOT be carried onto another agent's tab.

**Codex source model (US2)**

- **FR-005**: System MUST represent Codex's skills using the source kinds Codex actually has — a
  **global / user-level** kind, a **plugin** kind (one group per installed marketplace plugin), and a
  **project / current-folder** kind — discovering each without the user pointing at directories.
- **FR-006**: System MUST group the Codex view by these source kinds, each entry showing at minimum:
  name, one-line description, source kind (and plugin name where applicable), on-disk location, and
  enabled/disabled state.
- **FR-007**: System MUST reflect a Codex plugin's enabled/disabled state: skills belonging to a plugin
  that is switched off MUST be shown as disabled-because-plugin-off, not active and not hidden.
- **FR-008**: System MUST surface Codex's built-in (`.system`) skills honestly — marked as built-in /
  not user-authored — rather than presenting them as the user's own removable global skills.
- **FR-009**: System MUST flag — never silently drop — Codex skills with missing or malformed metadata.
- **FR-010**: System MUST reflect the true current on-disk/config state of Codex skills and plugins on
  each scan, including changes made outside the tool.
- **FR-011**: System MUST allow Codex skills to participate in the existing cross-agent overlap clusters,
  surfacing duplicates that span Codex and other agents, and MUST distinguish duplicate-by-identity (the
  same skill id present in two Codex sources) from similarity-based overlap.

**Codex control (US3)**

- **FR-012**: Users MUST be able to switch an installed Codex plugin on/off from the Codex tab by setting
  its `enabled` flag, reversibly and without deleting the plugin's files; the inventory MUST reflect the
  new plugin state on the next scan.
- **FR-013**: Users MUST be able to disable and re-enable an individual Codex skill from the Codex tab,
  with disable being reversible and lossless — disabling MUST NOT delete the skill's own files.
- **FR-014**: Where Codex provides a per-folder/per-project scoping mechanism, the system MUST be able to
  disable a Codex skill scoped to a single folder so that a globally-installed Codex skill turned off in
  one folder remains active in others.
- **FR-015**: Where Codex offers no per-folder scoping (so a disable can only take effect globally), the
  system MUST clearly communicate that the disable is global before applying it, and MUST NOT present it
  as folder-scoped.
- **FR-016**: System MUST modify on-disk Codex skill/plugin state ONLY in response to an explicit user
  action, never as a side effect of scanning, viewing the Codex tab, or analyzing overlap.
- **FR-017**: System MUST, on any Codex action failure, report the failure clearly and leave the Codex
  setup in its prior consistent state.

**Cross-cutting**

- **FR-018**: System MUST be safe-by-default for Codex as it is for other agents: reading and analyzing
  Codex skills never changes the user's setup.
- **FR-019**: System MUST clearly communicate the boundary of what it knows about Codex — distinguishing
  verified on-disk/config facts from suggestions (overlap) and from capabilities Codex does not provide
  (e.g. any per-project scoping it lacks).

### Key Entities *(include if feature involves data)*

- **Agent (tab)**: A coding agent the inspector can scope the view to (Claude, Codex, …). Key
  attributes: identity/name, detected/installed state, the set of source kinds it supports.
- **Codex Skill Source**: An origin the tool scans for Codex skills. Key attributes: kind (global/user-
  level, plugin, or project/current-folder), the plugin name + enabled state where kind is plugin, the
  built-in (`.system`) flag where applicable, location, readability/availability.
- **Skill** (existing): Carries an originating-agent attribute so a skill can be attributed to Codex and
  shown under the Codex tab, plus its enabled/disabled state including disabled-because-plugin-off.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user with both agents installed can switch from the Claude view to a Codex-only view in
  a single action (one tab click) and see Codex's skills, with 100% of shown skills attributed to Codex.
- **SC-002**: On the Codex tab, the tool discovers at least 95% of the Codex skills actually installed
  across Codex's global, plugin, and project locations on a representative machine (measured against a
  hand-built ground-truth list).
- **SC-003**: For every Codex skill shown, the user can correctly tell its source kind (global / which
  plugin / project) and whether it is enabled — 100% of Codex entries carry source kind + state.
- **SC-004**: For every installed Codex plugin, the tool's shown enabled/disabled state matches the
  plugin's actual `enabled` flag in config for 100% of plugins.
- **SC-005**: Zero unintended modifications: across normal use of the Codex tab, no Codex skill or plugin
  state changes except as the direct result of an explicit user action.
- **SC-006**: Whenever a Codex disable can only take effect globally, the tool says so before applying it
  in 100% of such cases — it never implies a folder-scoped change it cannot make.

## Assumptions

These are informed defaults chosen where the description left a detail open, plus the evidence-based
correction of the original premise. They can be revised via `/speckit-clarify`.

- **Premise corrected to match installed Codex**: The original "Codex has no plugin concept" assumption
  was checked against the installed Codex CLI (0.137.0) and found false — Codex has plugins +
  marketplaces with an `enabled` toggle, and plugin-shipped skills. The spec follows that verified
  reality, giving the Codex tab the same three source kinds (global, plugin, project) as Claude.
- **Builds on `002-skill-inspector`**: This feature extends the existing inspector's inventory / overlap
  / control / safety model and its source-provider trait/registry rather than introducing a separate
  tool. Codex becomes a first-class provider alongside Claude, replacing the generic `other-agent`
  treatment for Codex.
- **Codex source locations** (to be confirmed in planning research): global/user skills under the Codex
  home (`$CODEX_HOME/skills/`, default `~/.codex/skills/`, with `.system` for built-ins); plugin skills
  under each installed plugin's `skills/` directory as declared by the marketplace; the project-level
  Codex skill convention is the one detail still to be pinned by provider research (no project instance
  was found on-disk to confirm the exact path), and is not an MVP blocker.
- **Codex disable mechanisms**: plugin on/off is the `enabled` flag in `~/.codex/config.toml`'s
  `[plugins."<name>@<marketplace>"]` block (verified present). Whether Codex has a native per-skill or
  per-project skill toggle equivalent to Claude's `skillOverrides` is deferred to provider research;
  where none exists, the reversible global-disable fallback already defined in `002` for sources lacking
  a scoped toggle applies, surfaced honestly as global (FR-015).
- **Local, on-machine, safe-by-default**: All existing inspector constraints carry over unchanged —
  offline, scanning never mutates, only explicit actions write, destructive removal needs confirmation,
  and built-in `.system` skills are not offered for destructive removal.
