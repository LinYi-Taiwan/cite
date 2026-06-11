# Test fixtures

One fixture catalog per acceptance scenario / edge case (the convention from
`plan.md` Testing). Each directory under here is a self-contained mini-catalog:

```
tests/fixtures/<scenario>/
├── skillc.config.yaml      # framework config (target registry, agents, sharedReferences)
├── skills/                 # authored SKILL.md units + their references/
├── blocks/                 # reusable @include blocks
└── (optional) sources/     # local mirrors standing in for cross-repo Repo(<id>) imports
```

Integration tests in `tests/integration/` compile a fixture for a given
`(target, agent)` and assert on the artifact + diagnostics. Golden artifact bytes
are snapshotted with `insta`; run `cargo insta review` to accept intentional changes.

Determinism is a hard requirement: a fixture compiled twice with `--locked --frozen`
must be byte-identical (asserted in `us5_artifact.rs`).
