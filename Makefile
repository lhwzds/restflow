.PHONY: dev prod build down logs clean help run web local install cli release release-check fmt test lint audit types stress toolchain

RUST_TOOLCHAIN ?= stable

# Development mode with hot reload
dev:
	docker compose -f docker-compose.dev.yml up

# Production mode
prod:
	docker compose up -d --build

# Build production image only
build:
	docker compose build

# Stop all containers
down:
	docker compose -f docker-compose.dev.yml down 2>/dev/null || true
	docker compose down 2>/dev/null || true

# View logs
logs:
	docker compose logs -f

# Clean up volumes and images (includes down)
clean: down
	docker volume rm restflow_cargo-cache restflow_target-cache restflow_node-modules 2>/dev/null || true
	docker rmi restflow-backend restflow-restflow 2>/dev/null || true

# Run daemon locally (no docker)
run:
	cargo run --bin restflow -- daemon start --foreground

# Run frontend locally (no docker)
web:
	cd web && npm run dev

# Run both locally in background
local:
	@echo "Starting daemon..."
	@cargo run --bin restflow -- daemon start --foreground &
	@echo "Starting frontend..."
	@cd web && npm run dev

help:
	@echo "Usage:"
	@echo ""
	@echo "  Docker:"
	@echo "    make dev    - Start dev mode with docker (hot reload)"
	@echo "    make prod   - Start production mode with docker"
	@echo "    make down   - Stop all containers"
	@echo "    make logs   - View container logs"
	@echo "    make clean  - Remove containers and volumes"
	@echo ""
	@echo "  Local (no docker):"
	@echo "    make run    - Run backend locally"
	@echo "    make web    - Run frontend locally"
	@echo "    make local  - Run both backend and frontend locally"
	@echo "  CLI:"
	@echo "    make fmt     - Format Rust and web code"
	@echo "    make test    - Run backend and frontend tests"
	@echo "    make stress  - Run smoke, stress, and soak stress tests sequentially"
	@echo "    make lint    - Run backend clippy and frontend lint checks"
	@echo "    make audit   - Run cargo security audit"
	@echo "    make types   - Regenerate web TypeScript bindings"
	@echo "    make cli     - Build CLI in release mode"
	@echo "    make release - Run make lint, make audit, make test, and make cli"
	@echo "    make install - Install CLI (restflow & rf) to ~/.local/bin"

# Match CI's latest stable Rust toolchain for release checks
toolchain:
	rustup toolchain install $(RUST_TOOLCHAIN) --component clippy --component rustfmt

# Format Rust and web code
fmt:
	cargo fmt --all
	cd web && npm run format

# Run backend and frontend tests
test:
	@set -e; \
	TYPEGEN_DIR="$$(mktemp -d)"; \
	trap 'rm -rf "$$TYPEGEN_DIR"' EXIT; \
	TS_RS_EXPORT_DIR="$$TYPEGEN_DIR" cargo test
	cd web && npm run test

# Run smoke, stress, and soak stress tests sequentially
stress:
	@set -e; \
	TYPEGEN_DIR="$$(mktemp -d)"; \
	trap 'rm -rf "$$TYPEGEN_DIR"' EXIT; \
	RESTFLOW_STRESS_LEVEL=smoke TS_RS_EXPORT_DIR="$$TYPEGEN_DIR" cargo test -p restflow-core --features test-utils --test stress_mock_runtime -- --nocapture --test-threads=1; \
	RESTFLOW_STRESS_LEVEL=smoke TS_RS_EXPORT_DIR="$$TYPEGEN_DIR" cargo test -p restflow-core --features test-utils --test stress_chat_profiles -- --nocapture --test-threads=1; \
	RESTFLOW_STRESS_LEVEL=smoke TS_RS_EXPORT_DIR="$$TYPEGEN_DIR" cargo test -p restflow-core --features test-utils --test stress_background_profiles -- --nocapture --test-threads=1; \
	RESTFLOW_STRESS_LEVEL=smoke TS_RS_EXPORT_DIR="$$TYPEGEN_DIR" cargo test -p restflow-core --features test-utils --test stress_mixed_workloads -- --nocapture --test-threads=1; \
	RESTFLOW_STRESS_LEVEL=smoke TS_RS_EXPORT_DIR="$$TYPEGEN_DIR" cargo test -p restflow-core --features test-utils --test stress_ipc_sessions -- --nocapture --test-threads=1; \
	RESTFLOW_STRESS_LEVEL=stress TS_RS_EXPORT_DIR="$$TYPEGEN_DIR" cargo test -p restflow-core --features test-utils --test stress_mock_runtime -- --nocapture --test-threads=1; \
	RESTFLOW_STRESS_LEVEL=stress TS_RS_EXPORT_DIR="$$TYPEGEN_DIR" cargo test -p restflow-core --features test-utils --test stress_chat_profiles -- --nocapture --test-threads=1; \
	RESTFLOW_STRESS_LEVEL=stress TS_RS_EXPORT_DIR="$$TYPEGEN_DIR" cargo test -p restflow-core --features test-utils --test stress_background_profiles -- --nocapture --test-threads=1; \
	RESTFLOW_STRESS_LEVEL=stress TS_RS_EXPORT_DIR="$$TYPEGEN_DIR" cargo test -p restflow-core --features test-utils --test stress_mixed_workloads -- --nocapture --test-threads=1; \
	RESTFLOW_STRESS_LEVEL=stress TS_RS_EXPORT_DIR="$$TYPEGEN_DIR" cargo test -p restflow-core --features test-utils --test stress_ipc_sessions -- --nocapture --test-threads=1; \
	RESTFLOW_STRESS_LEVEL=soak TS_RS_EXPORT_DIR="$$TYPEGEN_DIR" cargo test -p restflow-core --features test-utils --test stress_mock_runtime -- --nocapture --test-threads=1; \
	RESTFLOW_STRESS_LEVEL=soak TS_RS_EXPORT_DIR="$$TYPEGEN_DIR" cargo test -p restflow-core --features test-utils --test stress_chat_profiles -- --nocapture --test-threads=1; \
	RESTFLOW_STRESS_LEVEL=soak TS_RS_EXPORT_DIR="$$TYPEGEN_DIR" cargo test -p restflow-core --features test-utils --test stress_background_profiles -- --nocapture --test-threads=1; \
	RESTFLOW_STRESS_LEVEL=soak TS_RS_EXPORT_DIR="$$TYPEGEN_DIR" cargo test -p restflow-core --features test-utils --test stress_mixed_workloads -- --nocapture --test-threads=1; \
	RESTFLOW_STRESS_LEVEL=soak TS_RS_EXPORT_DIR="$$TYPEGEN_DIR" cargo test -p restflow-core --features test-utils --test stress_ipc_sessions -- --nocapture --test-threads=1

# Run backend and frontend lint checks
lint: toolchain
	cargo +$(RUST_TOOLCHAIN) fmt --all --check
	cargo +$(RUST_TOOLCHAIN) clippy --all-targets -- -D warnings
	cd web && npm run format:check

# Run Rust security audit
audit: toolchain
	@command -v cargo-audit >/dev/null 2>&1 || cargo +$(RUST_TOOLCHAIN) install cargo-audit --locked
	cargo +$(RUST_TOOLCHAIN) audit

# Build CLI
cli:
	cargo build --release --package restflow-cli

# Regenerate web TypeScript bindings
types:
	./scripts/generate_web_types.sh

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
	@cp target/release/restflow $(HOME)/.local/bin/restflow
	@codesign --force --sign - $(HOME)/.local/bin/restflow 2>/dev/null || true
	@ln -sf $(HOME)/.local/bin/restflow $(HOME)/.local/bin/rf
	@echo "Installed: ~/.local/bin/restflow"
	@echo "Installed: ~/.local/bin/rf -> restflow"
