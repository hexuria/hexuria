# Architecture

## Architecture goal

Build a maintainable Rust pay plan platform, not a hardcoded MLM app.

The platform must support multiple companies, package catalogs, one-time and recurring billing, and stackable pay plan modules.

## Layers

### payplan_core

Pure domain layer.

Owns:

- Company identity model
- Catalog item model
- Product/service distinction
- Billing plan model
- Package model
- Purchase model
- Subscription model
- Entitlement model
- Enrollment model
- Pay plan stack model
- Event model
- Reward ledger model
- Royal Flush modules
- Binary modules

Does not own:

- SQL
- HTTP
- Leptos
- Spin
- payment gateway SDKs
- email
- scheduled jobs

### payplan_app

Workflow layer.

Owns:

- create company
- create product or service
- create billing plan
- create package
- purchase package
- create enrollment
- run pay plan engine
- run module stack
- collect events
- collect ledger entries
- close compensation periods
- **projection port traits**:
  - `ModuleProjector` — state JSON → per-module relational tables
  - `EventProjector` — emitted `DomainEvent`s → tables that can't be
    reconstructed from a single aggregate's state (duplication, cycle
    count, pot balances, pairing results)

### payplan_infra

Infrastructure layer.

Owns:

- Postgres repositories
- event store
- ledger store
- module-state store
- **projection implementations**:
  - `PgProjections` — `ModuleProjector` for flushline / `binary_nodes` /
    `binary_volume_ledger` / `binary_carryover`
  - `PgEventProjector` — `EventProjector` for duplication / pairing /
    `binary_nodes.cycle_count` / `royal_pot_bonus_balances`
- **atomic purchase writer** (`PgPurchaseWriter`) — wraps every write
  (purchase, subscription, entitlement, enrollment, events, ledger,
  module state, both projections) in a single Postgres transaction
- **auth** (Track C): `JwtService` (HS256, 15 min access + 7 day refresh),
  `PgRevokedJtiStore`
- email
- payment gateway adapters
- schedulers
- transaction manager

### payplan_web

Leptos Spin SSR delivery layer.

Owns:

- routes
- server functions
- forms
- session extraction
- redirects
- admin pages
- dashboard pages
- company setup
- package builder
- pay plan stack builder

`payplan_web` is not UI-only. It is the SSR web boundary. It should still call `payplan_app` instead of directly mutating domain state.

## Dependency direction

```text
payplan_web -> payplan_app -> payplan_core
payplan_infra -> payplan_app ports
```

Core remains pure. App coordinates. Infra persists. Web delivers.

## Module execution

A package purchase produces events. The pay plan engine loads the package stack and runs modules interested in those events.

```text
PackagePurchased
EnrollmentCreated
  -> PayPlanEngine
  -> active modules
  -> state changes
  -> emitted events
  -> ledger entries
  -> ModuleProjector (state JSON -> per-module tables)
  -> EventProjector  (emitted events -> duplication / cycle / pot rows)
  -> transaction commit
```

## Projection wiring

Both projectors are invoked from two places:

1. **Path A (transactional)** — `PgPurchaseWriter::write` runs both inside
   the same Postgres transaction as the purchase / events / ledger / state
   writes. This is the happy path; a purchase is only "real" when every
   projection row is present.
2. **Path B (best-effort)** — `operations::run_stack_against_event` runs
   both when the engine is replayed against a single event (renewal jobs,
   retries, manual operator triggers). Best-effort because the source
   `module_state` row is the source of truth — the relational tables are a
   denormalized read model.

`ModuleProjector` is keyed on `(module_key, module_version, aggregate_id)` and
materializes `module_state` JSON into the per-module tables. `EventProjector`
reacts to emitted `DomainEvent`s and materializes rows that can't be
reconstructed from a single state blob (e.g. an `RoyalAccountDuplicated`
event materializes a new enrollment + flushline account).

## Auth (Track C)

- HS256-signed JWTs. Access TTL = 15 min, refresh TTL = 7 days. Every token
  carries a UUIDv7 `jti`.
- `revoked_jti` table backs logout and refresh-rotation. `authenticate`
  fails closed if the `jti` is present (or the store errors).
- Refresh tokens are single-use: rotation writes the old `jti` to
  `revoked_jti` in the same transaction that issues the new pair.
- Routes are split into four `Router` groups by required role
  (`payplan_web::routes`):
  - public (`/health`, login, refresh, signup)
  - authenticated (logout, purchases, package listing)
  - company_admin (company / catalog / billing / package creation)
  - platform_admin (admin job triggers)
- `AppContext::new(pool, jwt_secret)` composes the auth deps. The server
  binary supplies the secret from `JWT_SECRET` (with a dev-only default
  from `dev_jwt_secret()`).

## Stack versioning

Pay plan stack versions should be immutable after purchases exist.

When a company changes compensation rules, create a new stack version and assign future purchases to it.
