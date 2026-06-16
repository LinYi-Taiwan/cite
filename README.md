# cite — the Skill & Context Inspector

A consumption-side **inspector + control panel** for the AI-agent skills already installed
on your machine. `cite` scans every skill across all of an agent's sources — user / global,
project, plugins, and other agents — surfaces duplicates and overlaps, and lets you toggle
skills **per-repo** from a local control-panel UI. Reads are pure; the only disk writes are
explicit, reversible toggles into the current repo's `.claude/settings.local.json`.

Safe-by-default is **structural**, not just disciplined: `scan` / `overlap` / `activation` /
`context` are pure-read modules; only `action/` (reached through `serve`) ever writes.

## Install

`cite` installs with Cargo, Rust's package manager. If you don't have Rust yet,
install it from [rustup.rs](https://rustup.rs) — one command, sets up `cargo` —
then:

    cargo install --git https://github.com/LinYi-Taiwan/cite --locked
    cite serve

This compiles `cite` and drops it in `~/.cargo/bin` (already on your `PATH` after
rustup).

Prefer not to install Rust? Grab a prebuilt binary from the
[Releases page](https://github.com/LinYi-Taiwan/cite/releases), extract, and move
`./cite` onto your `PATH`. Prebuilt targets: macOS (arm64 + x64) and Linux (x64).

To hack on it from a clone, see Build below.

## Build

    cargo build --release        # produces `target/release/cite`

A single crate (`crates/skill-inspector`) over the standard library plus `serde` / `clap` /
`sha2` — no network-fetched runtime deps, fully offline. The skill walker (one level of
`read_dir`) and the loopback HTTP server are built on `std` alone.

## Develop

The `cite` on your `PATH` is a **snapshot** copied by `cargo install` — it does *not* track
edits to this source tree. After changing any `.rs` or `assets/inspector.html`, either run from
source or reinstall, otherwise `cite serve` keeps serving the old build. The `Makefile` wraps
the loop:

| Target | What it does |
| --- | --- |
| `make dev` *(default)* | Run from source, always latest — Rust is recompiled and `inspector.html` is read from disk **per request** (`CITE_DEV_HTML`), so an HTML edit shows on a browser refresh without even restarting. Use this while developing. |
| `make scan` | One-shot read-only JSON inventory from the current source. The recipe is silenced, so stdout is clean JSON — safe to pipe (`make scan \| jq`). |
| `make install` | Recompile from this tree and overwrite the global `~/.cargo/bin/cite` (HTML is embedded at compile time, so it's fresh too — no `CITE_DEV_HTML` needed). |
| `make serve` | `install`, then serve via the freshly-installed global binary. |
| `make build` | `cargo build --release` → `target/release/cite`. |
| `make test` | Run the test suite. |

Override the inspected project or the port on any target — `PROJECT` defaults to the cwd,
`PORT` to an auto-picked free port:

    make dev PROJECT=/path/to/other/repo PORT=8787

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

- **Per-repo toggle** — flip a user / project skill off for **this repo only**, writing
  `skillOverrides["<id>"] = "off"` into the current repo's `.claude/settings.local.json` and
  preserving every other key. Flipping back removes the key — byte-lossless. No other repo is
  ever touched, and the skill's own files are never modified.

- **Whole-plugin switch** — a plugin is all-or-nothing: its skills can't be toggled
  individually. One switch enables/disables the entire plugin across **every** project, writing
  `enabledPlugins["<name>@<marketplace>"]` in the global `~/.claude/settings.json` (takes effect
  after `/reload-plugins` or restarting Claude Code).

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
- The only writers are the `serve` action endpoints. Per-repo skill toggles write **only** the
  current repo's `.claude/settings.local.json`; the whole-plugin switch writes the global
  `~/.claude/settings.json` `enabledPlugins` map. Both always preserve unrelated keys.
- The server binds `127.0.0.1` exclusively — nothing leaves the machine.
- The static HTML snapshot escapes injected JSON so a skill field can't break out of the
  embedded `<script>` block.
