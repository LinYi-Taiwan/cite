# Feature Specification: Skill & Context Inspector

**Feature Branch**: `002-skill-inspector`

**Created**: 2026-06-15

**Status**: Draft

**Input**: User description: "我依舊在 skill 這邊感受到滿滿的痛點：(1) 我不知道目前我有的 skill 的狀態，因為可能會有 overlap 或是裝了一堆 skill，所以很需要視覺化甚至直接 GUI 操作；(2) 開始執行 agent 後那些 context 的管理對開發者來說是黑盒子，我無法有意識地採用或拋棄，根本不知道 agent 實際上會去讀哪個，有點不可控。" → 這要的不是 compiler，是 inspector：一個面向 agent 使用者的 skill / context 檢視器 + 控制台。

---

## Overview *(context — not a spec section)*

This feature is a **deliberate pivot** away from the sibling spec `001-skill-compiler`
(an authoring / compile-time tool for *writing* skills). This spec describes a **consumption-side
inspector and control panel**: a tool that helps a person who *uses* AI coding agents understand,
de-duplicate, and control the skills already installed on their machine, and gain visibility into
what an agent actually pulls into context.

Two user pains drive the whole feature:

1. **"I don't know what I have."** Skills accumulate from many sources (the agent's own skill
   directories, per-project directories, installed plugins/marketplaces, other agents). Families
   overlap silently — three parallel `speckit` families, ten-plus code-review skills, many
   playwright/e2e skills — and the user cannot tell which overlap, which to keep, or which are dead.
2. **"I can't see or control what the agent reads."** Once an agent starts a turn, which skills it
   considers, which are active, and what it actually loaded into context is a black box. The user
   wants conscious adopt/discard decisions, not guesses.

The differentiator versus what agent vendors ship today: marketplaces only help you **install**;
nobody helps you **inventory, de-duplicate, and prune across agents**. A single cross-agent view is
the moat — a vendor will only ever manage its own skills.

---

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See every installed skill in one unified view (Priority: P1)

As an agent user, I open the inspector and immediately see a single, searchable list of **every**
skill installed on my machine — regardless of which source it came from (the agent's user-level
skill directory, the current project's directory, installed plugins/marketplaces) — each showing its
name, one-line description, source/origin, on-disk location, and enabled/disabled state.

**Why this priority**: This is the foundation and the smallest thing that already relieves pain #1.
Today no single screen exists that answers "what skills do I actually have?" — the information is
scattered across directories and plugin bundles. A unified, honest inventory is independently
valuable even before any overlap analysis or actions.

**Independent Test**: Point the tool at a machine that has skills from at least two sources (e.g.,
user-level + at least one installed plugin). Confirm the tool lists each skill once, with correct
name, description, source label, and enabled state, and that the list is searchable/filterable.
Delivers value on its own: the user can finally see the full set in one place.

**Acceptance Scenarios**:

1. **Given** skills installed from a user-level directory and from one or more plugins, **When** I
   open the inspector, **Then** I see one consolidated list containing every skill with its name,
   description, source, location, and enabled/disabled state.
2. **Given** the consolidated list, **When** I type part of a skill name or keyword from its
   description, **Then** the list filters to matching skills in under one second.
3. **Given** a skill whose metadata is malformed or missing a description, **When** the inventory is
   built, **Then** that skill still appears with a clear "metadata incomplete" indicator rather than
   being silently dropped.

---

### User Story 2 - Surface overlaps, duplicates, and dead skills (Priority: P1)

As an agent user, I want the inspector to group skills that appear to do the same job (e.g., the
three `speckit` families, the cluster of code-review skills, the cluster of playwright/e2e skills) so
I can see at a glance which skills overlap, decide which to keep, and spot ones that look dead or
redundant.

**Why this priority**: This is the "aha" of pain #1. The raw list (US1) tells me *what* I have;
overlap detection tells me *what's redundant* — which is the actual decision I'm stuck on every
session. P1 because the inventory without overlap grouping still leaves the user manually eyeballing
similarity.

**Independent Test**: On a machine with known overlapping skill families, confirm the tool groups
those families together, explains *why* it grouped them (the basis for the similarity), and that a
human reviewer agrees the suggested groups are reasonable. Delivers value: the user can point at a
cluster and decide what to prune.

**Acceptance Scenarios**:

1. **Given** several skills with similar descriptions/purpose, **When** the inspector analyzes the
   inventory, **Then** it presents them grouped into overlap clusters, each cluster showing the
   members and a human-readable reason they were grouped.
2. **Given** an overlap cluster, **When** I inspect it, **Then** I can see distinguishing details of
   each member (source, description, last-related activity if known) sufficient to choose which to
   keep.
3. **Given** a skill that no other skill overlaps with and shows no signal of being redundant, **When**
   I view the inventory, **Then** it is NOT forced into a misleading cluster (no false grouping).
4. **Given** the overlap analysis, **When** it runs, **Then** the user is told this is a *suggestion*
   to review, not an automatic judgment — no skill is altered by analysis alone.

---

### User Story 3 - Act on the findings: disable, remove, or organize skills safely (Priority: P2)

As an agent user, after I've seen the overlaps, I want to act directly from the interface — disable a
redundant skill, remove one I'm sure about, or group/label skills — without hand-editing files in
scattered directories, and without fear of breaking my setup. In particular, when I disable a skill I
want to scope it to **the current folder/project** so that turning off a globally-installed skill in
project A does not silently break it everywhere else — the exact surprise that motivates this story.

**Why this priority**: Visibility (US1+US2) is the bigger unmet need and must come first; acting on it
is the natural next step. P2 because the user can still get value from US1+US2 by manually editing
files, but in-tool action is where the "GUI 操作" ask is fulfilled and the workflow becomes painless.

**Independent Test**: From the interface, disable a skill and confirm the agent no longer offers it;
re-enable it and confirm it returns. Confirm a destructive remove requires explicit confirmation and
that disable is reversible. Delivers value: the user prunes their setup without leaving the tool.

**Acceptance Scenarios**:

1. **Given** a skill in the inventory, **When** I disable it from the interface, **Then** it is
   deactivated for the relevant agent, the change is reversible, and the inventory reflects the new
   state.
2. **Given** a skill that is installed globally (at user level) but that I only want off in my current
   project, **When** I disable it scoped to this folder, **Then** it is deactivated for this
   project/folder only, remains active in other folders, the skill's own files are not deleted, and the
   inventory shows it as disabled-in-this-folder.
3. **Given** a skill whose source has no per-folder scoping mechanism (e.g. a plugin skill), **When** I
   disable it, **Then** the tool tells me this disable is global (affects all folders) before applying
   it, rather than implying a folder-scoped change it cannot make.
4. **Given** a disabled skill, **When** I re-enable it, **Then** it returns to active state with no
   data loss.
5. **Given** I choose to remove a skill, **When** I confirm the destructive action through an explicit
   confirmation step, **Then** the skill is removed; **and** if I did not confirm, nothing changes.
6. **Given** any action that writes to disk, **When** it executes, **Then** the tool only ever
   modifies skills in response to my explicit action — never as a side effect of viewing or analyzing.
7. **Given** an action fails (e.g., a file is not writable), **When** it errors, **Then** the tool
   reports the failure clearly and leaves the setup in its prior, consistent state.

---

### User Story 4 - Inspect activation and what the agent actually loaded (Priority: P3)

As an agent user, I want to see which skills are *triggerable* in a given context, which are currently
*active*, and — where the agent exposes it — what was *actually pulled into context* during a turn, so
I can consciously adopt or discard rather than treating context as a black box.

**Why this priority**: This addresses pain #2, but it is partially gated by what each agent runtime
chooses to expose. The parts the tool can compute from on-disk metadata (what exists, what could
trigger, what is active) are deliverable; the "exactly what this turn loaded" part depends on the
agent surfacing it and may be incomplete. P3 because it is the most vendor-dependent and should not
block the vendor-independent wins in US1–US3.

**Independent Test**: For an agent whose configuration is on disk, confirm the tool can show, for a
given project/context, which skills are eligible to trigger and which are active. Where the agent
emits a record of what it loaded, confirm the tool surfaces that record. Where it does not, confirm
the tool clearly marks that data as unavailable rather than fabricating it.

**Acceptance Scenarios**:

1. **Given** a project context, **When** I ask what skills apply, **Then** the tool shows which skills
   are eligible/triggerable and which are currently active in that context.
2. **Given** an agent that records what it loaded into context, **When** I inspect a turn, **Then** the
   tool shows which skills were actually read.
3. **Given** an agent that does NOT expose what it loaded, **When** I inspect a turn, **Then** the tool
   explicitly states that runtime context data is unavailable for this agent rather than guessing.

---

### User Story 5 - Span multiple agents from one view (Priority: P3)

As an agent user who uses more than one coding agent, I want the inspector to include skills from those
other agents in the same unified view, because each vendor only manages its own skills and I need one
place to see across all of them.

**Why this priority**: Cross-agent coverage is the long-term moat, but the user's daily, evidenced pain
lives primarily in their main agent. Shipping the main agent first proves the value; additional agent
sources extend the same inventory/overlap/control model. P3 because breadth is additive on top of a
working single-agent inspector.

**Independent Test**: On a machine with skills from two different agents, confirm both appear in one
inventory, each correctly labeled by originating agent, and that overlap detection can surface
duplicates that span agents.

**Acceptance Scenarios**:

1. **Given** skills installed for two different agents, **When** I open the inventory, **Then** skills
   from both appear in one list, each labeled by its originating agent.
2. **Given** two skills from different agents that do the same job, **When** overlap analysis runs,
   **Then** they can be grouped into the same cross-agent overlap cluster.

---

### Edge Cases

- **No skills installed**: The inventory shows a clear empty state explaining where the tool looked,
  not a blank screen.
- **A source directory is missing or unreadable**: The tool reports which source it could not read and
  still shows results from the sources it could, rather than failing entirely.
- **Duplicate skill IDs across sources** (same id installed twice): Both instances are shown and flagged
  as a duplicate-by-identity, distinct from a similarity-based overlap.
- **Skill metadata is malformed** (unparseable description/frontmatter): The skill is listed with an
  "incomplete metadata" marker, never silently omitted.
- **A skill is disabled outside the tool** (user hand-edits a file or the agent's own settings): On next
  scan the inventory reflects the real on-disk state, not a stale cached state — including a skill the
  user already turned off through the agent's native per-project mechanism.
- **A skill's source has no per-folder scoping** (e.g. a plugin skill that the agent only manages
  globally): The tool MUST surface that a disable here is global in effect, and MUST NOT silently fall
  back to a global change while implying it was folder-scoped.
- **Action on a read-only / permission-restricted location**: The tool refuses cleanly and explains why,
  leaving state untouched.
- **Overlap analysis finds no overlaps**: The tool says so plainly rather than inventing weak groupings.
- **Very large number of installed skills**: The inventory remains responsive to search/filter.
- **An agent exposes only partial runtime context data**: The tool shows what it has and marks the rest
  unavailable — it never fabricates which skills were loaded.

## Requirements *(mandatory)*

### Functional Requirements

**Inventory (US1)**

- **FR-001**: System MUST discover skills from multiple sources on the user's machine, including the
  agent's user-level skill location, the current project's skill location, and installed
  plugins/marketplaces, without the user manually pointing at each directory.
- **FR-002**: System MUST present discovered skills as one consolidated inventory, each entry showing
  at minimum: name, one-line description, originating source, on-disk location, and enabled/disabled
  state.
- **FR-003**: System MUST list every discovered skill exactly once per installed instance, and MUST
  flag — never silently drop — skills with missing or malformed metadata.
- **FR-004**: Users MUST be able to search and filter the inventory by name and by description keywords.
- **FR-005**: System MUST reflect the true current on-disk state on each scan, including changes made
  outside the tool.

**Overlap detection (US2)**

- **FR-006**: System MUST analyze the inventory and group skills that appear to serve the same purpose
  into overlap clusters.
- **FR-007**: System MUST present, for each cluster, a human-readable reason the members were grouped,
  and enough distinguishing detail per member for the user to decide which to keep.
- **FR-008**: System MUST distinguish duplicate-by-identity (the same skill installed more than once)
  from similarity-based overlap.
- **FR-009**: System MUST treat overlap groupings as review suggestions only, and MUST NOT alter any
  skill as a result of analysis.
- **FR-010**: System MUST avoid forcing unrelated skills into clusters and MUST clearly report when no
  overlaps are found.

**Control / actions (US3)**

- **FR-011**: Users MUST be able to disable and re-enable a skill from the interface, with disable
  being reversible and lossless — disabling a skill MUST NOT delete the skill's own files.
- **FR-023**: Where the agent provides a per-folder/per-project scoping mechanism, the system MUST be
  able to disable a skill **scoped to a single folder/project**, such that a globally-installed skill
  turned off in one folder remains active in others. This is the primary disable behavior and directly
  addresses the "I disabled it in one project and broke it everywhere" pain.
- **FR-024**: When a skill's source has **no** per-folder scoping mechanism (so disable can only take
  effect globally), the system MUST clearly communicate that the disable is global before applying it,
  and MUST NOT present it as folder-scoped.
- **FR-012**: Users MUST be able to remove a skill, and the system MUST require an explicit confirmation
  step before any destructive removal.
- **FR-013**: System MUST modify on-disk skill state ONLY in response to an explicit user action, never
  as a side effect of scanning, viewing, or analyzing.
- **FR-014**: System MUST, on any action failure, report the failure clearly and leave the setup in its
  prior consistent state.
- **FR-015**: System SHOULD allow users to organize/label skills (e.g., grouping) to manage a large set.

**Activation & context transparency (US4)**

- **FR-016**: System MUST show, for a given project/context, which skills are eligible to trigger and
  which are currently active.
- **FR-017**: System MUST surface, for agents that record it, which skills were actually loaded into
  context during a turn.
- **FR-018**: System MUST explicitly mark runtime context data as unavailable for agents that do not
  expose it, and MUST NOT fabricate or infer "what was loaded" when the data is absent.

**Cross-agent (US5)**

- **FR-019**: System MUST be able to include skills from more than one agent in the same inventory, each
  labeled by originating agent.
- **FR-020**: System MUST allow overlap clusters to span agents, surfacing cross-agent duplicates.

**Cross-cutting safety & honesty**

- **FR-021**: System MUST be safe-by-default: read and analysis operations never change the user's setup.
- **FR-022**: System MUST clearly communicate the boundary of what it knows — distinguishing verified
  on-disk facts from suggestions (overlap) and from data it cannot obtain (vendor-gated runtime context).

### Key Entities *(include if feature involves data)*

- **Skill**: A single installed capability. Key attributes: identity/name, description, originating
  source, on-disk location, enabled/disabled state, metadata-completeness, originating agent.
- **Skill Source**: An origin the tool scans — e.g., user-level location, project-level location, an
  installed plugin/marketplace, or another agent's location. Key attributes: kind, location,
  readability/availability.
- **Overlap Cluster**: A suggested grouping of skills judged to serve the same purpose. Key attributes:
  member skills, grouping basis/reason, cluster kind (similarity-based vs duplicate-by-identity).
- **Activation State**: For a given context, whether a skill is eligible to trigger and whether it is
  active. Key attributes: context/project, eligibility, active flag.
- **Context Load Record**: Where an agent exposes it, the record of which skills were actually loaded
  into context during a turn. Key attributes: turn/session reference, skills loaded, availability
  status (present vs unavailable for this agent).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: From a cold start, a user can see a complete, unified inventory of their installed skills
  in under 30 seconds — a task that is effectively impossible today without manually inspecting multiple
  directories.
- **SC-002**: The inventory discovers at least 95% of the skills actually installed across the supported
  sources on a representative machine (measured against a hand-built ground-truth list).
- **SC-003**: For a machine with known overlapping skill families, the tool's suggested overlap clusters
  match a human reviewer's judgment of "these overlap" for at least 80% of the known overlaps, with no
  more than a small, reviewable number of false groupings.
- **SC-004**: A user can identify and disable a redundant skill, and confirm the change took effect, in
  under one minute — versus an open-ended manual hunt today.
- **SC-005**: Zero unintended modifications: across normal use, no skill's on-disk state changes except
  as the direct result of an explicit user action.
- **SC-006**: For every skill the tool shows, the user can correctly tell which source/agent it came
  from and whether it is currently enabled (100% of inventory entries carry source + state).
- **SC-007**: Whenever runtime context data is unavailable for an agent, the tool says so explicitly in
  100% of such cases — it never presents fabricated "what was loaded" information.

## Assumptions

These are informed defaults chosen where the description left a detail open. They can be revised via
`/speckit-clarify`.

- **Primary agent first**: The MVP targets the user's primary coding agent (the one they use daily, and
  where the evidenced pain lives), with the inventory/overlap/control model architected to extend to
  additional agents (US5) incrementally. Cross-agent breadth is the differentiator delivered over time,
  not necessarily all at once in v1.
- **Local, on-machine tool**: The inspector operates on the user's own machine against locally installed
  skills. No cloud account or upload of the user's skill list is assumed.
- **Visual / GUI surface**: "視覺化甚至 GUI 操作" is taken to mean the inspector provides a visual,
  interactive surface for browsing the inventory, viewing overlap clusters, and taking actions — building
  on the existing graph/explorer visualization as a seed. A command surface may also exist but the visual
  view is in scope.
- **Mutation is in scope but bounded**: Disable/remove actions that write to disk are in scope (the user
  explicitly asked for "停用/移除"), but are strictly opt-in, confirmed for destructive cases, reversible
  for disable, and never triggered by analysis.
- **Disable prefers the agent's native per-project scoping**: Rather than always moving/deleting skill
  files, disable preferentially uses the agent's own per-folder toggle where one exists (for Claude Code,
  the `skillOverrides` map in a project's `.claude/settings.local.json`), because that is what makes a
  disable folder-scoped and leaves the user's authored files untouched. A file-move quarantine is the
  fallback only for sources with no scoped toggle (e.g. plugin skills), and is surfaced as a global
  disable. The native mechanism for agents beyond Claude Code (e.g. Codex) is deferred per-provider
  research, not an MVP blocker.
- **Overlap is advisory**: Overlap detection produces review suggestions, not automatic pruning. The
  human always decides what to keep.
- **Runtime context transparency is best-effort and vendor-gated**: The tool delivers the
  vendor-independent portion of pain #2 (what exists / what is triggerable / what is active) fully, and
  the "exactly what this turn loaded" portion only to the extent the agent exposes it — explicitly marking
  the rest unavailable rather than guessing.
- **This supersedes the compiler framing for this line of work**: `001-skill-compiler` was an
  authoring-side tool; this feature is the consumption-side inspector. The existing visualization work is
  reused as the seed; the compiler/authoring scope is out of scope here.
