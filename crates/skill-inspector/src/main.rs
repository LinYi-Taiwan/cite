//! `skill-inspector` binary: clap dispatch over `scan` (pure-read) and `serve` (interactive).
//! The same subcommands are also reachable as `cite inspect …` (both call `skill_inspector::run`).

use clap::Parser;

use skill_inspector::cli::Cli;

fn main() {
    std::process::exit(skill_inspector::run(Cli::parse().command));
}
