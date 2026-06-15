//! CLI surface (T011): `clap` arg structs for `build`/`install`, dispatch into the
//! `skillc-core` pipeline, and exit-code mapping per `contracts/cli.md`.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use skillc_core::config::FrameworkConfig;
use skillc_core::diagnostics::{Code, Diagnostic, Diagnostics, ExitCode, Severity};
use skillc_core::graph_export::GraphFormat;
use skillc_core::{
    BuildOptions, CheckOptions, GraphOptions, InstallOptions, PipelineError, WhyOptions,
};

#[derive(Debug, Parser)]
#[command(
    name = "cite",
    about = "Compile reusable AI-agent skills into per-(target, agent) artifacts.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Compile one or more (target, agent) combinations into inspectable artifacts.
    Build(BuildArgs),
    /// Validate the catalog without writing an artifact (CI / pre-commit gate).
    Check(CheckArgs),
    /// Explain a skill's or block's reverse dependencies: who includes/imports it and
    /// which targets ship it.
    Why(WhyArgs),
    /// Export the catalog's dependency graph (targets / skills / blocks / references and
    /// the mounts / imports / includes / hosts edges) as html, json, dot, or mermaid.
    Graph(GraphArgs),
    /// Place a previously-built artifact into an agent destination.
    Install(InstallArgs),
    /// Inspect the skills installed on THIS machine (consumption side): build a unified
    /// inventory + overlap clusters (`scan`), or open the interactive control panel (`serve`).
    Inspect(InspectArgs),
}

/// `cite inspect <scan|serve>` — delegates to the `skill-inspector` crate.
#[derive(Debug, Args)]
struct InspectArgs {
    #[command(subcommand)]
    command: skill_inspector::cli::Command,
}

#[derive(Debug, Args)]
struct BuildArgs {
    /// Target name; must exist in config. Required unless --all-targets.
    #[arg(long)]
    target: Option<String>,
    /// Agent format; must exist in config. Required unless --all-agents.
    #[arg(long)]
    agent: Option<String>,
    /// Root of the skill catalog.
    #[arg(long, default_value = ".")]
    catalog: PathBuf,
    /// Framework config (default: skillc.config.yaml in the catalog).
    #[arg(long)]
    config: Option<PathBuf>,
    /// Artifact output root.
    #[arg(long, default_value = "dist")]
    out: PathBuf,
    /// Cartesian build over every registered target.
    #[arg(long)]
    all_targets: bool,
    /// Cartesian build over every registered agent.
    #[arg(long)]
    all_agents: bool,
    /// Fail if skills-lock.json would change (CI mode).
    #[arg(long)]
    locked: bool,
    /// Use the lockfile only; never touch the network.
    #[arg(long)]
    frozen: bool,
}

#[derive(Debug, Args)]
struct CheckArgs {
    /// Restrict the per-target checks to one target (default: every registered target).
    #[arg(long)]
    target: Option<String>,
    /// Root of the skill catalog.
    #[arg(long, default_value = ".")]
    catalog: PathBuf,
    /// Framework config (default: skillc.config.yaml in the catalog).
    #[arg(long)]
    config: Option<PathBuf>,
    /// Require every used cross-repo source to carry a verified contentHash pin.
    #[arg(long)]
    locked: bool,
    /// Use the lockfile only; never touch the network.
    #[arg(long)]
    frozen: bool,
}

#[derive(Debug, Args)]
struct WhyArgs {
    /// The skill or block id to explain.
    id: String,
    /// Root of the skill catalog.
    #[arg(long, default_value = ".")]
    catalog: PathBuf,
    /// Framework config (default: skillc.config.yaml in the catalog).
    #[arg(long)]
    config: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct GraphArgs {
    /// Output format: html (self-contained viewer), json (canonical), dot, mermaid.
    #[arg(long, default_value = "html")]
    format: String,
    /// Restrict to one target's bundle closure.
    #[arg(long)]
    target: Option<String>,
    /// Restrict to the neighborhood of one skill/block/reference id.
    #[arg(long)]
    focus: Option<String>,
    /// Hop radius around --focus.
    #[arg(long, default_value_t = 2)]
    depth: usize,
    /// Output file. Defaults: html → dist/graph.html; other formats → stdout.
    #[arg(long)]
    out: Option<PathBuf>,
    /// Open the written file (html) in the default browser.
    #[arg(long)]
    open: bool,
    /// Root of the skill catalog.
    #[arg(long, default_value = ".")]
    catalog: PathBuf,
    /// Framework config (default: skillc.config.yaml in the catalog).
    #[arg(long)]
    config: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct InstallArgs {
    /// A directory produced by `build`.
    #[arg(long)]
    artifact: PathBuf,
    /// Agent's destination root.
    #[arg(long)]
    dest: PathBuf,
    /// Override/confirm output format; defaults to the artifact's recorded agent.
    #[arg(long)]
    agent: Option<String>,
}

/// Parse args and run; returns the process exit code.
pub fn run() -> i32 {
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            // clap prints help/usage; its own exit convention maps cleanly to "usage".
            let _ = e.print();
            return if e.use_stderr() {
                ExitCode::Usage.code()
            } else {
                ExitCode::Success.code()
            };
        }
    };

    match cli.command {
        Command::Build(args) => run_build_cmd(args),
        Command::Check(args) => run_check_cmd(args),
        Command::Why(args) => run_why_cmd(args),
        Command::Graph(args) => run_graph_cmd(args),
        Command::Install(args) => run_install_cmd(args),
        Command::Inspect(args) => skill_inspector::run(args.command),
    }
}

fn run_check_cmd(args: CheckArgs) -> i32 {
    let opts = CheckOptions {
        catalog: args.catalog,
        config: args.config,
        target: args.target,
        locked: args.locked,
        frozen: args.frozen,
    };
    match skillc_core::run_check(&opts) {
        Ok(diags) => {
            print_diagnostics(&diags);
            eprintln!("cite: check passed");
            ExitCode::Success.code()
        }
        Err(PipelineError::Usage(d)) => {
            eprintln!("{d}");
            ExitCode::Usage.code()
        }
        Err(PipelineError::Hard(diags)) => {
            print_diagnostics(&diags);
            ExitCode::HardFailure.code()
        }
    }
}

fn run_why_cmd(args: WhyArgs) -> i32 {
    let opts = WhyOptions {
        catalog: args.catalog,
        config: args.config,
        id: args.id,
    };
    match skillc_core::run_why(&opts) {
        Ok(text) => {
            print!("{text}");
            ExitCode::Success.code()
        }
        Err(PipelineError::Usage(d)) => {
            eprintln!("{d}");
            ExitCode::Usage.code()
        }
        Err(PipelineError::Hard(diags)) => {
            print_diagnostics(&diags);
            ExitCode::HardFailure.code()
        }
    }
}

fn run_graph_cmd(args: GraphArgs) -> i32 {
    let Some(format) = GraphFormat::parse(&args.format) else {
        eprintln!(
            "{}",
            Diagnostic::error(
                Code::ConfigInvalid,
                format!(
                    "unknown format `{}` (expected html, json, dot, or mermaid)",
                    args.format
                ),
            )
        );
        return ExitCode::Usage.code();
    };

    let opts = GraphOptions {
        catalog: args.catalog.clone(),
        config: args.config,
        format,
        target: args.target,
        focus: args.focus,
        depth: args.depth,
    };
    let rendered = match skillc_core::run_graph(&opts) {
        Ok(r) => r,
        Err(PipelineError::Usage(d)) => {
            eprintln!("{d}");
            return ExitCode::Usage.code();
        }
        Err(PipelineError::Hard(diags)) => {
            print_diagnostics(&diags);
            return ExitCode::HardFailure.code();
        }
    };

    // html defaults to a file (it's for a browser); text formats default to stdout.
    let out = args.out.or_else(|| {
        (format == GraphFormat::Html).then(|| args.catalog.join("dist").join("graph.html"))
    });
    match out {
        None => {
            print!("{rendered}");
            ExitCode::Success.code()
        }
        Some(path) => {
            if let Some(parent) = path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    eprintln!(
                        "{}",
                        Diagnostic::error(
                            Code::ConfigInvalid,
                            format!("cannot create `{}`: {e}", parent.display()),
                        )
                    );
                    return ExitCode::Usage.code();
                }
            }
            if let Err(e) = std::fs::write(&path, rendered) {
                eprintln!(
                    "{}",
                    Diagnostic::error(
                        Code::ConfigInvalid,
                        format!("cannot write `{}`: {e}", path.display()),
                    )
                );
                return ExitCode::Usage.code();
            }
            eprintln!("cite: wrote {}", path.display());
            if args.open {
                open_in_browser(&path);
            }
            ExitCode::Success.code()
        }
    }
}

/// Best-effort `open` — failure to launch a browser must not fail the command.
fn open_in_browser(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let (program, args): (&str, &[&str]) = ("open", &[]);
    #[cfg(target_os = "windows")]
    let (program, args): (&str, &[&str]) = ("cmd", &["/c", "start", ""]);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let (program, args): (&str, &[&str]) = ("xdg-open", &[]);
    if let Err(e) = std::process::Command::new(program)
        .args(args)
        .arg(path)
        .spawn()
    {
        eprintln!("cite: could not open `{}`: {e}", path.display());
    }
}

fn run_build_cmd(args: BuildArgs) -> i32 {
    // Resolve the (target, agent) matrix. --all-* expand against the config registry.
    let targets = match resolve_dimension(args.all_targets, &args.target, "target") {
        Ok(Some(explicit)) => vec![explicit],
        Ok(None) => Vec::new(), // expand from config
        Err(code) => return code,
    };
    let agents = match resolve_dimension(args.all_agents, &args.agent, "agent") {
        Ok(Some(explicit)) => vec![explicit],
        Ok(None) => Vec::new(),
        Err(code) => return code,
    };

    // If any dimension is "all", load the config to enumerate the registry.
    let (targets, agents) = if args.all_targets || args.all_agents {
        let cfg = match FrameworkConfig::discover(&args.catalog, args.config.as_deref()) {
            Ok(c) => c,
            Err(d) => {
                eprintln!("{d}");
                return ExitCode::Usage.code();
            }
        };
        let targets = if args.all_targets {
            cfg.target_names().into_iter().collect()
        } else {
            targets
        };
        let agents = if args.all_agents {
            cfg.agents().to_vec()
        } else {
            agents
        };
        (targets, agents)
    } else {
        (targets, agents)
    };

    // Cartesian builds re-run catalog-wide analyses per (target, agent); identical
    // diagnostic lines repeated N× are noise (one bad refactor × 20 combos = hundreds of
    // duplicate lines). Print each distinct line once; count the suppressed repeats.
    let mut printed: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut suppressed: usize = 0;
    let mut print_once = |diags: &Diagnostics| {
        for d in diags.iter() {
            let line = d.to_string();
            if printed.insert(line.clone()) {
                eprintln!("{line}");
            } else {
                suppressed += 1;
            }
        }
    };

    let mut worst = ExitCode::Success.code();
    for target in &targets {
        for agent in &agents {
            let opts = BuildOptions {
                catalog: args.catalog.clone(),
                config: args.config.clone(),
                out: args.out.clone(),
                target: target.clone(),
                agent: agent.clone(),
                locked: args.locked,
                frozen: args.frozen,
            };
            let code = match skillc_core::run_build(&opts) {
                Ok(report) => {
                    print_once(&report.diagnostics);
                    eprintln!("cite: built {}", report.artifact_dir.display());
                    ExitCode::Success.code()
                }
                Err(PipelineError::Usage(d)) => {
                    eprintln!("{d}");
                    ExitCode::Usage.code()
                }
                Err(PipelineError::Hard(diags)) => {
                    print_once(&diags);
                    ExitCode::HardFailure.code()
                }
            };
            worst = worst.max(code);
        }
    }
    if suppressed > 0 {
        eprintln!("cite: {suppressed} duplicate diagnostic lines across combos suppressed");
    }
    worst
}

/// Resolve one build dimension: `Ok(Some(value))` for an explicit flag, `Ok(None)` when
/// `--all-*` defers to the registry, `Err(exit_code)` on a usage error.
fn resolve_dimension(
    all: bool,
    explicit: &Option<String>,
    name: &str,
) -> Result<Option<String>, i32> {
    match (all, explicit) {
        (true, _) => Ok(None),
        (false, Some(v)) => Ok(Some(v.clone())),
        (false, None) => {
            eprintln!(
                "{}",
                Diagnostic::error(
                    Code::ConfigInvalid,
                    format!("--{name} is required (or pass --all-{name}s)")
                )
            );
            Err(ExitCode::Usage.code())
        }
    }
}

fn run_install_cmd(args: InstallArgs) -> i32 {
    let opts = InstallOptions {
        artifact: args.artifact,
        dest: args.dest,
        agent: args.agent,
    };
    match skillc_core::run_install(&opts) {
        Ok(()) => ExitCode::Success.code(),
        Err(PipelineError::Usage(d)) => {
            eprintln!("{d}");
            ExitCode::Usage.code()
        }
        Err(PipelineError::Hard(diags)) => {
            print_diagnostics(&diags);
            ExitCode::HardFailure.code()
        }
    }
}

/// Print errors then warnings to stderr (warnings never silent — FR-027).
fn print_diagnostics(diags: &Diagnostics) {
    for d in diags.iter() {
        match d.severity {
            Severity::Error | Severity::Warning => eprintln!("{d}"),
        }
    }
}
