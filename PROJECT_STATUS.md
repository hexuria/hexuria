# PayPlan MLM — Project Status (authoritative handoff)

**Updated after sessions completing Tracks A2–A5, B1–B4, D, and C (partial).**
This supersedes the original project status doc.

## Quick health check
- `cargo check --workspace --all-targets` → **0 errors**
- `cargo test -p payplan_core` → **96 passed** (77 unit + 19 property)
- `cargo test --workspace --lib` → **62 passed**
- `cargo test -p payplan_infra --features integration --tests` → **26 passed** (6 files)
- `cargo clippy --workspace --all-targets` → 0 errors; ~21 warnings (mostly pre-existing + a few new from Track C, see below)

## How to run tests
```bash
# Unit + property (no DB needed)
cargo test --workspace --lib
cargo test -p payplan_core

# Integration tests (need Postgres on /tmp:5432)
export DATABASE_URL="postgres://$(whoami)@localhost:5432/postgres?host=/tmp"
cargo test -p payplan_infra --features integration --tests -- --include-ignored --test-threads=1
```

---

## Architecture (unchanged)
```
payplan_core/  — entities, events, runner, modules (flushline, binary, pot, duplication)
payplan_app/   — ports (traits), commands, errors, module registry
payplan_infra/ — Postgres impls, migrator, purchase_writer, operations (jobs), auth
payplan_web/   — axum routes + handlers (AppContext) + session (auth middleware)
payplan_server/— main binary
```

---

## Track completion summary

| Track | Status | What it delivered |
|---|---|---|
| **A2–A5** | ✅ Done | `ModuleProjector` + `PgProjections` — materializes `module_state` JSON into `royal_flushline_accounts`, `binary_nodes`, `binary_volume_ledger`, `binary_carryover` |
| **B1** | ✅ Done | `EventProjector` trait + `PgEventProjector` — `RoyalAccountDuplicated` materializes new enrollment + flushline account |
| **B2** | ✅ Done | `BinaryCycleClosed` advances `binary_nodes.cycle_count`; `BinaryPairMatched` inserts `binary_pairing_results` |
| **B3** | ✅ Done | Fixed `run_renewals`: per-node leg alternation, `node_id` in payload, `recurrence_interval` honored, `billing_type='recurring'` filter, volume SUM honors `quantity × is_commissionable` |
| **B4** | ✅ Done | `royal_pot_bonus_balances` table; `RoyalPotBonusDistributed` payload enriched with per-user `distributions`; cumulative upsert |
| **D** | ✅ Done | 19 property tests (volume 5, carryover 4, pot_bonus 6, duplication 4) |
| **C** | ⏳ ~80% | JWT auth built & compiling; **tests + final verification remain** (see `HANDOFF_TRACK_C.md` for full detail) |
| **E** | ❌ Not started | Documentation, clippy/fmt warning cleanup |

---

## Migrations (8 total, 0006–0008 are new this work)

| # | File | Purpose |
|---|---|---|
| 0001 | `platform.sql` | companies, users, catalog_items, billing_plans, packages, package_items |
| 0002 | `commerce_enrollment_ledger.sql` | purchases, subscriptions, entitlements, enrollments, event_log, reward_ledger |
| 0003 | `royal_modules.sql` | royal_flushline_accounts, royal_qualifications, royal_pot_bonus_pool, royal_matrices, royal_matrix_slots |
| 0004 | `binary_modules.sql` | binary_nodes, binary_volume_ledger, binary_cycle_periods, binary_pairing_results, binary_carryover |
| 0005 | `module_state.sql` | module_state (opaque JSON per module+aggregate) |
| **0006** | `binary_nodes_cycle_count.sql` | **NEW** — `ALTER TABLE binary_nodes ADD COLUMN cycle_count` |
| **0007** | `royal_pot_bonus_balances.sql` | **NEW** — per-user cumulative pot earnings |
| **0008** | `revoked_jti.sql` | **NEW** — JWT revocation table |

Each migration exists in BOTH `crates/payplan_infra/migrations/` (canonical, embedded via `sqlx::migrate!`) AND `migrations/` (repo-root mirror).

---

## New types/traits added (for reference)

### Projection system (Tracks A2–A5, B1–B4)
- `payplan_app::ports::ModuleProjector` — reacts to `StateChange` blobs → relational tables
- `payplan_app::ports::EventProjector` — reacts to emitted `DomainEvent`s → relational tables (B1/B2/B4)
- `payplan_infra::projections::PgProjections` — impl of ModuleProjector (flushline, binary_nodes, binary_volume, binary_carryover)
- `payplan_infra::projections::PgEventProjector` — impl of EventProjector (duplication, pairing_result, cycle_count, pot_bonus_balances)
- Both wired into `PgPurchaseWriter` (Path A, transactional) and `operations::run_stack_against_event` (Path B, best-effort)
- State structs augmented: `BinaryNode` (+company_id/enrollment_id), `BinaryVolumeEntry` (+node_id), `BinaryCarryover` (+company_id/node_id)

### Auth system (Track C)
- `payplan_app::ports::{TokenService, RevokedJtiStore, TokenClaims, TokenKind}`
- `payplan_infra::auth::{JwtService, PgRevokedJtiStore}` (HS256, access 15min / refresh 7d)
- `payplan_web::session::{AuthUser, AuthError, authenticate, require_authenticated, require_company_admin, require_platform_admin}`
- `AppContext` gained `tokens: Arc<dyn TokenService>` + `revoked_jti: Arc<dyn RevokedJtiStore>`
- **`AppContext::new` signature changed**: `new(pool)` → `new(pool, jwt_secret: String)`

---

## Known issues / warnings

### Pre-existing (not introduced this session)
- `payplan_app/tests/atomicity.rs` — 2 tests fail with DNS error (they use `PgPool::connect_lazy("postgres://x@y/z")` and `handle_purchase_package` acquires from the pool before validation short-circuits). Pre-existing from the port refactor; unrelated to our work.
- 8× `field 'pool' is never read` — the Pg*Repo structs hold a pool field they no longer use (port refactor moved to `&mut PgConnection`). Track E cleanup.
- 1× `this expression creates a reference which is immediately dereferenced` at `operations.rs:322` — pre-existing `&mut conn` in `module_state_store.save`.
- 2× `unused variable: conn` in `aggregate_repos.rs` — `PgPackageRepo::insert` / `PgPayPlanStackRepo::insert` take `conn` but use `pool`. Track E.
- 1× `unused import: ModuleStateStore` in `commands.rs`. Track E.
- 1× `unused import: PgConnection` in `purchase_writer.rs`. Track E.

### Introduced by Track C (cleanup candidates for Track E)
- **3× `very complex type`** in `session.rs:162,181,187` — the `require_role`/`require_company_admin`/`require_platform_admin` return types are very long `impl Fn(...) -> Pin<Box<...>>` signatures. Fix: extract a `type` alias or box the middleware differently.
- **2× `#[must_use] redundant`** on `require_company_admin()`/`require_platform_admin()` — the `impl Fn` return is already `#[must_use]`. Fix: drop the `#[must_use]` attribute.
- **2× `unused import: TokenService / RevokedJtiStore`** in `session.rs` — will clear once the tests use them (or add `#[allow(unused_imports)]` until then). Actually these ARE used (in `authenticate`), so this may be a clippy false-positive from the conditional path; verify.

### `AppContext::new` signature change (breaking if missed)
- The ONLY caller is `payplan_server/src/main.rs:52` → already updated to `AppContext::new(pool, jwt_secret)`.
- `from_lazy_pool` delegates to `new(pool, dev_jwt_secret())` so any future dev callers are fine.
- If a new caller is added, it MUST pass a JWT secret.

---

## Remaining work

### Track C — tests + verification (see `HANDOFF_TRACK_C.md` for full detail)
1. `crates/payplan_infra/tests/auth.rs` — `PgRevokedJtiStore` integration tests (revoke → is_revoked; idempotent; unknown jti → false)
2. `crates/payplan_web/tests/auth.rs` — HTTP-level end-to-end tests (login → purchase → logout → 401 on reuse; purchase-for-other-user → 403; missing token → 401). **Requires adding `tower` (util feature) + `http-body-util` + `tokio` (macros) to `payplan_web/Cargo.toml [dev-dependencies]`.**
3. Run full verification suite.

**Note:** `seeds/dev.sql` has NO platform_admin user. To manually test admin endpoints, you must seed one via SQL (argon2-hash a password and insert with `role='platform_admin'`).

### Track E — documentation + warning cleanup (NOT started)
- Resolve the 13 pre-existing + 7 Track C warnings (listed above)
- The `session.rs` complex-type warnings can be fixed by introducing a `type RoleMiddleware = ...` alias
- `cargo fmt --all` (formatting hasn't been run)
- Update `docs/PRD.md` §11 Persistence Model to add `royal_pot_bonus_balances` and `binary_nodes.cycle_count`
- Update the architecture docs to mention the EventProjector layer

### Design decisions that are LOCKED (do not re-litigate)
1. Money: `rust_decimal::Decimal` + currency string, `NUMERIC(20,4)`
2. IDs: UUIDv7, `serde(transparent)`, sqlx support
3. Module state: opaque JSON at `(module_key, module_version, aggregate_id)`
4. Purchase flow: build all aggregates in memory → cascade → persist atomically via `PgPurchaseWriter`
5. All write ports take `&mut PgConnection` (join caller's tx)
6. **Projections**: `ModuleProjector` (state→tables) + `EventProjector` (events→tables) — two separate traits, both wired into Path A (transactional) and Path B (best-effort)
7. **Auth**: access (15min) + refresh (7d) HS256 tokens, `revoked_jti` table, refresh is single-use (rotated)
8. **Route gating**: public / authenticated / company_admin / platform_admin sub-routers via `from_fn_with_state`
9. **Registration**: public, but role forced to `User` (no privilege escalation)
10. **Purchase gate**: regular users must purchase for themselves; admins can impersonate
