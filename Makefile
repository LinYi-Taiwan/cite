# cite — build / install / serve helpers.
#
# Why this exists: `cite` on your PATH (~/.cargo/bin/cite) is a SNAPSHOT copied by
# `cargo install` — it does NOT track edits to this source tree. After changing any
# .rs or assets/inspector.html you must either reinstall (`make install`) or run from
# source (`make dev`), or `cite serve` keeps serving the old build.

CRATE      := crates/skill-inspector
BIN        := cite
DEV_HTML   := $(CURDIR)/$(CRATE)/assets/inspector.html
# Project to inspect; override on the CLI, e.g. `make dev PROJECT=/path/to/other/repo`.
PROJECT    ?= $(CURDIR)
# Optional fixed port (default: cite picks one). e.g. `make serve PORT=8787`.
PORT       ?=
PORT_ARG   := $(if $(PORT),--port $(PORT),)

.DEFAULT_GOAL := dev

# --- iterate: ALWAYS latest, no install -------------------------------------------------
# `cargo run` recompiles changed Rust on each invocation; CITE_DEV_HTML makes the server
# read inspector.html from disk per request, so an HTML edit shows on browser refresh
# without even restarting. This is the command to use while developing.
.PHONY: dev
dev:
	CITE_DEV_HTML="$(DEV_HTML)" cargo run -p skill-inspector -- serve --project "$(PROJECT)" $(PORT_ARG)

# One-shot read-only scan (JSON to stdout) from current source. Recipe is silenced (@) so
# stdout is pure JSON, safe to pipe (e.g. `make scan | jq`).
.PHONY: scan
scan:
	@cargo run -q -p skill-inspector -- scan --project "$(PROJECT)" --format json

# --- update the global binary -----------------------------------------------------------
# Recompiles from THIS tree and overwrites ~/.cargo/bin/$(BIN). After this, the plain
# `cite` command everywhere is the latest (HTML is embedded at compile time, so it's
# fresh too — no CITE_DEV_HTML needed).
.PHONY: install
install:
	cargo install --path $(CRATE) --force --locked

# Reinstall, then serve via the freshly-installed global binary.
.PHONY: serve
serve: install
	$(BIN) serve --project "$(PROJECT)" $(PORT_ARG)

# --- plain build / test -----------------------------------------------------------------
.PHONY: build
build:
	cargo build --release -p skill-inspector

.PHONY: test
test:
	cargo test -p skill-inspector

.PHONY: help
help:
	@echo "make dev      - run from source, always latest (Rust recompiled, HTML from disk). DEFAULT."
	@echo "make install  - rebuild and overwrite the global ~/.cargo/bin/$(BIN)."
	@echo "make serve    - install, then serve via the global $(BIN)."
	@echo "make scan     - one-shot JSON inventory from current source."
	@echo "make build    - cargo build --release."
	@echo "make test     - run the test suite."
	@echo "vars: PROJECT=<dir> (default cwd), PORT=<n> (default auto)."
