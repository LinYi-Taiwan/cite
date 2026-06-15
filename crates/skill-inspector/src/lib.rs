//! Skill & Context Inspector — a consumption-side inspector + control panel for installed
//! AI-agent skills. Read (`scan`/`overlap`/`activation`/`context`) and write (`action`) are
//! separated at the module boundary so safe-by-default is structural, not just disciplined.

pub mod action;
pub mod activation;
pub mod cli;
pub mod context;
pub mod export;
pub mod fsutil;
pub mod model;
pub mod overlap;
pub mod scan;
pub mod server;
pub mod settings;
pub mod state;
pub mod util_time;

use std::path::PathBuf;

use export::Export;
use scan::source::{Registry, ScanContext};
use state::InspectorState;

/// The embedded control-panel UI (extended from the reused `graph.html` seed).
pub const INSPECTOR_HTML: &str = include_str!("../assets/inspector.html");

/// Build a `ScanContext` from explicit project/home paths.
pub fn scan_context(project_root: PathBuf, home: PathBuf) -> ScanContext {
    ScanContext { project_root, home }
}

/// Build just the unified inventory (no overlap/activation/context). Used by the action layer
/// and tests, where the full export projection isn't needed.
pub fn build_inventory(
    agents: &[String],
    ctx: &ScanContext,
    state: &InspectorState,
) -> scan::Inventory {
    let registry = Registry::with_defaults();
    scan::scan(agents, ctx, state, &registry)
}

/// One-shot: scan → overlap → activation → context → full export. The single entry point
/// shared by `scan`, `serve`'s initial load, and the integration tests.
pub fn build_export(agents: &[String], ctx: &ScanContext, state: &InspectorState) -> Export {
    let inventory = build_inventory(agents, ctx, state);
    export::build(agents, ctx, inventory)
}

/// Render a self-contained static HTML snapshot: inject the export JSON into the UI so it
/// renders with no server (the `scan --format html` artifact).
///
/// `serde_json` does not escape `<`/`>`, so a skill field containing the literal `</script>`
/// would otherwise close the injected `<script>` block and execute (the snapshot is a
/// shareable file opened in a browser). Escaping `</` as `<\/` is safe per the JSON spec —
/// `\/` parses back to `/` — and prevents any `</script>` (or HTML-comment) breakout.
pub fn render_static_html(export_json: &str) -> String {
    let safe_json = export_json.replace("</", "<\\/");
    let injected = format!("<script>window.__EXPORT__ = {safe_json};</script>");
    if INSPECTOR_HTML.contains("<!--INVENTORY_DATA-->") {
        INSPECTOR_HTML.replace("<!--INVENTORY_DATA-->", &injected)
    } else {
        // Fallback: inject right before </body>.
        INSPECTOR_HTML.replace("</body>", &format!("{injected}\n</body>"))
    }
}

fn home_dir() -> PathBuf {
    std::env::var("HOME").map(PathBuf::from).unwrap_or_default()
}

fn project_root(opt: Option<PathBuf>) -> PathBuf {
    opt.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

/// Run one inspector subcommand, returning a process exit code (0 = success). Shared by the
/// standalone `skill-inspector` binary and the `cite inspect` subcommand.
pub fn run(command: cli::Command) -> i32 {
    match command {
        cli::Command::Scan(args) => run_scan(args),
        cli::Command::Serve(args) => run_serve(args),
    }
}

fn run_scan(args: cli::ScanArgs) -> i32 {
    let agents = cli::agents_or_default(&args.agent);
    let ctx = scan_context(project_root(args.project), home_dir());
    let state = InspectorState::load();

    // Pure-read: never writes to a skill root or inspector-state.json (FR-013/021/SC-005).
    let export = build_export(&agents, &ctx, &state);
    let json = match serde_json::to_string_pretty(&export) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("error: failed to serialize export: {e}");
            return 1;
        }
    };

    let output = match args.format {
        cli::Format::Json => json,
        cli::Format::Html => render_static_html(&json),
    };

    match args.out {
        Some(path) => {
            if let Err(e) = std::fs::write(&path, output) {
                eprintln!("error: cannot write {}: {e}", path.display());
                return 1;
            }
        }
        None => println!("{output}"),
    }
    0
}

fn run_serve(args: cli::ServeArgs) -> i32 {
    let agents = cli::agents_or_default(&args.agent);
    let config = server::ServeConfig {
        agents,
        project_root: project_root(args.project),
        home: home_dir(),
        state_path: InspectorState::state_path(),
        quarantine_dir: InspectorState::quarantine_dir(),
        trash_dir: InspectorState::trash_dir(),
    };
    match server::serve(config, args.port) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: serve failed: {e}");
            1
        }
    }
}
