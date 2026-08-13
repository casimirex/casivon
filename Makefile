# Casivon — one entry point for the things you do every day.
#
# `make` on its own lists the targets. Everything here is a thin wrapper over
# cargo, npm and docker compose: nothing is hidden, and every recipe is a command
# you could have typed. Run `make -n <target>` to see exactly what it would do.

BACKEND  := backend
FRONTEND := frontend

# The services worth having up for local development. `mailpit` catches password
# reset mail and `minio` stores uploaded receipts; both are optional, and the
# backend says so rather than failing when they are absent.
INFRA    := postgres redis
EXTRAS   := mailpit minio

# Where `make db-backup` writes, kept outside the repo so a dump can never be
# committed by accident.
BACKUP_DIR := $(HOME)/casivon-backups
STAMP      := $(shell date +%Y%m%d-%H%M%S)

# Read from the compose file's own defaults so this cannot drift from it.
PG_USER := erp
PG_DB   := erp_db
PG      := casivon-postgres

.DEFAULT_GOAL := help
.PHONY: help setup up up-all down restart logs ps clean \
        backend backend-check backend-test frontend frontend-install frontend-test \
        dev test lint build gate generate db-shell db-backup db-restore db-reset

## —— Getting started ————————————————————————————————————————————————

help: ## List every target
	@grep -hE '^[a-zA-Z_-]+:.*?## .*$$|^## ' $(MAKEFILE_LIST) \
	  | sed -E 's/^## (.*)/\n\1/' \
	  | awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:/ {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2} !/^[a-zA-Z_-]+:/ {print $$0}'
	@echo ""

setup: ## First run: copy the env template and install frontend deps
	@test -f $(BACKEND)/.env || (cp $(BACKEND)/.env.example $(BACKEND)/.env && echo "created $(BACKEND)/.env from the template")
	@$(MAKE) --no-print-directory frontend-install
	@echo "Now: make up && make backend   (and 'make frontend' in another terminal)"

## —— Docker ——————————————————————————————————————————————————————————

up: ## Start Postgres and Redis
	docker compose up -d $(INFRA)

up-all: ## Start Postgres, Redis, Mailpit and MinIO
	docker compose up -d $(INFRA) $(EXTRAS)
	@echo "Mailpit http://localhost:8025   MinIO console http://localhost:9001"

down: ## Stop the stack, keeping the data volumes
	docker compose down

restart: down up ## Stop and start again

ps: ## What is running
	@docker compose ps

logs: ## Follow the stack's logs (make logs SERVICE=postgres for one)
	docker compose logs -f $(SERVICE)

clean: ## Stop the stack AND delete its data volumes — destroys the database
	@printf 'This deletes casivon_postgres_data and casivon_minio_data. Type yes to confirm: ' \
	  && read answer && [ "$$answer" = "yes" ] || (echo "cancelled"; exit 1)
	docker compose down -v

## —— Running it ——————————————————————————————————————————————————————

backend: ## Run the API on :8080 (applies migrations on boot)
	cd $(BACKEND) && cargo run

frontend: ## Run the web app on :3000
	cd $(FRONTEND) && npm run dev

frontend-install: ## Install frontend dependencies
	cd $(FRONTEND) && npm install

dev: up ## Start the stack, then the API — run `make frontend` alongside it
	@$(MAKE) --no-print-directory backend

## —— Checking it —————————————————————————————————————————————————————

backend-check: ## Type-check the backend without running anything
	cd $(BACKEND) && cargo check --all-targets

backend-test: ## Backend unit and integration tests (needs `make up`)
	cd $(BACKEND) && cargo test

frontend-test: ## Frontend tests
	cd $(FRONTEND) && npm test

test: backend-test frontend-test ## Every test, both sides

lint: ## Type-check and lint the frontend
	cd $(FRONTEND) && npx tsc --noEmit && npm run lint

build: ## Production build of both sides
	cd $(BACKEND) && cargo build --release
	cd $(FRONTEND) && npm run build

gate: backend-test frontend-test lint build ## Everything CI would run

generate: ## Regenerate openapi.json and the frontend's types from the handlers
	cd $(FRONTEND) && npm run generate:spec && npm run generate:types

## —— The database ————————————————————————————————————————————————————

db-shell: ## psql into the running database
	docker exec -it $(PG) psql -U $(PG_USER) -d $(PG_DB)

db-backup: ## Dump the database to ~/casivon-backups
	@mkdir -p $(BACKUP_DIR)
	docker exec $(PG) pg_dumpall -U $(PG_USER) > $(BACKUP_DIR)/casivon-$(STAMP).sql
	@gzip -k $(BACKUP_DIR)/casivon-$(STAMP).sql
	@ls -lh $(BACKUP_DIR)/casivon-$(STAMP).sql* | awk '{print "  wrote", $$NF, "("$$5")"}'

db-restore: ## Load a dump back in: make db-restore FILE=~/casivon-backups/x.sql
	@test -n "$(FILE)" || (echo "FILE= is required, e.g. make db-restore FILE=$(BACKUP_DIR)/casivon-....sql"; exit 1)
	@test -f "$(FILE)" || (echo "no such file: $(FILE)"; exit 1)
	@printf 'This loads %s over the current database. Type yes to confirm: ' "$(FILE)" \
	  && read answer && [ "$$answer" = "yes" ] || (echo "cancelled"; exit 1)
	docker exec -i $(PG) psql -U $(PG_USER) -d postgres -q < "$(FILE)"
	@echo "restored"

db-reset: ## Drop and recreate the database, then let the backend migrate it
	@printf 'This destroys every row in $(PG_DB). Type yes to confirm: ' \
	  && read answer && [ "$$answer" = "yes" ] || (echo "cancelled"; exit 1)
	docker exec $(PG) psql -U $(PG_USER) -d postgres -c "DROP DATABASE IF EXISTS $(PG_DB);"
	docker exec $(PG) psql -U $(PG_USER) -d postgres -c "CREATE DATABASE $(PG_DB);"
	@echo 'empty. "make backend" will apply the migrations.'
