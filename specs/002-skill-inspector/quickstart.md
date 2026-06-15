# Quickstart: Skill & Context Inspector

Runnable validation scenarios that prove each user story end-to-end. Details live in
[data-model.md](./data-model.md) and [contracts/](./contracts/); this is the run/verify guide.

## Prerequisites

- Rust 1.83+ (existing workspace toolchain).
- `crates/skill-inspector` added to the workspace `members`.
- Fixture skill-root trees under `tests/fixtures/` (synthetic — never touch the real `~/.claude`).

```bash
cargo build --release            # -> target/release/skill-inspector
```

A fixture tree models multiple sources, e.g.:

```text
tests/fixtures/multi-source/
├── home/.claude/skills/
│   ├── code-review/SKILL.md            # name+desc: "Review a diff…"
│   ├── react-code-review/SKILL.md      # overlaps code-review (similarity)
│   └── broken/SKILL.md                 # malformed frontmatter (metadata_complete=false)
├── home/.claude/plugins/devkit/skills/
│   └── devkit.typescript.code-review/SKILL.md   # overlaps code-review
└── project/.claude/skills/
    └── qa-playwright/SKILL.md          # also exists in home → duplicate-identity
```

---

## US1 — unified inventory (P1)

```bash
skill-inspector scan --agent claude-code \
  --project tests/fixtures/multi-source/project \
  --format json > /tmp/inv.json
```

**Expect** (assert against `/tmp/inv.json`, see `contracts/inventory-export.md`):
- `skills[]` lists every fixture skill **once**, each with `source_id`, `agent`, `state`, `path`.
- `broken` appears with `metadata_complete: false` (FR-003) — not dropped.
- `sources[]` shows the project source `readable` and any missing root as `missing` (edge), scan exits 0.
- HTML form renders in a browser via the reused `graph.html`: `scan --format html --out /tmp/inv.html`.

## US2 — overlap clusters (P1)

Same `scan`. **Expect** in `clusters[]`:
- A `similarity` cluster grouping `code-review` + `react-code-review` + `devkit.typescript.code-review`
  with a human-readable `reason` (FR-007) and a `score`.
- A `duplicate-identity` cluster for the two `qa-playwright` instances (FR-008).
- `qa-playwright` is NOT force-merged into the code-review similarity cluster (FR-010, no false group).
- On a fixture with no overlaps, `clusters: []` **and** `clusters_empty_reason` is a non-null string.
- No skill changed on disk by scanning (re-run `scan`, diff fixture tree → identical; SC-005).

## US3 — act safely (P2)

```bash
skill-inspector serve --agent claude-code --project tests/fixtures/multi-source/project --port 7777 &
# disable
curl -s localhost:7777/api/disable -d '{"skill_key":"claude-code/claude:user/react-code-review"}'
# re-enable
curl -s localhost:7777/api/enable  -d '{"skill_key":"claude-code/claude:user/react-code-review"}'
# remove WITHOUT confirm → rejected
curl -s localhost:7777/api/remove  -d '{"skill_key":"…/broken"}'                    # ok:false
# remove WITH confirm → backed up then deleted
curl -s localhost:7777/api/remove  -d '{"skill_key":"…/broken","confirm":true}'      # ok:true, backup path
```

**Expect**:
- After disable, the skill dir is in quarantine + a `QuarantineRecord` exists; the skill no longer
  appears active. After enable, it is back at its **exact** original path, byte-identical (FR-011).
- Unconfirmed remove changes nothing (FR-012); confirmed remove leaves a restorable backup.
- A write to a read-only fixture root returns `ok:false` and leaves the tree unchanged (FR-014).

## US4 — activation & context transparency (P3)

```bash
skill-inspector scan --agent claude-code --project tests/fixtures/multi-source/project --format json
```

**Expect**:
- `activation[]` marks active (non-quarantined) skills `eligible/active`; a quarantined skill is not.
- `context_loads[]`: for a fixture carrying a recorded transcript → an entry with
  `availability:"present"` listing loaded skills; for an agent with no record → `availability:"unavailable"`
  with empty `loaded_skill_keys` (FR-018, SC-007) — **never fabricated**.

## US5 — span multiple agents (P3)

Add a second-agent fixture root and a `--agent <other>` provider.

```bash
skill-inspector scan --agent claude-code --agent <other-agent> --project … --format json
```

**Expect**: `skills[]` includes both agents, each correctly labeled; a `duplicate`/`similarity` cluster
can span agents (FR-019/020).

---

## Automated equivalents

```bash
cargo test -p skill-inspector            # scan_inventory, overlap_clusters, action_roundtrip (insta snapshots)
cargo insta review                       # review inventory/cluster snapshots on first run
```

| Scenario | Test | Success criterion |
|---|---|---|
| Unified inventory across sources | `scan_inventory.rs` | SC-002, SC-006, FR-003 |
| Overlap + no false grouping + empty case | `overlap_clusters.rs` | SC-003, FR-008/010 |
| Disable→enable lossless; scan never mutates | `action_roundtrip.rs` | FR-011, SC-005 |
| Never-fabricate context | `scan_inventory.rs` (context fixture) | FR-018, SC-007 |
