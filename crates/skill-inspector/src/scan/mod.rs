//! Scan orchestration (FR-001..005): discover sources via the registry → walk each readable
//! root → assemble the `Skill` list, merging `InspectorState` (quarantine + folder overrides
//! + labels) and the target folder's `skillOverrides` to set `state`. Pure-read: never mutates.

pub mod claude;
pub mod frontmatter;
pub mod other_agent;
pub mod skill_md;
pub mod source;

use crate::model::{split_key, Skill, SkillSource, SkillState};
use crate::settings;
use crate::state::InspectorState;
use source::{Registry, ScanContext};

/// The consolidated, deterministic result of a scan.
pub struct Inventory {
    pub sources: Vec<SkillSource>,
    pub skills: Vec<Skill>,
}

impl Inventory {
    pub fn find(&self, skill_key: &str) -> Option<&Skill> {
        self.skills.iter().find(|s| s.key() == skill_key)
    }
}

/// Build the unified inventory across the requested agents.
pub fn scan(
    agents: &[String],
    ctx: &ScanContext,
    state: &InspectorState,
    registry: &Registry,
) -> Inventory {
    let mut sources: Vec<SkillSource> = Vec::new();
    let mut skills: Vec<Skill> = Vec::new();

    // Per-repo disable is derived from the project's settings.local.json — never any other repo.
    // Two mechanisms, by source: `skillOverrides[<id>]="off"` for user/project skills (disable a
    // *global* skill for this folder only), and a `permissions.deny` `Skill(<plugin>:<id>)` rule
    // for plugin skills (which skillOverrides can't touch). Either ⇒ `DisabledInFolder`.
    let folder_overrides = settings::read_skill_overrides(&ctx.project_root);
    let folder_denies = settings::read_skill_denies(&ctx.project_root);
    // Machine-wide: Claude only loads a plugin that is explicitly enabled. A plugin that is
    // `false` OR absent from `enabledPlugins` is inert in EVERY project — its skills still sit on
    // disk (so they scan) but none can trigger ⇒ `DisabledPlugin`. Global, not per-repo, and it
    // dominates a folder override (the plugin being off is the reason). So: active iff the bare
    // plugin name is in this enabled set.
    let enabled_plugins = settings::read_enabled_plugin_names(&ctx.home, &ctx.project_root);
    let is_off_in_folder = |source_id: &str, id: &str| {
        if let Some(name) = claude::plugin_skill_perm_name(source_id, id) {
            folder_denies.contains(&name)
        } else if claude::supports_folder_scope(source_id) {
            // Only user/project skills are keyed by bare `id` in skillOverrides — gate on it so a
            // non-Claude skill sharing an `id` with an overridden one isn't shown wrongly disabled.
            folder_overrides
                .get(id)
                .map(|v| v == "off")
                .unwrap_or(false)
        } else {
            false
        }
    };

    // Turn a walked entry (skill OR command) into a `Skill` with disk-derived state + labels.
    // Shared so `commands/` entries get identical folder-override / plugin-kill / label handling.
    let build_skill = |walked: skill_md::WalkedSkill, source_id: &str, agent: &str| -> Skill {
        let mut skill = Skill {
            id: walked.id,
            name: walked.name,
            description: walked.description,
            source_id: source_id.to_string(),
            agent: agent.to_string(),
            path: walked.path.to_string_lossy().into_owned(),
            content_hash: walked.content_hash,
            state: SkillState::Active,
            metadata_complete: walked.metadata_complete,
            labels: Vec::new(),
        };
        if is_off_in_folder(&skill.source_id, &skill.id) {
            skill.state = SkillState::DisabledInFolder;
        }
        // Plugin-level kill wins: a plugin not in the enabled set (false or absent) is
        // inert everywhere regardless of any per-repo rule.
        if let Some(plugin) = skill.source_id.strip_prefix("claude:plugin:") {
            if !enabled_plugins.contains(plugin) {
                skill.state = SkillState::DisabledPlugin;
            }
        }
        skill.labels = state.labels.get(&skill.key()).cloned().unwrap_or_default();
        skill
    };

    for agent in agents {
        let Some(provider) = registry.get(agent) else {
            continue;
        };
        for source in provider.sources(ctx) {
            let root = std::path::Path::new(&source.root);
            // The source root is `<base>/skills`; `<base>` is the plugin install dir (or
            // `~/.claude` / `<proj>/.claude` for user/project). A *plugin* declares what it loads in
            // `<base>/.claude-plugin/plugin.json` — and Claude loads exactly those declared paths,
            // which need NOT be `skills/` or `commands/` (it could put a command under `test/`). So
            // honor the manifest first; only fall back to the convention dirs for a kind the
            // manifest doesn't declare. User/project roots have no manifest ⇒ always convention.
            let base = root.parent();
            let manifest = if claude::is_plugin_source(&source.id) {
                base.map(claude::read_plugin_manifest)
            } else {
                None
            };

            // Skills: declared dirs if the manifest lists them, else convention `<base>/skills`.
            match manifest.as_ref().and_then(|m| m.skills.as_ref()) {
                Some(dirs) => {
                    for dir in dirs {
                        // A declared entry is normally a leaf skill dir (holds `SKILL.md`). If it
                        // instead points at a container (e.g. `./skills`), fall back to walking its
                        // children so the skills aren't silently lost.
                        if let Some(walked) = skill_md::walk_skill_dir(dir) {
                            skills.push(build_skill(walked, &source.id, agent));
                        } else {
                            for walked in skill_md::walk_root(dir) {
                                skills.push(build_skill(walked, &source.id, agent));
                            }
                        }
                    }
                }
                None => {
                    if source::classify_availability(root) == crate::model::Availability::Readable {
                        for walked in skill_md::walk_root(root) {
                            skills.push(build_skill(walked, &source.id, agent));
                        }
                    }
                }
            }

            // Commands the agent also loads: declared files if listed, else convention
            // `<base>/commands/**/*.md`. A command is a skill from the user's view, so it lands in
            // the same inventory. (Independent of the skills-root being readable — a root can ship
            // commands without a `skills/` dir.)
            match manifest.as_ref().and_then(|m| m.commands.as_ref()) {
                Some(files) => {
                    for file in files {
                        if let Some(walked) = skill_md::walk_command_file(file) {
                            skills.push(build_skill(walked, &source.id, agent));
                        }
                    }
                }
                None => {
                    if let Some(base) = base {
                        for walked in skill_md::walk_commands_root(&base.join("commands")) {
                            skills.push(build_skill(walked, &source.id, agent));
                        }
                    }
                }
            }
            sources.push(source);
        }
    }

    // Merge Tier-2 quarantined skills (moved out of their active root) so they still appear,
    // honestly flagged `disabled-global`. Reconcile to real disk: skip a stale record whose
    // quarantine dir no longer exists (FR-005).
    for rec in &state.quarantine {
        let Some((rec_agent, source_id, id)) = split_key(&rec.skill_key) else {
            continue;
        };
        if !agents.iter().any(|a| a == &rec_agent) {
            continue;
        }
        if skills.iter().any(|s| s.key() == rec.skill_key) {
            continue; // already surfaced from an active root (record is stale; on-disk wins)
        }
        let qpath = std::path::Path::new(&rec.quarantine_path);
        if !qpath.is_dir() {
            continue;
        }
        let (name, description, metadata_complete) = read_meta(qpath);
        skills.push(Skill {
            id,
            name,
            description,
            source_id,
            agent: rec_agent,
            path: rec.quarantine_path.clone(),
            content_hash: rec.content_hash.clone(),
            state: SkillState::DisabledGlobal,
            metadata_complete,
            labels: state
                .labels
                .get(&rec.skill_key)
                .cloned()
                .unwrap_or_default(),
        });
    }

    sources.sort_by(|a, b| a.id.cmp(&b.id));
    skills.sort_by_key(|s| s.key());
    Inventory { sources, skills }
}

/// Read name/description/completeness from a skill dir (used for quarantined skills).
fn read_meta(dir: &std::path::Path) -> (Option<String>, Option<String>, bool) {
    let skill_md = dir.join("SKILL.md");
    match std::fs::read_to_string(&skill_md) {
        Ok(text) => match frontmatter::parse(&text, &skill_md) {
            Ok(parsed) => {
                let name = parsed.frontmatter.name.clone();
                let description = parsed.frontmatter.description.clone();
                let complete = name.is_some() && description.is_some();
                (name, description, complete)
            }
            Err(_) => (None, None, false),
        },
        Err(_) => (None, None, false),
    }
}
