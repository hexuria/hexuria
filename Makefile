# PayPlan Platform v2 — developer Makefile
#
# Common developer shortcuts.

SHELL := /bin/bash
DATABASE_URL ?= postgres://$(shell whoami)@localhost:5432/postgres?host=/tmp
BIND_ADDR ?= 0.0.0.0:3000
RUST_LOG ?= info,payplan=debug,sqlx=warn
JWT_SECRET ?= dev-secret-change-me-dev-secret-change-me

.PHONY: help
help: ## Show this help
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "\033[36m%-22s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

.PHONY: fmt
fmt: ## cargo fmt --all
	cargo fmt --all

.PHONY: clippy
clippy: ## clippy with -D warnings
	cargo clippy --workspace --all-targets -- -D warnings

.PHONY: test
test: ## Run workspace tests sequentially
	DATABASE_URL="$(DATABASE_URL)" cargo test --workspace -- --test-threads=1

.PHONY: test-integration
test-integration: ## Run Postgres integration tests
	DATABASE_URL="$(DATABASE_URL)" \
		cargo test -p payplan_infra --features integration -- \
			--include-ignored --test-threads=1

.PHONY: test-ui
test-ui: ## Compile and run Leptos app/client/server tests
	DATABASE_URL="$(DATABASE_URL)" JWT_SECRET="$(JWT_SECRET)" \
		cargo leptos test

.PHONY: check
check: ## cargo check --workspace --all-targets
	cargo check --workspace --all-targets

.PHONY: build
build: ## release build
	cargo build --workspace --release

.PHONY: ui-build
ui-build: ## Build optimized SSR + islands assets
	cargo leptos build --release --precompress

.PHONY: ui-tree
ui-tree: ## Verify hydrate-only dependency boundary
	cargo tree -p payplan_ui --target wasm32-unknown-unknown --no-default-features --features hydrate

.PHONY: ui-baseline
ui-baseline: ui-build ## Print deterministic release asset baseline
	bash scripts/ui-asset-baseline.sh

.PHONY: ui-route-baseline
ui-route-baseline: ## Measure authenticated dashboard/packages HTML timings (requires COOKIE_FILE)
	bash scripts/ui-route-baseline.sh

.PHONY: seed
seed: ## Apply dev seed (drops Acme data, re-creates fixtures)
	psql "$(DATABASE_URL)" -f seeds/dev.sql

.PHONY: reset
reset: ## Wipe ALL data in the configured database (DANGER)
	psql "$(DATABASE_URL)" -c "TRUNCATE TABLE reward_ledger, event_log, entitlements, enrollments, purchases, subscriptions, package_items, packages, pay_plan_stack_modules, pay_plan_stacks, billing_plans, catalog_items, users, companies RESTART IDENTITY CASCADE"

.PHONY: serve
serve: ## Build browser assets, run the server, and watch for changes
	DATABASE_URL="$(DATABASE_URL)" JWT_SECRET="$(JWT_SECRET)" \
		BIND_ADDR="$(BIND_ADDR)" RUST_LOG="$(RUST_LOG)" \
		cargo leptos watch

.PHONY: serve-release
serve-release: ## Run an optimized local server with precompressed assets
	DATABASE_URL="$(DATABASE_URL)" JWT_SECRET="$(JWT_SECRET)" \
		BIND_ADDR="$(BIND_ADDR)" RUST_LOG="$(RUST_LOG)" \
		cargo leptos serve --release --precompress

.PHONY: ci
ci: fmt clippy test test-integration test-ui ui-build ui-tree ## Run the full CI suite locally

.PHONY: health
health: ## curl /health on the running server
	curl -s http://127.0.0.1:3000/health | jq .
