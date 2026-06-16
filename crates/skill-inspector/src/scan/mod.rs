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

    for agent in agents {
        let Some(provider) = registry.get(agent) else {
            continue;
        };
        for source in provider.sources(ctx) {
            if source::classify_availability(std::path::Path::new(&source.root))
                == crate::model::Availability::Readable
            {
                for walked in skill_md::walk_root(std::path::Path::new(&source.root)) {
                    let mut skill = Skill {
                        id: walked.id,
                        name: walked.name,
                        description: walked.description,
                        source_id: source.id.clone(),
                        agent: agent.clone(),
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
                    skills.push(skill);
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
