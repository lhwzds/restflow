.PHONY: help run install cli release release-check fmt test rust-test tui-unit-test lint audit toolchain tui-pty-smoke

RUST_TOOLCHAIN ?= stable
CARGO_TARGET_DIR ?= $(HOME)/.cargo-targets/restflow
export CARGO_TARGET_DIR
RESTFLOW_RELEASE_BIN := $(CARGO_TARGET_DIR)/release/restflow

# Run daemon locally
run:
	cargo run --package cli --bin restflow -- daemon start --foreground

help:
	@echo "Usage:"
	@echo ""
	@echo "  Local:"
	@echo "    make run    - Run daemon locally"
	@echo "  CLI:"
	@echo "    make fmt     - Format Rust code"
	@echo "    make test    - Run Rust tests"
	@echo "    make lint    - Run Rust fmt and clippy checks"
	@echo "    make tui-pty-smoke - Run a real PTY smoke for the TUI"
	@echo "    make audit   - Run cargo security audit"
	@echo "    make cli     - Build CLI in release mode"
	@echo "    make release - Run make lint, make audit, make test, and make cli"
	@echo "    make install - Install CLI (restflow & rf) to ~/.local/bin"

# Match CI's latest stable Rust toolchain for release checks
toolchain:
	rustup toolchain install $(RUST_TOOLCHAIN) --component clippy --component rustfmt

# Format Rust code
fmt:
	cargo fmt --all

# Run Rust tests
rust-test:
	@set -e; \
	TYPEGEN_DIR="$$(mktemp -d)"; \
	trap 'rm -rf "$$TYPEGEN_DIR"' EXIT; \
		TS_RS_EXPORT_DIR="$$TYPEGEN_DIR" cargo test --workspace

tui-unit-test:
	cargo test --package tui --lib

test:
	$(MAKE) rust-test
	$(MAKE) tui-pty-smoke
	$(MAKE) tui-unit-test

# Run Rust lint checks
lint: toolchain
	cargo +$(RUST_TOOLCHAIN) fmt --all --check
	cargo +$(RUST_TOOLCHAIN) clippy --workspace --all-targets -- -D warnings

tui-pty-smoke:
	@if [ "$$(uname -s 2>/dev/null || echo Windows)" = "Windows" ]; then \
		echo "Skipping TUI PTY smoke on Windows: pseudo-terminal APIs are Unix-only."; \
	elif [ ! -f scripts/tui_pty_smoke.py ]; then \
		echo "Skipping TUI PTY smoke: scripts/tui_pty_smoke.py not found."; \
	else \
		python3 scripts/tui_pty_smoke.py; \
	fi

# Run Rust security audit
audit: toolchain
	@command -v cargo-audit >/dev/null 2>&1 || cargo +$(RUST_TOOLCHAIN) install cargo-audit --locked
	cargo +$(RUST_TOOLCHAIN) audit

# Build CLI
cli:
	cargo build --release --package cli

release-check:
	$(MAKE) lint
	$(MAKE) audit
	$(MAKE) test

release:
	$(MAKE) lint
	$(MAKE) audit
	$(MAKE) test
	$(MAKE) cli

# Install CLI with rf alias
install: cli
	@mkdir -p $(HOME)/.local/bin
	@cp "$(RESTFLOW_RELEASE_BIN)" $(HOME)/.local/bin/restflow
	@codesign --force --sign - $(HOME)/.local/bin/restflow 2>/dev/null || true
	@ln -sf $(HOME)/.local/bin/restflow $(HOME)/.local/bin/rf
	@echo "Installed: ~/.local/bin/restflow"
	@echo "Installed: ~/.local/bin/rf -> restflow"
