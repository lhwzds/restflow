.PHONY: dev prod build down logs clean help run web local install cli release release-check

CLI_RELEASE_CRATES := restflow-storage restflow-core restflow-ai restflow-cli
RELEASE_TARGET_DIR ?= $(CURDIR)/target-release-check
CARGO_BUILD_JOBS ?= 8
RELEASE_FD_CLOSE := exec 3<&- 4<&- 5<&- 6<&- 7<&- 8<&- 9<&-

define RUN_RELEASE_GATES
mkdir -p web/dist; \
command -v cargo-audit >/dev/null 2>&1 || env -u MAKEFLAGS -u MFLAGS -u CARGO_MAKEFLAGS cargo install cargo-audit --locked; \
env -u MAKEFLAGS -u MFLAGS -u CARGO_MAKEFLAGS cargo audit; \
for crate in $(CLI_RELEASE_CRATES); do \
	echo "==> cargo clippy --package $$crate --all-targets -- -D warnings"; \
	env -u MAKEFLAGS -u MFLAGS -u CARGO_MAKEFLAGS CARGO_TARGET_DIR="$(RELEASE_TARGET_DIR)" CARGO_INCREMENTAL=0 cargo clippy -j "$(CARGO_BUILD_JOBS)" --package "$$crate" --all-targets -- -D warnings || exit $$?; \
done; \
for crate in $(CLI_RELEASE_CRATES); do \
	echo "==> cargo test --package $$crate --verbose"; \
	env -u MAKEFLAGS -u MFLAGS -u CARGO_MAKEFLAGS CARGO_TARGET_DIR="$(RELEASE_TARGET_DIR)" CARGO_INCREMENTAL=0 cargo test -j "$(CARGO_BUILD_JOBS)" --package "$$crate" --verbose || exit $$?; \
done
endef

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
	@echo "    make cli     - Build CLI in release mode"
	@echo "    make release - Run local release gates and build CLI release binary"
	@echo "    make install - Install CLI (restflow & rf) to ~/.local/bin"

# Build CLI
cli:
	cargo build --release --package restflow-cli

release-check:
	@set -e; \
	$(RELEASE_FD_CLOSE); \
	trap 'rm -rf "$(RELEASE_TARGET_DIR)"' EXIT; \
	$(RUN_RELEASE_GATES)

release:
	@set -e; \
	$(RELEASE_FD_CLOSE); \
	trap 'rm -rf "$(RELEASE_TARGET_DIR)"' EXIT; \
	$(RUN_RELEASE_GATES); \
	env -u MAKEFLAGS -u MFLAGS -u CARGO_MAKEFLAGS CARGO_TARGET_DIR="$(RELEASE_TARGET_DIR)" CARGO_INCREMENTAL=0 cargo build -j "$(CARGO_BUILD_JOBS)" --release --package restflow-cli

# Install CLI with rf alias
install: cli
	@mkdir -p $(HOME)/.local/bin
	@cp target/release/restflow $(HOME)/.local/bin/restflow
	@codesign --force --sign - $(HOME)/.local/bin/restflow 2>/dev/null || true
	@ln -sf $(HOME)/.local/bin/restflow $(HOME)/.local/bin/rf
	@echo "Installed: ~/.local/bin/restflow"
	@echo "Installed: ~/.local/bin/rf -> restflow"
