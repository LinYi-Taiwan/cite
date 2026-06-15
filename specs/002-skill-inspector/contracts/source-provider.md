# Contract: SourceProvider (pluggable per-agent discovery)

Adding an agent (US5, FR-019) = implementing this trait and registering it. The walker (`<id>/SKILL.md`)
is shared; only **root discovery** differs per agent.

```rust
/// Discovers the skill roots for one agent on this machine.
pub trait SourceProvider {
    /// Stable agent label, e.g. "claude-code".
    fn agent(&self) -> &str;

    /// Enumerate the roots to scan. Each carries kind + path + availability.
    /// MUST classify a missing/unreadable root rather than erroring the whole scan.
    fn sources(&self, ctx: &ScanContext) -> Vec<SkillSource>;
}

pub struct ScanContext {
    pub project_root: PathBuf,   // for project-level + activation
    pub home: PathBuf,           // user-level discovery
}
```

## Obligations

- **Read-only.** A provider MUST NOT mutate the filesystem.
- **Graceful degradation.** A root that is missing/unreadable is returned with
  `availability = missing|unreadable` (never a hard error) so other sources still scan (edge case).
- **Honest labeling.** Every `SkillSource.id` is stable and unique; every yielded `Skill.agent` equals
  `provider.agent()` (FR-019, US5).

## MVP provider: `claude-code`

`sources()` returns, when present:
- `claude:user` — kind `user`, root `~/.claude/skills/`
- `claude:project` — kind `project`, root `<project_root>/.claude/skills/`
- `claude:plugin:<name>` — kind `plugin`, one per installed plugin's skill dir under the Claude config dir

> The exact plugin skill-root layout is verified on the real machine during implementation; a fixture
> mirrors whatever shape is found. This contract commits only to "the agent's documented skill
> locations", not a guessed absolute path.

## Walker (shared)

For each readable root: find immediate child dirs containing `SKILL.md`; parse frontmatter
(`name`/`description`) via the reused `skillc-core` parser; compute `content_hash`; set
`metadata_complete` false (not drop) on parse failure (FR-003).
