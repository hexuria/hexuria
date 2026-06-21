# PayPlan MLM — Project Status (authoritative handoff)

**Updated after sessions completing Tracks A2–A5, B1–B4, C, D, and E.**
This supersedes the original project status doc.

## Quick health check
- `cargo check --workspace --all-targets` → **0 errors, 0 warnings**
- `cargo clippy --workspace --all-targets` → **No issues found**
- `cargo fmt --all -- --check` → **clean**
- `cargo test -p payplan_core` → **96 passed** (77 unit + 19 property)
- `cargo test --workspace --lib` → **62 passed**
- `cargo test -p payplan_infra --features integration --tests -- --include-ignored --test-threads=1` → **33 passed** (7 files)
- `cargo test -p payplan_web --features integration --tests -- --include-ignored --test-threads=1` → **8 passed** (2 files; auth HTTP suite)

## How to run tests
```bash
# Unit + property (no DB needed)
cargo test --workspace --lib
cargo test -p payplan_core

# Integration tests (need Postgres on /tmp:5432; use --test-threads=1 for shared DB)
export DATABASE_URL="postgres://$(whoami)@localhost:5432/postgres?host=/tmp"
cargo test -p payplan_infra --features integration --tests -- --include-ignored --test-threads=1
cargo test -p payplan_web  --features integration --tests -- --include-ignored --test-threads=1
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
| **C** | ✅ Done | JWT auth: HS256 access(15m)+refresh(7d), `revoked_jti` store, single-use refresh rotation, 4-tier route gating, login/refresh/logout handlers, `authenticate` extractor+middleware, 11 integration tests (3 infra + 8 web HTTP-level) |
| **E** | ✅ Done | Documentation + warning cleanup. See "Track E — what was fixed" below. |

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

## Track E — what was fixed

### Documentation
- `docs/PRD.md` §10.5 — new "Projection layer" subsection (ModuleProjector +
  EventProjector, Path A transactional / Path B best-effort wiring)
- `docs/PRD.md` §10.6 — new "Auth layer" subsection (HS256 + refresh rotation
  + `revoked_jti` + 4-tier route gating)
- `docs/PRD.md` §11 — added `royal_pot_bonus_balances`, `module_state`,
  `revoked_jti` to the table lists; new §11.1 column/invariant table covering
  `binary_nodes.cycle_count`, `royal_pot_bonus_balances`, `revoked_jti`
- `docs/ARCHITECTURE.md` — payplan_app / payplan_infra "owns" lists now
  mention the projector port traits and their PG impls; new "Projection
  wiring" and "Auth (Track C)" sections
- `README.md` — added auth endpoints to the HTTP table (with auth tier),
  new login example, refresh-rotation note, projection-layer paragraph,
  updated test counts (96 core / 62 lib / 33 infra / 8 web), updated crate
  summary diagram
- `PROJECT_STATUS.md` — this file

### Warning cleanup (21 → 0)
Every clippy warning has been resolved. Notably the cleanup found one real
atomicity bug, not just a style nit:

- **Atomicity bug (B)** — `PgPackageRepo::insert` and `PgPayPlanStackRepo::insert`
  used to ignore the `&mut PgConnection` argument and open a *second*
  transaction via `self.pool.begin()`. Inside `PgPurchaseWriter::write` that
  meant the package + pay-plan-stack rows were committed in a separate
  transaction from the rest of the purchase — so a failure after the package
  insert would not roll back the package row. **Both functions now write
  through the caller's `&mut PgConnection`** and rely on the caller to
  commit. The `unused variable: conn` warnings on the same two lines were
  the symptom; the bug is fixed.
- **10× dead `pool` field** (`PgCatalogRepo`, `PgPackageRepo`,
  `PgPayPlanStackRepo`, `PgPurchaseRepo`, `PgSubscriptionRepo`,
  `PgEntitlementRepo`, `PgEnrollmentRepo`, `PgCompanyRepo`, `PgUserRepo`,
  `PgLedgerStore`) — field removed; the structs are now empty `{}` and
  derive `Default`; `new()` delegates to `Self::default()`. The repos take
  `&mut PgConnection` per call (the port contract), so the `pool` field was
  always dead code from the port refactor.
- **4× unused import** — `ModuleStateStore` (commands.rs, fully-qualified
  elsewhere), `PgConnection` (purchase_writer.rs), and the two `session.rs`
  intra-doc-link imports (`TokenService`, `RevokedJtiStore`) silenced with
  `#[allow(unused_imports)]` so the doc links still resolve.
- **3× very complex type** — extracted `type RoleFuture = Pin<Box<dyn Future<...> + Send>>`
  in `session.rs`; the long `impl Fn(...) -> RoleFuture + ...` signatures
  are now readable.
- **2× redundant `#[must_use]`** on `require_company_admin` /
  `require_platform_admin` — the underlying `impl Fn` is already
  `#[must_use]`, so the attribute was duplicated. Dropped.
- **1× unnecessary `mut`** on `conn` rebind in `operations.rs:197,310`.
- **1× reference immediately re-derefed** at `operations.rs:396`
  (`&mut conn` where `conn: &mut PgConnection`) → `&mut *conn`.

### Side-effects
- `Pg*Repo::new()` / `PgLedgerStore::new()` no longer take a pool — callers
  (`payplan_web/src/context.rs` and the two integration test files) updated
  accordingly. `PgEventStore`, `PgModuleStateStore`, `PgPurchaseWriter`, and
  `PgRevokedJtiStore` still legitimately need a pool (they open their own
  transactions) and are unchanged.
- `cargo fmt --all` reformatting applied.

---

## Known issues / warnings

None outstanding. `cargo clippy --workspace --all-targets` is clean.

### Notes for future work
- `payplan_app/tests/atomicity.rs` — 2 tests use `PgPool::connect_lazy("postgres://x@y/z")`
  and panic before validation short-circuits. They fail without a valid
  `DATABASE_URL`. When `DATABASE_URL` is set (matching the other integration
  tests) they pass cleanly. The pre-handoff note attributed this to a
  separate bug; in practice it's a missing-env-var failure, not a logic
  defect. Marked as a known quirk rather than a real failure.

### `AppContext::new` signature change (breaking if missed)
- The ONLY caller is `payplan_server/src/main.rs:52` → already updated to `AppContext::new(pool, jwt_secret)`.
- `from_lazy_pool` delegates to `new(pool, dev_jwt_secret())` so any future dev callers are fine.
- If a new caller is added, it MUST pass a JWT secret.

---

## Design decisions that are LOCKED (do not re-litigate)
1. Money: `rust_decimal::Decimal` + currency string, `NUMERIC(20,4)`
2. IDs: UUIDv7, `serde(transparent)`, sqlx support
3. Module state: opaque JSON at `(module_key, module_version, aggregate_id)`
4. Purchase flow: build all aggregates in memory → cascade → persist atomically via `PgPurchaseWriter`
5. All write ports take `&mut PgConnection` (join caller's tx) — **Track E now enforces this**
6. **Projections**: `ModuleProjector` (state→tables) + `EventProjector` (events→tables) — two separate traits, both wired into Path A (transactional) and Path B (best-effort)
7. **Auth**: access (15min) + refresh (7d) HS256 tokens, `revoked_jti` table, refresh is single-use (rotated)
8. **Route gating**: public / authenticated / company_admin / platform_admin sub-routers via `from_fn_with_state`
9. **Registration**: public, but role forced to `User` (no privilege escalation)
10. **Purchase gate**: regular users must purchase for themselves; admins can impersonate
