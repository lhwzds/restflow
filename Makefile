.PHONY: dev prod build down logs clean help

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

# Clean up volumes and images
clean:
	docker compose -f docker-compose.dev.yml down -v 2>/dev/null || true
	docker compose down -v 2>/dev/null || true
	docker rmi restflow-restflow 2>/dev/null || true

# Run backend locally (no docker)
run:
	cargo run --bin restflow-server

# Run frontend locally (no docker)
web:
	cd web && npm run dev

# Run both locally in background
local:
	@echo "Starting backend..."
	@cargo run --bin restflow-server &
	@echo "Starting frontend..."
	@cd web && npm run dev

help:
	@echo "Usage:"
	@echo "  make dev    - Start dev mode (hot reload)"
	@echo "  make prod   - Start production mode"
	@echo "  make down   - Stop all containers"
	@echo "  make logs   - View container logs"
	@echo "  make clean  - Remove containers and volumes"
	@echo "  make run    - Run backend locally"
	@echo "  make web    - Run frontend locally"
