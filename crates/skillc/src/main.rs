//! `skillc` CLI entrypoint. Phase 1 stub; real dispatch lands in Phase 2 (T011).
mod cli;

fn main() {
    std::process::exit(cli::run());
}
