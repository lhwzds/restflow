.PHONY: help run install cli release release-check fmt test lint audit toolchain

RUST_TOOLCHAIN ?= stable
CARGO_TARGET_DIR ?= $(HOME)/.cargo-targets/restflow
export CARGO_TARGET_DIR
RESTFLOW_RELEASE_BIN := $(CARGO_TARGET_DIR)/release/restflow

# Run daemon locally
run:
	cargo run --bin restflow -- daemon start --foreground

help:
	@echo "Usage:"
	@echo ""
	@echo "  Local:"
	@echo "    make run    - Run daemon locally"
	@echo "  CLI:"
	@echo "    make fmt     - Format Rust code"
	@echo "    make test    - Run Rust tests"
	@echo "    make lint    - Run Rust fmt and clippy checks"
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
test:
	@set -e; \
	TYPEGEN_DIR="$$(mktemp -d)"; \
	trap 'rm -rf "$$TYPEGEN_DIR"' EXIT; \
	TS_RS_EXPORT_DIR="$$TYPEGEN_DIR" cargo test

# Run Rust lint checks
lint: toolchain
	cargo +$(RUST_TOOLCHAIN) fmt --all --check
	cargo +$(RUST_TOOLCHAIN) clippy --all-targets -- -D warnings

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
