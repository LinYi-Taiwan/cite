//! skillc-core: the skill-compiler pipeline.
//!
//! Stages: `parse → resolve → validate → shake → assemble → emit → install`.
//! Each stage threads a [`Diagnostics`] accumulator; a hard error short-circuits the
//! build (non-zero exit) while warnings are surfaced and the build proceeds.
//!
//! Later stages are filled in per user story; in this foundational cut (T010) the
//! pipeline runs `parse` for real and treats the rest as pass-through, so the CLI can
//! drive an end-to-end (if empty) build.

pub mod agent;
pub mod assemble;
pub mod config;
pub mod diagnostics;
pub mod emit;
pub mod graph;
pub mod graph_export;
pub mod install;
pub mod model;
pub mod parse;
pub mod query;
pub mod resolve;
pub mod schema;
pub mod shake;
pub mod validate;

use std::path::PathBuf;

use assemble::Bundle;
use config::FrameworkConfig;
use diagnostics::{Code, Diagnostic, Diagnostics, ExitCode};
use model::Catalog;

/// Options for `skillc build` (one `(target, agent)` combination).
#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub catalog: PathBuf,
    pub config: Option<PathBuf>,
    pub out: PathBuf,
    pub target: String,
    pub agent: String,
    pub locked: bool,
    pub frozen: bool,
}

/// Options for `skillc install`.
#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub artifact: PathBuf,
    pub dest: PathBuf,
    pub agent: Option<String>,
}

/// Options for `skillc check` (validate the catalog without writing an artifact).
#[derive(Debug, Clone)]
pub struct CheckOptions {
    pub catalog: PathBuf,
    pub config: Option<PathBuf>,
    /// Restrict to one target; `None` checks every registered target.
    pub target: Option<String>,
    pub locked: bool,
    pub frozen: bool,
}

/// A successful build's report: the assembled bundle, where the artifact landed, and any
/// non-fatal warnings.
#[derive(Debug, Clone)]
pub struct BuildReport {
    pub artifact_dir: PathBuf,
    pub bundle: Bundle,
    pub diagnostics: Diagnostics,
}

/// A build that did not produce an artifact.
#[derive(Debug, Clone)]
pub enum PipelineError {
    /// Usage-level problem (unknown target/agent, missing config) → exit 2.
    Usage(Diagnostic),
    /// One or more hard failures during compilation → exit 1. Carries every diagnostic.
    Hard(Diagnostics),
}

impl PipelineError {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::Usage(_) => ExitCode::Usage,
            Self::Hard(_) => ExitCode::HardFailure,
        }
    }
}

/// Compile one `(target, agent)` into an inspectable artifact under `opts.out`.
pub fn run_build(opts: &BuildOptions) -> Result<BuildReport, PipelineError> {
    // --- usage validation: config + target/agent registry (exit 2) ---
    let cfg = FrameworkConfig::discover(&opts.catalog, opts.config.as_deref())
        .map_err(PipelineError::Usage)?;

    if !cfg.is_target(&opts.target) {
        return Err(PipelineError::Usage(Diagnostic::error(
            Code::ConfigInvalid,
            format!(
                "unknown target `{}` (registered targets: {})",
                opts.target,
                cfg.target_names()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )));
    }
    if !cfg.is_agent(&opts.agent) {
        return Err(PipelineError::Usage(Diagnostic::error(
            Code::ConfigInvalid,
            format!(
                "unknown agent `{}` (registered agents: {})",
                opts.agent,
                cfg.agents().join(", ")
            ),
        )));
    }

    let mut diags = Diagnostics::new();

    // --- parse (real) ---
    let catalog: Catalog = parse::parse_catalog(&opts.catalog, &mut diags);
    if diags.has_errors() {
        return Err(PipelineError::Hard(diags));
    }

    // --- graph: @include missing/cycle/unused (US1) ---
    graph::analyze_includes(&catalog, &mut diags);
    if diags.has_errors() {
        return Err(PipelineError::Hard(diags));
    }

    // --- resolve: markers → imports, cross-repo pull, dedup, version conflict (US2) ---
    let lock = resolve::lock::Lockfile::read(&opts.catalog).map_err(PipelineError::Usage)?;
    let resolution = resolve::resolve(
        &catalog,
        &opts.catalog,
        &lock,
        opts.locked,
        opts.frozen,
        &mut diags,
    );
    if diags.has_errors() {
        return Err(PipelineError::Hard(diags));
    }

    // --- validate: schema, stray refs, collisions (US4) ---
    validate::validate(&catalog, &cfg, &mut diags);
    if diags.has_errors() {
        return Err(PipelineError::Hard(diags));
    }

    // --- shake: per-target membership by entry-point pull (US3) ---
    let membership = shake::shake(&catalog, &cfg, &resolution, &opts.target, &mut diags);
    // --- per-target required-reference check (US4, membership-dependent) ---
    validate::check_per_target_references(&catalog, &membership, &opts.target, &mut diags);
    if diags.has_errors() {
        return Err(PipelineError::Hard(diags));
    }

    // --- assemble: inline @includes, render markers, land deps, prune refs (US1/US2/US3) ---
    let bundle = assemble::assemble(&catalog, &cfg, &resolution, &membership, &opts.target);

    // --- emit: deterministic artifact writer (US5) ---
    let artifact_dir = opts.out.join(&opts.target).join(&opts.agent);
    let warnings: Vec<String> = diags
        .warnings()
        .map(|d| format!("{}: {}", d.code.as_str(), d.message))
        .collect();
    if let Err(d) = emit::emit(&bundle, &opts.agent, &warnings, &artifact_dir) {
        diags.push(d);
        return Err(PipelineError::Hard(diags));
    }

    Ok(BuildReport {
        artifact_dir,
        bundle,
        diagnostics: diags,
    })
}

/// Validate the catalog without writing any artifact (the CI / pre-commit gate): runs
/// `parse → graph → resolve → validate` once, then the per-target `shake` + reference
/// checks for every target in scope. Returns the (deduped) diagnostics on success so
/// callers can print warnings; hard failures carry every diagnostic.
pub fn run_check(opts: &CheckOptions) -> Result<Diagnostics, PipelineError> {
    let cfg = FrameworkConfig::discover(&opts.catalog, opts.config.as_deref())
        .map_err(PipelineError::Usage)?;

    let targets: Vec<String> = match &opts.target {
        Some(t) => {
            if !cfg.is_target(t) {
                return Err(PipelineError::Usage(Diagnostic::error(
                    Code::ConfigInvalid,
                    format!(
                        "unknown target `{t}` (registered targets: {})",
                        cfg.target_names()
                            .into_iter()
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                )));
            }
            vec![t.clone()]
        }
        None => cfg.target_names().into_iter().collect(),
    };

    let mut diags = Diagnostics::new();

    // Same stage gating as `run_build`: a stage's errors stop the pipeline so later
    // stages don't pile cascading follow-on errors (resolve over a cyclic/unparsable
    // catalog) on top of the root cause.
    let catalog: Catalog = parse::parse_catalog(&opts.catalog, &mut diags);
    graph::analyze_includes(&catalog, &mut diags);
    if diags.has_errors() {
        diags.dedup();
        return Err(PipelineError::Hard(diags));
    }

    let lock = resolve::lock::Lockfile::read(&opts.catalog).map_err(PipelineError::Usage)?;
    let resolution = resolve::resolve(
        &catalog,
        &opts.catalog,
        &lock,
        opts.locked,
        opts.frozen,
        &mut diags,
    );
    if diags.has_errors() {
        diags.dedup();
        return Err(PipelineError::Hard(diags));
    }

    validate::validate(&catalog, &cfg, &mut diags);

    // Per-target membership + required-reference checks. Catalog-wide analyses repeated
    // inside shake (orphans) produce identical lines per target; dedup below.
    if !diags.has_errors() {
        for target in &targets {
            let membership = shake::shake(&catalog, &cfg, &resolution, target, &mut diags);
            validate::check_per_target_references(&catalog, &membership, target, &mut diags);
        }
    }

    diags.dedup();
    if diags.has_errors() {
        return Err(PipelineError::Hard(diags));
    }
    Ok(diags)
}

/// Options for `skillc why <id>`.
#[derive(Debug, Clone)]
pub struct WhyOptions {
    pub catalog: PathBuf,
    pub config: Option<PathBuf>,
    pub id: String,
}

/// Explain a unit's reverse dependencies (`skillc why <id>`): who includes/imports it and
/// which targets ship it. Read-only; reuses the compiler's own resolution, so the answer
/// matches what `build` would do.
pub fn run_why(opts: &WhyOptions) -> Result<String, PipelineError> {
    let cfg = FrameworkConfig::discover(&opts.catalog, opts.config.as_deref())
        .map_err(PipelineError::Usage)?;

    let mut diags = Diagnostics::new();
    let catalog: Catalog = parse::parse_catalog(&opts.catalog, &mut diags);
    graph::analyze_includes(&catalog, &mut diags);
    let lock = resolve::lock::Lockfile::read(&opts.catalog).map_err(PipelineError::Usage)?;
    let resolution = resolve::resolve(&catalog, &opts.catalog, &lock, false, false, &mut diags);
    if diags.has_errors() {
        return Err(PipelineError::Hard(diags));
    }

    match query::why(&catalog, &cfg, &resolution, &opts.id) {
        Some(report) => Ok(report.text),
        None => Err(PipelineError::Usage(Diagnostic::error(
            Code::ConfigInvalid,
            format!(
                "`{}` is not a skill or block in this catalog\n{}",
                opts.id,
                query::inventory(&catalog)
            ),
        ))),
    }
}

/// Options for `skillc graph`.
#[derive(Debug, Clone)]
pub struct GraphOptions {
    pub catalog: PathBuf,
    pub config: Option<PathBuf>,
    pub format: graph_export::GraphFormat,
    /// Restrict to one target's bundle closure.
    pub target: Option<String>,
    /// Restrict to the neighborhood of one unit.
    pub focus: Option<String>,
    /// Hop radius for `focus` (ignored without it).
    pub depth: usize,
}

/// Export the catalog's dependency graph (`skillc graph`): every target / skill / block /
/// reference and the mounts / imports / includes / hosts edges between them. Read-only;
/// reuses the compiler's own resolution, so the picture matches what `build` would do.
pub fn run_graph(opts: &GraphOptions) -> Result<String, PipelineError> {
    let cfg = FrameworkConfig::discover(&opts.catalog, opts.config.as_deref())
        .map_err(PipelineError::Usage)?;

    if let Some(t) = &opts.target {
        if !cfg.is_target(t) {
            return Err(PipelineError::Usage(Diagnostic::error(
                Code::ConfigInvalid,
                format!(
                    "unknown target `{t}` (registered targets: {})",
                    cfg.target_names()
                        .into_iter()
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )));
        }
    }

    let mut diags = Diagnostics::new();
    let catalog: Catalog = parse::parse_catalog(&opts.catalog, &mut diags);
    graph::analyze_includes(&catalog, &mut diags);
    let lock = resolve::lock::Lockfile::read(&opts.catalog).map_err(PipelineError::Usage)?;
    let resolution = resolve::resolve(&catalog, &opts.catalog, &lock, false, false, &mut diags);
    if diags.has_errors() {
        return Err(PipelineError::Hard(diags));
    }

    let full = graph_export::export(&catalog, &cfg, &resolution, &opts.catalog);

    if let Some(focus) = &opts.focus {
        if !full.nodes.iter().any(|n| &n.id == focus) {
            return Err(PipelineError::Usage(Diagnostic::error(
                Code::ConfigInvalid,
                format!(
                    "`{focus}` is not a unit in this catalog\n{}",
                    query::inventory(&catalog)
                ),
            )));
        }
    }

    let filtered = graph_export::filter(
        full,
        &graph_export::GraphFilter {
            target: opts.target.clone(),
            focus: opts.focus.clone(),
            depth: opts.depth,
        },
    );
    Ok(graph_export::render(&filtered, opts.format))
}

/// Place a previously-built artifact into an agent destination (separate step, FR-023).
pub fn run_install(opts: &InstallOptions) -> Result<(), PipelineError> {
    install::install(&opts.artifact, &opts.dest, opts.agent.as_deref())
        .map_err(PipelineError::Usage)
}
