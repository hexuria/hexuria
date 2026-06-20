# PayPlan Platform v2 — developer Makefile
#
# Common shortcuts. All targets assume RTK is installed for token-efficient shell use.

SHELL := /bin/bash
DATABASE_URL ?= postgres://$(shell whoami)@localhost:5432/postgres?host=/tmp
BIND_ADDR ?= 0.0.0.0:3000
RUST_LOG ?= info,payplan=debug,sqlx=warn

.PHONY: help
help: ## Show this help
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "\033[36m%-22s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

.PHONY: fmt
fmt: ## cargo fmt --all
	rtk cargo fmt --all

.PHONY: clippy
clippy: ## clippy with -D warnings
	rtk cargo clippy --workspace --all-targets -- -D warnings

.PHONY: test
test: ## cargo test --workspace
	rtk cargo test --workspace

.PHONY: check
check: ## cargo check --workspace --all-targets
	rtk cargo check --workspace --all-targets

.PHONY: build
build: ## release build
	rtk cargo build --workspace --release

.PHONY: seed
seed: ## Apply dev seed (drops Acme data, re-creates fixtures)
	psql "$(DATABASE_URL)" -f seeds/dev.sql

.PHONY: reset
reset: ## Wipe ALL data in the configured database (DANGER)
	psql "$(DATABASE_URL)" -c "TRUNCATE TABLE reward_ledger, event_log, entitlements, enrollments, purchases, subscriptions, package_items, packages, pay_plan_stack_modules, pay_plan_stacks, billing_plans, catalog_items, users, companies RESTART IDENTITY CASCADE"

.PHONY: serve
serve: ## Run the server against the configured DATABASE_URL
	DATABASE_URL="$(DATABASE_URL)" BIND_ADDR="$(BIND_ADDR)" RUST_LOG="$(RUST_LOG)" \
		rtk cargo run --bin payplan-server

.PHONY: ci
ci: fmt clippy test ## Run the full CI suite locally

.PHONY: health
health: ## curl /health on the running server
	curl -s http://127.0.0.1:3000/health | jq .
