# cite — the Skill & Context Inspector

A consumption-side **inspector + control panel** for the AI-agent skills already installed
on your machine. `cite` scans every skill across all of an agent's sources — user / global,
project, plugins, and other agents — surfaces duplicates and overlaps, and lets you toggle
skills **per-repo** from a local control-panel UI. Reads are pure; the only disk writes are
explicit, reversible toggles into the current repo's `.claude/settings.local.json`.

Safe-by-default is **structural**, not just disciplined: `scan` / `overlap` / `activation` /
`context` are pure-read modules; only `action/` (reached through `serve`) ever writes.

## Build

    cargo build --release        # produces `target/release/cite`

A single crate (`crates/skill-inspector`) over the standard library plus `serde` / `clap` /
`sha2` — no network-fetched runtime deps, fully offline. The skill walker (one level of
`read_dir`) and the loopback HTTP server are built on `std` alone.

## Commands

One binary, two subcommands.

```bash
# Pure-read: build one unified inventory + overlap clusters across all sources.
cite scan --agent claude-code --project . --format json
cite scan --format html --out inventory.html   # self-contained, server-less snapshot

# Interactive: local control-panel UI + action API, bound to 127.0.0.1 only.
cite serve --project .
```

`--agent` is repeatable (default `claude-code`); `--project` defaults to the cwd.

## What it shows

- **Unified inventory** — every installed skill across the user/global, project, and plugin
  roots (and any additional agents), each with name, description, source, on-disk path, and
  enabled/disabled state. Malformed metadata is flagged (`metadata_complete:false`), never
  dropped. A missing or unreadable source root degrades gracefully (classified, not fatal).

- **Overlap clusters** — deterministic, offline, lexical similarity (token-set Jaccard +
  TF-IDF cosine) plus duplicate-by-identity (shared content hash, or the same skill id
  installed under more than one source). Advisory only — clustering never mutates anything.

- **Per-repo toggle** — flip a skill off for **this repo only**, writing the current repo's
  `.claude/settings.local.json` and preserving every other key:
  - user / project skills → `skillOverrides["<id>"] = "off"`
  - plugin skills → a `permissions.deny` `Skill("<plugin>:<id>")` rule
  Flipping back removes the key/rule — byte-lossless. No other repo is ever touched, and the
  skill's own files are never modified.

- **Activation & context** — eligible/active is computed from disk; an agent's recorded
  runtime loads are surfaced where available and explicitly marked `unavailable` (never
  fabricated) where not.

## Source providers

Adding an agent is implementing one trait (`scan::source::SourceProvider`) and registering
it — the `<id>/SKILL.md` walker is shared; only root discovery differs per agent. The
shipped providers are Claude Code (user / project / plugin roots) and a generic other-agent
provider.

## Safety model

- `scan` reads only; it never writes a skill root or any settings file.
- The sole writers are the `serve` action endpoints, and they write **only** the current
  repo's `.claude/settings.local.json`, always preserving unrelated keys.
- The server binds `127.0.0.1` exclusively — nothing leaves the machine.
- The static HTML snapshot escapes injected JSON so a skill field can't break out of the
  embedded `<script>` block.
