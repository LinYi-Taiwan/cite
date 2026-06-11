//! Dependency graph analysis (T016): `@include` missing/cycle/unused detection via
//! `petgraph`. Import-graph dedup + version-conflict detection is added in US2 (T025).

use std::collections::{BTreeMap, BTreeSet};

use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};

use crate::diagnostics::{Code, Diagnostics};
use crate::model::Catalog;

/// Analyze the `@include` graph: report missing includes (FR-003), cycles (FR-004), and
/// unused blocks (FR-005). Returns `false` if any hard error (missing/cycle) was emitted.
pub fn analyze_includes(catalog: &Catalog, diags: &mut Diagnostics) -> bool {
    let mut ok = true;

    // 1. Missing-include detection, naming skill/block + the missing block (FR-003).
    for skill in catalog.skills.values() {
        for inc in &skill.includes {
            if !catalog.blocks.contains_key(inc) {
                diags.error(
                    Code::IncludeMissing,
                    format!(
                        "skill `{}` @includes block `{}` which does not exist",
                        skill.id, inc
                    ),
                );
                ok = false;
            }
        }
    }
    for block in catalog.blocks.values() {
        for inc in &block.includes {
            if !catalog.blocks.contains_key(inc) {
                diags.error(
                    Code::IncludeMissing,
                    format!(
                        "block `{}` @includes block `{}` which does not exist",
                        block.id, inc
                    ),
                );
                ok = false;
            }
        }
    }

    // 2. Cycle detection over the block→block include subgraph (FR-004).
    let (g, _idx) = build_block_graph(catalog);
    if let Err(cycle) = toposort(&g, None) {
        let chain = describe_cycle(&g, cycle.node_id());
        diags.error(Code::Cycle, format!("@include cycle: {chain}"));
        ok = false;
    }

    // 3. Unused blocks: not reachable from any skill via include edges (FR-005).
    let reachable = reachable_blocks(catalog);
    for id in catalog.blocks.keys() {
        if !reachable.contains(id.as_str()) {
            diags.warning(
                Code::BlockUnused,
                format!("block `{id}` is included by no skill"),
            );
        }
    }

    ok
}

/// Build a directed graph of block→block include edges. Node weight is the block id.
fn build_block_graph(catalog: &Catalog) -> (DiGraph<&str, ()>, BTreeMap<&str, NodeIndex>) {
    let mut g = DiGraph::<&str, ()>::new();
    let mut idx = BTreeMap::new();
    for id in catalog.blocks.keys() {
        idx.insert(id.as_str(), g.add_node(id.as_str()));
    }
    for block in catalog.blocks.values() {
        let from = idx[block.id.as_str()];
        for inc in &block.includes {
            if let Some(&to) = idx.get(inc.as_str()) {
                g.add_edge(from, to, ());
            }
        }
    }
    (g, idx)
}

/// Render a readable cycle chain like `a → b → a`.
///
/// `start` is on a cycle (toposort failed). We restrict the walk to the strongly-connected
/// component containing `start` so we follow real cycle edges, not a dead-end branch off the
/// cycle node (e.g. `a→b, a→c, c→a` must report `a → c → a`, never `a → b → …`).
fn describe_cycle(g: &DiGraph<&str, ()>, start: NodeIndex) -> String {
    let scc: BTreeSet<NodeIndex> = petgraph::algo::tarjan_scc(g)
        .into_iter()
        .find(|c| c.contains(&start))
        .map(|c| c.into_iter().collect())
        .unwrap_or_else(|| std::iter::once(start).collect());

    let mut path = vec![start];
    let mut current = start;
    for _ in 0..=scc.len() {
        // Only follow edges that stay inside the cycle's SCC.
        match g.neighbors(current).find(|n| scc.contains(n)) {
            Some(n) if n == start => break,
            Some(n) if path.contains(&n) => break,
            Some(n) => {
                path.push(n);
                current = n;
            }
            None => break,
        }
    }
    let mut names: Vec<&str> = path.iter().map(|&n| g[n]).collect();
    names.push(g[start]);
    names.join(" → ")
}

/// The set of block ids reachable from some skill's `@include`s (transitively).
fn reachable_blocks(catalog: &Catalog) -> BTreeSet<&str> {
    let mut seen = BTreeSet::new();
    let mut stack: Vec<&str> = catalog
        .skills
        .values()
        .flat_map(|s| s.includes.iter().map(String::as_str))
        .collect();
    while let Some(id) = stack.pop() {
        let Some(block) = catalog.blocks.get(id) else {
            continue;
        };
        if seen.insert(id) {
            for inc in &block.includes {
                stack.push(inc.as_str());
            }
        }
    }
    seen
}
