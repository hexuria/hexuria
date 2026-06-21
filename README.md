# PayPlan Platform v2

A Rust + Leptos Spin SSR platform for configurable MLM pay plans. Companies
create products and services, bundle them into packages, and attach compensation
pay plan stacks (Royal Flush, Binary, or a custom hybrid). All rewards flow
through an append-only event log + reward ledger.

This is **v2** — the platform is no longer a single MLM app. Royal Flush and
Binary are both first-class built-in module families, and the engine is a
generic pay plan runner that any company can configure.

---

## Status

- ✅ Phase 0–7 of the PRD plan are complete.
- ✅ Royal Flush modules (sponsor, flushline, matrix, pot bonus, duplication) implemented.
- ✅ Binary modules (tree, volume, pairing, carryover) implemented.
- ✅ Module trait + `ModuleRegistry` + `StackRunner` + cascading event loop.
- ✅ Postgres persistence with sqlx.
- ✅ Atomic purchase flow (single transaction).
- ✅ `ModuleProjector` (state → per-module tables) + `EventProjector`
  (emitted events → duplication / pairing / cycle_count / pot balances),
  wired into both `PgPurchaseWriter` (transactional) and operations jobs
  (best-effort).
- ✅ 19 property tests in `payplan_core` (volume, carryover, pot bonus,
  duplication invariants).
- ✅ JWT auth (HS256, 15 min access + 7 day refresh, single-use refresh
  rotation, `revoked_jti` store, 4-tier route gating).
- ✅ argon2 password hashing.
- ✅ Dev seed + Makefile + GitHub Actions CI.
- ⏳ Leptos SSR UI / Spin WASM target (deferred; same handler functions can be wrapped later).

Test counts: **96 in `payplan_core`** (77 unit + 19 property),
**62 lib tests across the workspace**, **33 infra integration tests**,
**8 web auth integration tests**.

---

## Quickstart

Prereqs: Rust stable, Postgres 14+, `rtk` (optional but recommended).

```bash
# 1. Start a local Postgres on the unix socket at /tmp (any Postgres works).
pg_isready -h /tmp

# 2. Verify the workspace builds and tests pass.
make ci

# 3. Apply the dev seed (creates Acme + Royal Flush + Binary packages).
make seed

# 4. Start the server.
make serve
```

The server binds to `0.0.0.0:3000` by default. Override with `BIND_ADDR=...`.

```bash
# Health check
curl http://127.0.0.1:3000/health
# {"modules":[["binary.carryover","1.0.0"], ...], "status":"ok"}
```

---

## HTTP API

All endpoints accept/return JSON. Errors come back as `{"message": "..."}`.
Authenticated endpoints expect `Authorization: Bearer <access_token>`.

| Method | Path                                      | Auth tier         | Purpose                                           |
| ------ | ----------------------------------------- | ----------------- | ------------------------------------------------- |
| GET    | `/health`                                | public            | Liveness + module inventory                       |
| POST   | `/api/auth/login`                        | public            | Email + password → access + refresh pair          |
| POST   | `/api/auth/refresh`                      | public            | Rotate refresh (single-use), returns new pair     |
| POST   | `/api/auth/logout`                       | authenticated     | Revoke the presented token's `jti`                |
| POST   | `/api/users`                             | public            | Self-service signup (role forced to `user`)       |
| POST   | `/api/companies`                         | company_admin+    | Create a company                                  |
| POST   | `/api/catalog_items`                      | company_admin+    | Create a product or service                       |
| POST   | `/api/billing_plans`                      | company_admin+    | Attach one-time or recurring pricing              |
| POST   | `/api/packages`                           | company_admin+    | Bundle catalog items + assign a pay plan stack    |
| GET    | `/api/packages`                           | authenticated     | List all packages (across all companies)         |
| POST   | `/api/purchases`                          | authenticated     | Run the full purchase flow (atomic transaction)   |
| POST   | `/admin/jobs/renewals/run`                | platform_admin    | Manual renewal job                                |
| POST   | `/admin/jobs/royal_pot_distribution/run`  | platform_admin    | Manual Royal Flush weekly pot distribution        |
| POST   | `/admin/jobs/binary_cycle_close/run`      | platform_admin    | Manual Binary cycle close                         |

### Create a company

```bash
curl -X POST http://127.0.0.1:3000/api/companies \
  -H 'Content-Type: application/json' \
  -d '{"name":"Acme MLM","slug":"acme"}'
```

### Register a user

```bash
curl -X POST http://127.0.0.1:3000/api/users \
  -H 'Content-Type: application/json' \
  -d '{
    "email": "buyer@acme.local",
    "password": "correct horse battery staple",
    "role": "user",
    "company_id": "11111111-1111-1111-1111-111111111111"
  }'
```

Passwords are hashed with argon2id (memory=19 MiB, t=2, p=1) before storage.
The server forces `role` to `user` server-side — clients cannot escalate to
`company_admin` or `platform_admin` through signup.

### Log in

```bash
curl -X POST http://127.0.0.1:3000/api/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"email":"buyer@acme.local","password":"correct horse battery staple"}'
# 200 OK
# {
#   "access_token":  "<15-min HS256 JWT>",
#   "refresh_token": "<7-day HS256 JWT>",
#   "token_type":    "Bearer",
#   "expires_in":    900
# }
```

Pass the `access_token` as `Authorization: Bearer <access_token>` on every
authenticated request. The refresh token is single-use — posting it to
`/api/auth/refresh` rotates both tokens and revokes the old `jti`.

### Purchase a package

```bash
curl -X POST http://127.0.0.1:3000/api/purchases \
  -H 'Content-Type: application/json' \
  -d '{
    "company_id": "11111111-1111-1111-1111-111111111111",
    "user_id":    "<user-uuid>",
    "package_id": "55555555-5555-5555-5555-555555555551"
  }'
# The price and currency are derived server-side from the package's billing
# plans (sum of price × quantity); the client cannot set the amount.
# 201 Created
# {
#   "purchase_id": "...",
#   "enrollment_id": "...",
#   "subscription_ids": ["..."],
#   "entitlement_ids": ["..."],
#   "events_emitted": 5,
#   "ledger_entries": 0
# }
```

This single call:
1. Validates the package and billing plans.
2. Creates subscription(s) for any recurring items.
3. Grants entitlements for every package item.
4. Creates the purchase and enrollment records.
5. Loads the package's pay plan stack and runs every module that handles
   `PackagePurchased` / `EnrollmentCreated` events.
6. Persists all events + ledger entries.

All steps 1–6 commit in a **single Postgres transaction** (or roll back
together if anything fails).

---

## Architecture

```
payplan_core    pure domain: entities, events, ledger, all modules,
                Module trait, ModuleRegistry, StackRunner, StateCache
payplan_app     workflows: commands, port traits, PurchaseDeps,
                cascading event loop, ModuleProjector + EventProjector
                port traits
payplan_infra   Postgres impls: repos, EventStore, LedgerStore,
                PgProjections + PgEventProjector, PgPurchaseWriter
                (atomic), JwtService + PgRevokedJtiStore, ops jobs
payplan_web     axum routes + handlers + AppContext composition root,
                session extractor + auth middleware (4-tier route gating)
payplan_server  payplan-server binary
```

### Dependency rule

```
payplan_web ──> payplan_app ──> payplan_core
payplan_infra ──> payplan_app (implements its port traits)
payplan_server ──> payplan_web + payplan_infra + payplan_app
```

`payplan_core` never imports `payplan_infra` (except via the optional `sqlx`
feature flag for the newtype `sqlx::Type` impls).

### Module contract

```rust
pub trait Module: Send + Sync {
    fn key(&self) -> &'static str;            // e.g. "royal.flushline"
    fn version(&self) -> &'static str;        // e.g. "1.0.0"
    fn handles(&self) -> &'static [EventType]; // which events trigger this module
    fn run(&self, ctx: &ModuleContext) -> CoreResult<ModuleResult>;
}
```

`ModuleResult` carries emitted events, ledger entries, optional state change,
and warnings. State changes are persisted as opaque JSON via the engine.

### Engine cascading loop

When a purchase is made, the engine:

1. Builds the initial events (`PackagePurchased`, `EnrollmentCreated`).
2. Runs every module that handles the event in stack `sort_order`.
3. Captures emitted events + ledger entries + state changes.
4. Re-runs the stack against each newly-emitted event (cascades).
5. Caps at 32 iterations to prevent runaway loops.
6. Calls `ModuleProjector` and `EventProjector` to materialise the per-module
   relational tables and event-derived rows (duplication, pairing, cycle
   count, pot balances).
7. Commits all writes in one Postgres transaction.

### Projection layer

The engine writes canonical state to `module_state` (opaque JSON) and emits
`DomainEvent`s. Two projector traits turn those into the per-module
relational tables the rest of the system reads from:

- `ModuleProjector` — `(module_key, module_version, aggregate_id, state)`
  changes → `royal_flushline_accounts` / `binary_nodes` /
  `binary_volume_ledger` / `binary_carryover`.
- `EventProjector` — emitted `DomainEvent`s → rows that can't be
  reconstructed from a single aggregate's state: duplication (new
  enrollment + flushline account), pairing results, `binary_nodes.cycle_count`
  increments, and cumulative `royal_pot_bonus_balances` upserts.

Both projectors are wired into `PgPurchaseWriter::write` (transactional
Path A) and `operations::run_stack_against_event` (best-effort Path B
for renewals and job retries).

### Built-in modules

**Royal Flush stack** (`sponsor.allocation`, `royal.flushline`,
`royal.matrix`, `royal.pot_bonus`, `royal.account_duplication`):
- Flushline: Ten→Jack→Queen→King→Ace with per-tier thresholds (1/2/3/4/5);
  graduates at 15 cumulative points; weekly reset moves graduated accounts
  back to King.
- Matrix: 7-slot binary-shaped matrix with sponsor-first placement;
  cycles when full.
- Pot bonus: 75/25 split; user-level qualification (≥1 graduation AND
  ≥1 cycle).
- Duplication: gated on both signals; emits a new Royal account.

**Binary stack** (`sponsor.allocation`, `binary.tree`, `binary.volume`,
`binary.pairing_bonus`, `binary.carryover`):
- Tree: 4 placement strategies (Manual/SponsorPreference/AutoBalance/
  OutsideLegPreference).
- Volume: from package purchases + renewals.
- Pairing: ratio-based matching with configurable commission % and cap.
- Carryover: per-leg carryover between cycles.

---

## Development

```bash
make help          # Show all targets
make ci            # fmt + clippy + test
make seed          # Apply dev seed
make serve         # Run server
make reset         # Wipe all data (DANGER)
make health        # curl /health
```

### Layout

```
crates/
  payplan_core/      pure domain
  payplan_app/       workflows
  payplan_infra/     Postgres + auth + ops
  payplan_web/       axum routes + handlers + AppContext
  payplan_server/    payplan-server binary
docs/                PRD + architecture + module contract
migrations/          Postgres schema (also embedded in payplan_infra)
seeds/               dev seed
.github/workflows/   CI
```

### Testing

- 96 tests in `payplan_core` (77 unit + 19 property) run on every commit.
- 62 lib tests across the rest of the workspace.
- Integration tests live under `crates/*/tests/` and are gated behind the
  `integration` feature flag. Run them with a real Postgres:

```bash
export DATABASE_URL="postgres://$(whoami)@localhost:5432/postgres?host=/tmp"
cargo test -p payplan_infra --features integration --tests -- --include-ignored --test-threads=1
cargo test -p payplan_web   --features integration --tests -- --include-ignored --test-threads=1
```

The `payplan_web` integration suite is the auth HTTP-level suite (8 tests
covering login, refresh-rotation, logout-revocation, role gating).

### CI

GitHub Actions runs `fmt --check`, `clippy -- -D warnings`, `cargo test`, and
the integration suite against a `postgres:16` service container on every PR.

---

## License

UNLICENSED — internal project.
