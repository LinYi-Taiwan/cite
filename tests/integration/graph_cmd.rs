//! `skillc graph` — the whole-catalog dependency-graph export. The canonical format is
//! JSON; DOT / Mermaid / HTML are renderings of the same export. Layers are derived from
//! structure (mounted ⇒ template, imports ⇒ organism, none ⇒ molecule; blocks ⇒ atom),
//! and pulled cross-repo units keep their derived layer with `external: true`.

#[path = "common.rs"]
mod common;

use serde_json::Value;
use skillc_core::graph_export::GraphFormat;
use skillc_core::GraphOptions;

fn graph(fixture: &str, format: GraphFormat, target: Option<&str>, focus: Option<&str>) -> String {
    let opts = GraphOptions {
        catalog: common::fixture(fixture),
        config: None,
        format,
        target: target.map(String::from),
        focus: focus.map(String::from),
        depth: 1,
    };
    skillc_core::run_graph(&opts).expect("graph")
}

fn json(fixture: &str, target: Option<&str>, focus: Option<&str>) -> Value {
    serde_json::from_str(&graph(fixture, GraphFormat::Json, target, focus)).expect("valid json")
}

fn node<'a>(g: &'a Value, id: &str) -> &'a Value {
    g["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == id)
        .unwrap_or_else(|| panic!("node `{id}` missing"))
}

fn has_edge(g: &Value, from: &str, to: &str, kind: &str) -> bool {
    g["edges"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["from"] == from && e["to"] == to && e["kind"] == kind)
}

#[test]
fn json_export_derives_layers_and_marks_external() {
    let g = json("us2-markers-deps", None, None);

    // Mounted catalog skills are templates; the cross-repo dep keeps its derived layer
    // (no imports ⇒ molecule) and is marked external with its source id.
    assert_eq!(node(&g, "scm")["layer"], "page");
    assert_eq!(node(&g, "reviewer")["layer"], "template");
    let dep = node(&g, "code-review");
    assert_eq!(dep["layer"], "molecule");
    assert_eq!(dep["external"], true);
    assert_eq!(dep["source"], "shared-skills");
    assert_eq!(dep["targets"], serde_json::json!(["scm"]));

    assert!(has_edge(&g, "scm", "reviewer", "mounts"));
    assert!(has_edge(&g, "reviewer", "code-review", "imports"));
    let alias = g["edges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["from"] == "reviewer" && e["to"] == "code-review")
        .unwrap()["alias"]
        .clone();
    assert_eq!(alias, "CodeReview");
}

#[test]
fn includes_edges_and_atoms_appear() {
    let g = json("nested-discovery", None, None);
    assert_eq!(node(&g, "creds")["layer"], "atom");
    assert!(has_edge(&g, "leaf", "creds", "includes"));
    // composer is mounted ⇒ template even though it has imports.
    assert_eq!(node(&g, "composer")["layer"], "template");
    assert!(has_edge(&g, "composer", "leaf", "imports"));
}

#[test]
fn target_filter_keeps_only_the_bundle_closure() {
    let g = json("us2-markers-deps", Some("scm"), None);
    let ids: Vec<&str> = g["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"scm") && ids.contains(&"reviewer") && ids.contains(&"code-review"));
    // Every edge endpoint survives the filter.
    for e in g["edges"].as_array().unwrap() {
        assert!(ids.contains(&e["from"].as_str().unwrap()));
        assert!(ids.contains(&e["to"].as_str().unwrap()));
    }
}

#[test]
fn focus_filter_restricts_to_the_neighborhood() {
    // depth 1 around the external dep: its importers, but not the target node two hops up.
    let g = json("us2-markers-deps", None, Some("code-review"));
    let ids: Vec<&str> = g["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"code-review") && ids.contains(&"reviewer") && ids.contains(&"auditor"));
    assert!(
        !ids.contains(&"scm"),
        "two hops away must be cut at depth 1"
    );
}

#[test]
fn unknown_focus_is_a_usage_error() {
    let opts = GraphOptions {
        catalog: common::fixture("us2-markers-deps"),
        config: None,
        format: GraphFormat::Json,
        target: None,
        focus: Some("nope".into()),
        depth: 1,
    };
    assert!(matches!(
        skillc_core::run_graph(&opts),
        Err(skillc_core::PipelineError::Usage(_))
    ));
}

#[test]
fn unknown_target_is_a_usage_error() {
    let opts = GraphOptions {
        catalog: common::fixture("us2-markers-deps"),
        config: None,
        format: GraphFormat::Json,
        target: Some("nope".into()),
        focus: None,
        depth: 1,
    };
    assert!(matches!(
        skillc_core::run_graph(&opts),
        Err(skillc_core::PipelineError::Usage(_))
    ));
}

#[test]
fn text_renderings_carry_the_same_graph() {
    let dot = graph("us2-markers-deps", GraphFormat::Dot, None, None);
    assert!(dot.starts_with("digraph"));
    assert!(dot.contains("\"reviewer\" -> \"code-review\""));

    let mermaid = graph("us2-markers-deps", GraphFormat::Mermaid, None, None);
    assert!(mermaid.starts_with("flowchart TB"));
    assert!(mermaid.contains("reviewer -- CodeReview --> code_review"));

    let html = graph("us2-markers-deps", GraphFormat::Html, None, None);
    assert!(
        !html.contains("__GRAPH_JSON__"),
        "placeholder must be filled"
    );
    assert!(html.contains("\"code-review\""));
}
