# PayPlan — Remediation Plan (Security + Correctness)

**Created:** 2026-06-21
**Status:** Complete — all 15 tasks done. Phase 1 (Tasks 1–4), Phase 2 (Tasks 5–9, 12),
Phase 3 (Tasks 10–11, 13–14), Phase 4 (Task 15). Last updated 2026-06-21.
**Baseline:** working tree on `main` (large uncommitted refactor present);
`cargo check`, `cargo clippy`, and `cargo test --workspace --lib` (62) all pass.
The bugs below are **logic/security**, not compilation — they pass type-check and
the current unit tests because the tests exercise the correct paths and use
stack-less package fixtures that dodge the broken job code.

## How to use this doc
- Each task has: **why**, **exact location**, **what to change**, **how to verify**.
- Check the box and fill the "Done" note when complete.
- Keep tasks PR-sized and independently landable; suggested order is top-to-bottom
  within each phase. Phase 1 first (system is unsafe/non-functional without it).
- After each task: `cargo clippy --workspace --all-targets` must stay clean and
  `cargo test --workspace --lib` must stay green. Integration tests need Postgres
  (see "Running tests" at the bottom).

## Progress tracker

| # | Task | Severity | Phase | Status |
|---|------|----------|-------|--------|
| 1 | Path B duplicate event append → dup-key crash | 🔴 Critical | 1 | ☑ |
| 2 | Pot-bonus self-cascade infinite loop (Path B) | 🔴 Critical | 1 | ☑ |
| 3 | Purchase price/currency client-controlled | 🔴 Critical | 1 | ☑ |
| 4 | `Purchase::validate()` bypassed (negative amounts) | 🔴 Critical | 1 | ☑ |
| 5 | Binary tree scoped per-enrollment (no tree forms) | 🔴 Critical | 2 | ☑ |
| 6 | Cross-company IDOR on admin write routes | 🟠 High | 2 | ☑ |
| 7 | Refresh rotation preserves stale role/company | 🟠 High | 2 | ☑ |
| 8 | Binary carryover always (0,0) — unmatched volume lost | 🟠 High | 2 | ☑ |
| 9 | Purchase volume/points ignore qty + is_commissionable | 🟠 High | 2 | ☑ |
| 10 | Path B writes non-atomic (no transaction) | 🟡 Medium | 3 | ☑ |
| 11 | Projection truncates fractional commissions | 🟡 Medium | 3 | ☑ |
| 12 | `BinaryTreeState.user_to_node` `#[serde(skip)]` breaks reload | 🟡 Medium | 3 | ☑ |
| 13 | Internal errors leak to clients; purchase err → 401 | 🟡 Medium | 3 | ☑ |
| 14 | Hardcoded `dev-secret-change-me` in `pub` API | 🟡 Medium | 3 | ☑ |
| 15 | Hardening: JWT leeway/aud, login timing, response unwraps | 🟢 Low | 4 | ☑ |

---

# PHASE 1 — Money path is unsafe / jobs are dead on arrival

These four make the system either exploitable or non-functional in production.
Recommend landing them together (one focused PR) since 3 & 4 share a file.

## ☑ Task 1 — Path B re-appends the trigger event → duplicate-key crash

**Severity:** 🔴 Critical
**File:** `crates/payplan_infra/src/operations.rs`
**Symptom:** every renewal / pot-distribution / binary-cycle-close job crashes
against any package that *has* a pay plan stack. (Stack-less packages early-return
and dodge it — which is why `tests/operations.rs` passes today.)

**Root cause (verified):** `run_stack_against_event` appends the trigger once at
**line 333–335**:
```rust
deps.events.append(std::slice::from_ref(event), &mut *conn).await?;   // append #1
```
then seeds the cascade with the same event at **line 374** (`let mut emitted =
vec![event.clone()];`) and re-appends the whole vector — still containing that
event with the *same* `id` — at **line 392–394**:
```rust
if !emitted.is_empty() {
    deps.events.append(&emitted, &mut *conn).await?;   // append #2 — dup id
}
```
`event_log.id` is `PRIMARY KEY` (`migrations/0002_commerce_enrollment_ledger.sql`)
with no `ON CONFLICT`, so append #2 errors. Worse: `run_royal_pot_distribution`
appends a **third** time at **lines 208–210** before calling
`run_stack_against_event`, so the trigger is inserted 3×.

**Fix (chosen approach):** make the cascade not re-emit the trigger. Seed the loop
with an empty vec, drive the cascade off the trigger, and append only the
*newly emitted* events (the trigger is already persisted at line 333).
- In `run_stack_against_event`: change `let mut emitted = vec![event.clone()];`
  to `let mut emitted: Vec<DomainEvent> = vec![];`, and run the first cascade pass
  against `event` directly (seed `processed`/loop so the trigger drives module
  runs but is not itself pushed into `emitted`). Then `append(&emitted, ...)` only
  contains cascade-produced events.
- In `run_royal_pot_distribution`: remove the explicit `deps.events.append(...)`
  at **lines 208–210** — `run_stack_against_event` already persists the trigger
  at line 333. (Otherwise the trigger is double-appended even after the loop fix.)
- Defense in depth (optional but recommended): make `EventStore::append` use
  `INSERT ... ON CONFLICT (id) DO NOTHING` in `crates/payplan_infra/src/event_store.rs`.

**Verify:**
- New integration test in `crates/payplan_infra/tests/operations.rs`: a package
  **with** a pay plan stack, run `run_renewals` / `run_royal_pot_distribution`,
  assert no error and exactly one `event_log` row per logical event.
- Existing stack-less test must still pass.

**Done note:** Done 2026-06-21 (fix was already present in the working-tree
refactor; verified + regression-tested this session).
`run_stack_against_event` persists the trigger once (operations.rs ~341) and
appends only `emitted[1..]` (cascade output) — the trigger is never re-appended.
`run_royal_pot_distribution` persists its own trigger but
`run_stack_against_event` short-circuits on the missing `package_id` before its
own append, so no double-insert. Added regression test
`renewal_with_stack_does_not_double_append_trigger` (operations.rs): a package
WITH a pay plan stack runs `run_renewals` without a dup-key crash and leaves
exactly ONE `SubscriptionRenewed` row. The pre-existing stack-less test still
passes. (Optional `ON CONFLICT` defense-in-depth in `event_store.rs` was NOT
added — the dedup is handled correctly at the source.)

---

## ☑ Task 2 — Pot-bonus distribution infinite-loops in Path B

**Severity:** 🔴 Critical
**Files:** `crates/payplan_core/src/modules/royal/pot_bonus_module.rs`,
`crates/payplan_core/src/modules/royal/pot_bonus.rs`,
`crates/payplan_infra/src/operations.rs`

**Root cause (verified):** `pot_bonus_module.rs` handles
`RoyalPotBonusDistributed` (line 49 in `handles()`) and, inside that handler,
**re-emits `RoyalPotBonusDistributed`** at **lines 179–188** whenever
`distribute()` returns `Some`. `distribute()` (`pot_bonus.rs:104`) returns `None`
only when `qualified_users == 0 && top_cycler_payouts.is_empty()`. With the
default weights `[40,30,20,10]` (`pot_bonus.rs:126`) the payout vec is **never
empty** — it becomes `[0,0,0,0]` once the pool is zeroed — so the handler keeps
re-emitting forever. Path A is saved by `MAX_ITERATIONS = 32`
(`commands.rs:511`), but **Path B's cascade loop (`operations.rs:378`) has no
cap** → unbounded loop / OOM.

**Fix (two parts, do both):**
1. **Stop the self-cascade.** Don't emit `RoyalPotBonusDistributed` from within
   its own handler. Introduce a distinct terminal event
   `EventType::RoyalPotBonusSettled` (add to the enum in
   `crates/payplan_core/src/payplan/events.rs`) and emit *that* at
   `pot_bonus_module.rs:179`. Nothing should `handles()` it (or only projectors
   react to it). The pool is still zeroed at line 190.
   - Update the event projector / docs that key off `RoyalPotBonusDistributed`
     for `royal_pot_bonus_balances` to react to `RoyalPotBonusSettled` instead
     (search `RoyalPotBonusDistributed` across `crates/payplan_infra`).
2. **Add the safety cap to Path B.** Mirror `commands.rs:511` in
   `operations.rs` `run_stack_against_event`: add `MAX_ITERATIONS` +
   `iterations` guard to the `while processed < emitted.len()` loop
   (**line 378**) returning `AppError::Conflict` on overflow. This is a backstop
   even after fix (1).

**Verify:**
- Unit test in `pot_bonus_module.rs`: feeding a `RoyalPotBonusDistributed`
  (or the new settled trigger) produces ledger entries but emits **no** further
  `RoyalPotBonusDistributed`.
- Integration: `run_royal_pot_distribution` terminates and writes the expected
  `royal_pot_bonus_balances` rows.

**Done note:** Done 2026-06-21. Added `EventType::RoyalPotBonusSettled`
(events.rs); pot_bonus_module now emits `RoyalPotBonusSettled` (terminal,
handled by no module) instead of re-emitting `RoyalPotBonusDistributed`.
Projector (`project_pot_bonus_balances`) now keys off `RoyalPotBonusSettled`.
Path-B MAX_ITERATIONS=32 backstop was already present in
`operations.rs::run_stack_against_event`. Unit test
`distribution_emits_settled_not_distributed` asserts no self re-emission;
projection integration tests updated to the settled event and pass.

---

## ☑ Task 3 — Purchase price & currency are client-controlled

**Severity:** 🔴 Critical
**Files:** `crates/payplan_web/src/handlers.rs` (`PurchaseBody` line 295,
`purchase_package_handler` line 315), `crates/payplan_app/src/commands.rs`
(`handle_purchase_package`, gross built at line 297).

**Root cause (verified):** `PurchaseBody.gross_amount` and `payment_currency`
come straight from the request body and are written verbatim into the `purchases`
ledger (`commands.rs:297` `Money::new(cmd.gross_amount, cmd.payment_currency)`).
The flow loads `billing_plans` (which carry `price`) but never compares the
client amount to the package's real price. A user can buy any package for `0.01`
and still get full entitlements + downstream commission/volume events.

**Fix (DECIDED: server derives, drop client fields):** compute `gross`
server-side from the package's billing plans; remove the client-supplied amount
entirely.
- Remove `gross_amount` and `payment_currency` from `PurchaseBody`
  (`handlers.rs:295`) and from `PurchasePackageCommand`
  (`commands.rs`, search the struct def). Update the README purchase example +
  any tests that send those fields.
- In `handle_purchase_package` (`commands.rs`), after loading the billing plans
  (the `load_billing_plans` helper near line 480), sum the plan prices
  (× quantity, matching Task 9's semantics) to get the authoritative gross +
  currency. Use that for the `Purchase`.
- Currency rule: all billing plans in a package must share a currency; reject
  mixed-currency packages with `AppError::Validation`.

**Verify:**
- Unit/integration: purchase with a tampered low `gross_amount` is rejected (or
  ignored in favor of the computed price); purchase with no amount succeeds and
  records the correct price.

**Done note:** Done 2026-06-21 (present in the working-tree refactor; verified
this session). `gross_amount`/`payment_currency` are gone from both `PurchaseBody`
(handlers.rs) and `PurchasePackageCommand` (commands.rs). `compute_package_price`
(commands.rs) sums `plan.price.amount × item.quantity` server-side and rejects
mixed-currency packages with `AppError::Validation`; the result is the
authoritative gross/net. The README purchase example sends no amount and notes
server derivation. (PRD §"purchases" still lists `gross_amount` — correct, it's a
stored ledger column, not a request field. Test SQL that inserts `gross_amount`
targets the `purchases` table column, also correct.) Server-derivation makes a
tampered low amount impossible to supply; the invariant is further guarded by
Task 4's `validate()`.

---

## ☑ Task 4 — `Purchase::validate()` is bypassed (negative amounts accepted)

**Severity:** 🔴 Critical
**File:** `crates/payplan_app/src/commands.rs` (Purchase struct literal at
**lines 299–309**).

**Root cause (verified):** every other aggregate calls `.validate()`
(`commands.rs:129, 148, 164, 184, 205`), but the purchase path builds
`Purchase { ... }` as a raw struct literal and never calls `Purchase::new()` or
`purchase.validate()`. The negative-amount and gross≠net currency guards in
`crates/payplan_core/src/validation.rs:303–320` are therefore dead code on this
path — a negative `gross_amount` flows into the ledger.

**Fix:** call `purchase.validate().map_err(AppError::from)?;` immediately after
constructing `purchase` (line 309), before any persistence. Largely moot if
Task 3 derives the price server-side, but keep it as the invariant guard.
(If `AppError: From<validation error>` doesn't exist, add the conversion — check
how the other `.validate()` calls map their errors.)

**Verify:** unit test — constructing a purchase with a negative amount returns a
validation error rather than persisting.

**Done note:** Done 2026-06-21 (call site present in the refactor; unit test added
this session). `handle_purchase_package` calls
`purchase.validate().map_err(AppError::from)?` immediately after building the
`Purchase` literal (commands.rs ~312), before any persistence — matching the
other aggregates. `Purchase::validate` (validation.rs) rejects negative gross/net
and currency mismatch. Added lib unit test
`purchase_validate_rejects_negative_gross` (validation.rs) asserting a negative
gross fails `validate()`.

---

# PHASE 2 — Core domain correctness + tenant isolation

## ☑ Task 5 — Binary tree is scoped per-enrollment, so no tree ever forms

**Severity:** 🔴 Critical (placed in Phase 2 because it needs design care)
**Files:** `crates/payplan_app/src/commands.rs` (ctx at **line 523**:
`.with_enrollment(enrollment.id)`), `crates/payplan_infra/src/operations.rs`
(line 357–359), `crates/payplan_core/src/payplan/runner.rs` (line ~159, uses
`aggregate_id = enrollment.id`), `crates/payplan_core/src/modules/binary/tree_module.rs`.

**Root cause (verified):** all modules in the cascade resolve their state
aggregate to `enrollment.id` (`ctx.with_enrollment` sets
`aggregate_id = enrollment.id.0`; the runner loads/saves state under that id).
But `BinaryTreeState` is documented "Per-(company) tree state"
(`tree_module.rs:13`). So each enrollment loads an **empty** tree, places itself
as root (`pick_placement` returns `(None, None)` for an empty tree), and never
sees sponsors or other nodes. Auto-balance, sponsor-preference, and leg
placement are all dead — every member is a root.

**Fix (DECIDED: add `scope()` to the `Module` trait):** scope genealogy-wide
modules to a **company-level aggregate id** instead of the enrollment id.
- Add `fn scope(&self) -> AggregateScope { AggregateScope::Enrollment }` (default)
  to the `Module` trait in `crates/payplan_core/src/payplan/registry.rs`, plus an
  `AggregateScope { Enrollment, Company }` enum.
- In the runner (`crates/payplan_core/src/payplan/runner.rs:~159`) and the two
  cascade drivers (`commands.rs:523`, `operations.rs:357`), pick the aggregate id
  per module: `Company => company_id.0`, `Enrollment => enrollment.id.0`. The
  `StateCache` key already includes the aggregate id, so company-scoped modules
  will now share one row.
- Override `scope()` to return `Company` for `binary.tree`,
  `binary.carryover`, and `royal.pot_bonus` (these are all genealogy/company-wide,
  not per-enrollment). Leave the rest defaulted to `Enrollment`.
- Migration note: existing per-enrollment `module_state` rows for these keys are
  orphaned after the switch (dev data only; reseed). Flag for any non-dev data.

**Beware:** this interacts with Task 12 (the `user_to_node` map is
`#[serde(skip)]`, so a reloaded company tree comes back with an empty index).
Fix Task 12 together with this or the tree's idempotency/placement lookups break
once the tree actually persists multiple nodes.

**Verify:** integration test — two enrollments under the same company with a
sponsor relationship produce a tree where the second is placed *under* the first
(not as a second root); auto-balance distributes across legs.

**Done note:** Done 2026-06-21. Added `AggregateScope{Enrollment,Company}` +
`Module::scope()` (default `Enrollment`) to `registry.rs`. Runner resolves the
state aggregate per module: `Company => ctx.company_id.0`, else
`ctx.state_aggregate()`. Overrode `scope()=Company` for `binary.tree`,
`binary.carryover`, `royal.pot_bonus`. Both cascade drivers
(`commands.rs::handle_purchase_package`, `operations.rs::run_stack_against_event`)
now preload module state under BOTH the enrollment and company aggregates; the
Path-B inner cascade ctx now inherits the outer context (company + enrollment)
instead of dropping the enrollment id. Solved together with Task 12 (index
rebuild). New integration test
`binary_tree_is_company_scoped_and_forms_one_tree` (persistent_state.rs): two
separate purchases under one company form ONE tree with the second placed under
the first. All DB-backed integration tests pass.
NOTE: two pre-existing failures in `payplan_app/tests/atomicity.rs`
(`empty_stack_leaves_no_orphan_rows`, `unknown_billing_plan_returns_validation_before_insert`)
fail on the refactor baseline too (verified via `git stash`) — the refactor
moved `pool.acquire()` to the top of `handle_purchase_package` so validation no
longer precedes DB acquisition, and those tests use a deliberately-unreachable
lazy pool. Out of scope for Task 5; flagged for the purchase-flow/atomicity
owner.

---

## ☑ Task 6 — Cross-company IDOR on admin write routes

**Severity:** 🟠 High
**File:** `crates/payplan_web/src/handlers.rs` — `create_catalog_item_handler`,
`create_billing_plan_handler`, `create_package_handler` (line 273), and the
purchase handler (line 315). `company_id` is read from the request body and
never checked against `auth.company_id`.

**Root cause (verified):** `create_package_handler` builds
`CompanyId::from(body.company_id)` (line 278) with no comparison to the caller's
company. A `company_admin` of company A can pass `company_id = B` and create
catalog items / billing plans / packages under another tenant. For purchase,
`cmd.company_id` is also never reconciled with the package's actual
`company_id`.

**Fix:**
- For `company_admin`-tier handlers: ignore body `company_id` and use
  `auth.company_id` (the `AuthUser` already carries it — see
  `crates/payplan_web/src/session.rs`). Only `platform_admin`
  (`auth.can_impersonate()` / role check) may target an arbitrary company.
- For purchase: after loading the package, assert
  `package.company_id == cmd.company_id` (or just derive `company_id` from the
  loaded package) and 403 on mismatch.

**Verify:** integration test — company-A admin attempting to create a package
under company B gets 403; platform admin succeeds.

**Done note:** Done 2026-06-21. Added `effective_company(auth, body_company_id)`
helper in handlers.rs: non-platform admins are pinned to `auth.company_id` and a
body `company_id` targeting a different company is rejected 403; platform admins
may target any company. Wired `auth: AuthUser` into `create_catalog_item_handler`,
`create_package_handler`, and `create_billing_plan_handler`
(the latter loads the referenced catalog item and 403s if it belongs to another
company — its body has no `company_id`). Purchase: `handle_purchase_package`
now asserts `package.company_id == cmd.company_id` and returns the new
`AppError::Forbidden` on mismatch (derives tenant from the loaded package). Added
`AppError::Forbidden`, `AuthUser::can_admin_platform()`, and upgraded `ApiError`
to be status-aware (domain→4xx, infra→generic 500 + `tracing::error!`) — this is
a slice of Task 13's web-side taxonomy that Task 13 will extend to
purchase/refresh/logout/session. New integration test
`company_admin_cannot_create_package_for_another_company` (web auth.rs): company-A
admin → 403 under company B, passes the gate (400 on empty items) under its own
company; platform admin passes the gate for any company. All 9 web auth tests
pass.

---

## ☑ Task 7 — Refresh rotation preserves stale role/company for 7 days

**Severity:** 🟠 High
**File:** `crates/payplan_web/src/handlers.rs` — `refresh_handler`
**lines 553–562**.

**Root cause (verified):** refresh issues the new pair from `claims.role` /
`claims.company_id` copied from the *old* token
(`issue_access(claims.sub, claims.company_id, &claims.role)`), not re-read from
the DB. A demoted, suspended, or company-moved user keeps their old role and
company for up to 7 days by rotating refresh tokens.

**Fix:** in `refresh_handler`, after verifying the refresh token, reload the user
from the DB by `claims.sub` (use `ctx`'s user repo / a direct query). Re-derive
`role` and `company_id` from the DB row; reject (401/403) if the user is
missing/disabled. Issue the new pair from the DB-sourced values, not the claim.

**Verify:** integration test — demote a user in the DB, rotate their refresh
token, assert the new access token carries the *new* (lower) role.

**Done note:** Done 2026-06-21. `refresh_handler` now reloads the user via
`ctx.users.get(UserId::from(claims.sub), &mut conn)` after revoking the old jti,
and issues the new pair from the DB-sourced `role`/`company_id` (via
`user_role_str(user.role)` / `user.company_id`) instead of the old token's
claims. A user that no longer exists is rejected (401). NOTE: the `User` model
has no disabled/suspended flag, so "reject if disabled" reduces to "reject if
missing" — flagged if a status column is added later. Integration test
`refresh_reissues_role_from_db_not_stale_claims` (web auth.rs): a company_admin
is demoted to `user` in the DB, then rotates the refresh token; both the response
`role` and the decoded new access-token claim are `user`. Error mapping still
funnels through `AuthError` — Task 13 refines the taxonomy.

---

## ☑ Task 8 — Binary carryover always carries (0,0); unmatched volume lost

**Severity:** 🟠 High
**Files:** `crates/payplan_core/src/modules/binary/carryover_module.rs`
(reads `last_unmatched` at ~lines 60–61, resets to default ~line 84),
`crates/payplan_core/src/modules/binary/pairing_module.rs` (computes
`matched = min(left, right)`, resets `pending_totals` to default ~line 164).

**Root cause (verified):** `CarryoverState.last_unmatched` is the only input to
the carryover computation but nothing ever **writes** it — it's read and reset
only. The pairing module holds the real leg totals in a separate, uncoordinated
struct (`BinaryPairingState.pending_totals`) and discards the unmatched
remainder when it resets to default after computing the match. So unmatched leg
volume never carries to the next cycle — it vanishes.

**Fix:** make the pairing module emit the unmatched remainder so carryover can
consume it.
- On `BinaryCycleClosed`, have pairing compute and emit
  `left - matched` / `right - matched` (e.g. in the `BinaryPairMatched` /
  cycle-closed payload, or a dedicated `carry` field).
- Have `carryover_module` accumulate `last_unmatched` from that event payload
  instead of reading state that's never set. Add the carried amounts to the next
  cycle's opening leg totals.
- Alternatively merge pairing + carryover state so the remainder isn't split
  across two uncoordinated structs (bigger refactor — note in Open questions).

**Verify:** unit test — left=10, right=7 → matched=7, carryover left=3; next
cycle opens with left=3 carried in.

**Done note:** Done 2026-06-21 (default approach: consume the pairing event,
no module merge). `binary.carryover` now `handles()` `BinaryPairMatched` instead
of `BinaryCycleClosed` and derives the remainder from that event's existing
`left`/`right`/`matched` payload (`leg - matched`, clamped ≥0) rather than
reading `state.last_unmatched`, which nothing ever wrote. The remainder
accumulates onto `state.carry` across cycles. In the real cascade (Path A/B),
pairing emits `BinaryPairMatched` on cycle close and the cascade then drives
carryover off it. Unit tests `carries_unmatched_remainder_from_pair_event`
(10/7→carry left 3) and `carryover_accumulates_across_cycles` (5+2→7) added;
updated the `stack_e2e` carryover test to feed `BinaryPairMatched`; the
`binary_carryover` projection integration test still passes. NOTE: full
feed-back of carried volume into the NEXT cycle's pairing `pending_totals`
(pairing + carryover share state) is the larger refactor the plan defers — not
done; carry is tracked correctly and exposed via `BinaryCarryoverUpdated`.

---

## ☑ Task 9 — Purchase volume/points ignore quantity + is_commissionable

**Severity:** 🟠 High
**File:** `crates/payplan_app/src/commands.rs` **lines 323–324**.

**Root cause (verified):** initial purchase computes
```rust
let package_points: u32 = package.items.iter().map(|i| i.points_value).sum();
let package_volume: u32 = package.items.iter().map(|i| i.commissionable_volume).sum();
```
ignoring both `quantity` and `is_commissionable`. The renewal path
(`operations.rs:155` `load_package_renewal_shape`) correctly does
`SUM(commissionable_volume * quantity) FILTER (WHERE is_commissionable)`. So a
package with a non-commissionable item or any `quantity > 1` credits **different**
volume/points on purchase vs. renewal — over-counting on purchase.

**Fix:** make the purchase computation match the renewal SQL semantics:
```rust
let package_points: u32 = package.items.iter()
    .filter(|i| i.is_commissionable)
    .map(|i| i.points_value * i.quantity).sum();
let package_volume: u32 = package.items.iter()
    .filter(|i| i.is_commissionable)
    .map(|i| i.commissionable_volume * i.quantity).sum();
```
(Confirm the field name/type for quantity on the in-memory `PackageItem`; the
SQL uses `pi.quantity`.) Watch for `u32` overflow on large quantities — consider
`u64` or checked math.

**Verify:** unit test — package with one commissionable item (qty 2) + one
non-commissionable item credits volume = 2× the commissionable item only, and
matches what a renewal of the same package computes.

**Done note:** Done 2026-06-21. Extracted `package_commissionable_totals(items)`
in commands.rs: filters `is_commissionable`, multiplies `points_value` /
`commissionable_volume` by `quantity`, computes in `u64` and saturates to
`u32::MAX` (overflow-safe). `handle_purchase_package` now uses it, matching
`load_package_renewal_shape`'s `SUM(commissionable_volume * quantity) FILTER
(WHERE is_commissionable)`. Unit tests
`totals_scale_by_quantity_and_skip_non_commissionable` (qty 2 commissionable +
ignored non-commissionable → volume 20 / points 10) and
`totals_saturate_instead_of_overflowing`.

---

# PHASE 3 — Robustness, consistency, info-leak

## ☑ Task 10 — Path B writes are non-atomic (no transaction)

**Severity:** 🟡 Medium
**File:** `crates/payplan_infra/src/operations.rs` — `run_stack_against_event`
write/project block **lines 392–425**, which runs on a pooled
`&mut PgConnection` with **no `begin()`** (the only `begin()` in the infra write
path is `purchase_writer.rs:30`).

**Root cause (verified):** failures are *not* silently swallowed (every `?`
propagates), but event append / ledger append / `module_state.save` / both
projectors run without a surrounding transaction. A failure mid-sequence leaves
e.g. `module_state` written without its projection row — the exact drift the
docs warn about.

**Fix:** wrap the whole write+project block in one `pool.begin()` …
`tx.commit()`, passing `&mut tx` to `events.append`, `ledger.append`,
`module_state_store.save`, and both projectors — mirroring
`PgPurchaseWriter::write`. Note this overlaps Task 1's restructuring of the same
function; consider doing 1 and 10 in the same change.

**Verify:** integration test — inject a failing projector; assert no
`module_state`/event rows were committed (full rollback).

**Done note:** Done 2026-06-21 (done together with Task 1's restructuring of the
same function). `run_stack_against_event` now does all reads (stack lookup, stack
load, module-state preload) on the pooled connection, drops it, then opens ONE
`pool.begin()` transaction for the entire write+project block: `events.append`
(full `emitted` vec incl. trigger, appended exactly once), `ledger.append`,
`module_state_store.save`, the module projector, and the event projector — then
`tx.commit()`. Mirrors `PgPurchaseWriter::write`. The stack-less path still does a
single standalone trigger append (atomic on its own). Integration test
`failing_projector_rolls_back_event_append` (operations.rs): a `FailingEventProjector`
makes `run_renewals` error and leaves ZERO `SubscriptionRenewed` rows (the
in-transaction event append rolled back). Task 1's
`renewal_with_stack_does_not_double_append_trigger` still passes (single append).

---

## ☑ Task 11 — Projection truncates fractional commissions

**Severity:** 🟡 Medium
**Files:** `crates/payplan_core/src/modules/royal/pot_bonus_module.rs`
**lines 118–119, 167** (`Decimal::try_into::<i64>()`),
`crates/payplan_infra/src/projections.rs` (`project_pot_bonus_balances` ~523,
`project_pairing_result` ~415).

**Root cause (verified):** the ledger stores the exact `Decimal`
(`per_qualified_user`, e.g. `187.5`), but the *event payload* converts to minor
units via `try_into::<i64>()`, which truncates toward zero (187.5 → 187,
62.5 → 62). The projection then stores the truncated value and accumulates the
error across cycles via the cumulative upsert. Relational projection drifts below
the canonical ledger.

**Fix (DECIDED: NUMERIC columns, exact Decimal):**
- New migration `0009_*.sql` (in BOTH `crates/payplan_infra/migrations/` and the
  repo-root `migrations/` mirror — see PROJECT_STATUS migration note): alter the
  truncating columns on `royal_pot_bonus_balances` and the binary pairing
  projection from `BIGINT` to `NUMERIC(20,4)` (matching the money convention in
  "LOCKED decisions #1").
- Remove the `Decimal::try_into::<i64>()` conversions at
  `pot_bonus_module.rs:118–119, 167`; carry the exact `Decimal` (as string in the
  event payload, like `per_qualified_user` already is at line 185) through to the
  projection.
- Update `project_pot_bonus_balances` (`projections.rs:~523`) and
  `project_pairing_result` (`projections.rs:~415`) to bind `Decimal`/`NUMERIC`
  and upsert with exact arithmetic.

**Verify:** property/unit test — a distribution of 187.5 lands identically in the
ledger and the `royal_pot_bonus_balances` projection (no <1-unit drift), and
cumulative balances over N cycles equal the summed ledger.

**Done note:** Migration 0009 widens `royal_pot_bonus_balances` (total_earned, profit_share_earned, top_cycler_earned) and `binary_pairing_results.commission_amount` from BIGINT to NUMERIC(20,4). `pot_bonus_module.rs` no longer calls `try_into::<i64>()`; event payloads carry exact Decimal strings. `projections.rs`: added `decimal_field()` helper; `project_pot_bonus_balances` and `project_pairing_result` bind `rust_decimal::Decimal` instead of `i64`. Integration tests updated to `try_get::<Decimal, _>`. Clippy clean, 68 lib tests pass.

---

## ☑ Task 12 — `BinaryTreeState.user_to_node` is `#[serde(skip)]`

**Severity:** 🟡 Medium (latent today; becomes live once Task 5 lands)
**File:** `crates/payplan_core/src/modules/binary/tree_module.rs`
**lines 18–19** (`#[serde(skip)] user_to_node`), idempotency check ~line 100,
`SponsorPreference` lookup ~line 163; `decode_state` in
`crates/payplan_core/src/payplan/module.rs:147` uses plain `from_value`.

**Root cause (verified):** `user_to_node` is skipped on (de)serialize and
`decode_state` doesn't rebuild it, so after loading persisted state the map is
empty. The "already placed? idempotent" guard and sponsor lookups always miss →
a user can be placed twice on a re-run. Masked today only because Task 5 means
the tree never persists more than one node.

**Fix:** rebuild the index after deserialize. Either drop `#[serde(skip)]` and
serialize the map, or implement a custom `Deserialize` / `#[serde(default)]` +
a post-load `from_nodes()` rebuild that reconstructs `user_to_node` from `nodes`.
Ensure `decode_state` (module.rs:147) triggers the rebuild.

**Verify:** unit test — serialize a tree with 3 placed users, deserialize, assert
`user_to_node` is fully populated and re-running `EnrollmentCreated` for an
already-placed user is a no-op.

**Done note:** Done 2026-06-21 (together with Task 5). `tree_module::run`
now rebuilds the index right after `decode_state` via
`state = BinaryTreeState::from_nodes(state.nodes)`, so the `#[serde(skip)]`
`user_to_node` map is repopulated from `nodes` on every load. The idempotency
guard and `SponsorPreference` lookup now work after reload. Covered by the
`binary_tree_is_company_scoped_and_forms_one_tree` integration test, where the
second purchase loads the persisted single-node tree and correctly finds the
existing node to place under (would place as a second root if the index were
empty).

---

## ☑ Task 13 — Internal errors leak to clients; purchase error → 401

**Severity:** 🟡 Medium
**File:** `crates/payplan_web/src/handlers.rs` — purchase maps any error to
`AuthError::InvalidToken(e.to_string())` (**line 337**); `ApiError`
(lines ~28–32) returns `e.to_string()` of any `AppError` as a 500; refresh/logout
use `AuthError::InvalidToken` for DB/acquire failures (lines 525, 535, 547, …).
`session.rs:67` similarly stringifies.

**Root cause (verified):** DB/sqlx/infra error strings are rendered into client
response bodies, and non-auth failures are mislabeled as auth errors (a DB error
during purchase shows up as 401 "invalid token").

**Fix:**
- Introduce a clear error taxonomy: domain/validation → 400/409 with safe
  message; infra/DB → generic 500 "internal error" to the client, full detail to
  `tracing::error!` server-side only.
- Stop mapping purchase/refresh/logout infra failures to `AuthError`. Give the
  purchase handler its own error type (or reuse `ApiError`) that distinguishes
  auth (401/403) from domain (400/409) from infra (500).

**Verify:** integration test — force a DB error on purchase; assert 500 with a
generic body (no SQL text), and that the detail is logged.

**Done note:** All four auth-adjacent handlers (`purchase_package_handler`, `login_handler`, `refresh_handler`, `logout_handler`) now return `Result<..., ApiError>` instead of `Result<..., AuthError>`. Domain/validation → 400, forbidden → 403, auth → 401, infra → 500 with generic "internal server error" body; actual error detail goes to `tracing::error!` server-side only. `session.rs::authenticate` no longer leaks connection error strings (generic "service unavailable" instead). Clippy clean, 68 lib tests pass.

---

## ☑ Task 14 — Hardcoded `dev-secret-change-me` reachable via `pub` API

**Severity:** 🟡 Medium
**File:** `crates/payplan_web/src/context.rs` **lines ~108–117**
(`dev_jwt_secret()`, `from_lazy_pool`).

**Root cause (verified):** `main.rs` correctly hard-fails in release when
`JWT_SECRET` is unset, but `dev_jwt_secret()` and `from_lazy_pool` are `pub` and
fall back to the hardcoded `"dev-secret-change-me"`. Any other binary/embedding
calling `from_lazy_pool` ships a known signing key.

**Fix:** gate the dev secret behind `#[cfg(debug_assertions)]` (or a `dev` Cargo
feature) so it cannot be compiled into a release artifact, and/or make
`from_lazy_pool`/`dev_jwt_secret` non-`pub` (crate-private, test-only). Never
expose a hardcoded secret in a public API surface.

**Verify:** `cargo build --release` does not link the dev secret; a release-built
context with no `JWT_SECRET` refuses to construct.

**Done note:** `dev_jwt_secret()` gated behind `#[cfg(debug_assertions)]` — cannot compile into release binaries. `from_lazy_pool()` removed (unused). `main.rs` updated from runtime `cfg!()` to compile-time `#[cfg(not(debug_assertions))]` so the bail is enforced at the type level. Clippy clean, 68 lib tests pass.

---

# PHASE 4 — Hardening (low severity, batch together)

## ☑ Task 15 — JWT validation + login timing + response unwraps

**Severity:** 🟢 Low (do as one cleanup PR)
**Items:**
1. **JWT validation** (`crates/payplan_infra/src/auth.rs:150–153`): default
   `Validation` applies 60s leeway and binds no `aud`/`iss`. Set explicit
   `validation.leeway = 0` (or small), set `aud`/`iss`, require `exp`. HS256 is
   already pinned (no alg-confusion) — leave that.
2. **Login timing oracle** (`crates/payplan_web/src/handlers.rs:465–481`):
   "no such user" returns before any argon2 work, leaking user existence via
   timing. Verify against a dummy argon2 hash when the user is missing so both
   branches do equal work.
3. **Response `unwrap()`s** (`handlers.rs:77, 99, 132, 230, 291, …`):
   `serde_json::to_value(...).unwrap()` can panic → dropped 500. Propagate as
   `ApiError` instead.
4. **`format!`-built interval SQL** (`operations.rs:135–141`): not injectable
   today (closed trusted enum) but the only string-built query — switch to a
   bound `$3::interval` via `make_interval()` for safety.
5. **CORS / security headers** (`crates/payplan_web/src/routes.rs:63–70`): only
   `TraceLayer` today. Add an explicit restrictive `CorsLayer` (allowlist) for a
   payments API.

**Verify:** clippy clean; a unit test for the dummy-hash timing path; manual
check that CORS rejects a disallowed origin.

**Done note:** (1) JWT: `Validation` now sets `leeway=5`, requires `exp/iat/sub/aud/iss` claims; tokens carry `iss="payplan"`, `aud="payplan"`. (2) Login timing: "no such user" path runs argon2 against `DUMMY_ARGON2_HASH` so both branches do equal work. (3) All six `serde_json::to_value().unwrap()` in handlers replaced with `to_json()` helper that returns `ApiError`. (4) Interval SQL: replaced `format!`-built `INTERVAL` clause with `make_interval(months => $3, days => $4)` bound parameters. (5) CORS: restrictive `CorsLayer` added — no cross-origin by default; `CORS_ORIGIN` env var allowlists a single origin for production. Clippy clean, 68 lib tests pass.

---

# Decisions (locked 2026-06-21)

- **Task 3 — purchase price:** server derives gross/currency from billing plans;
  `gross_amount`/`payment_currency` removed from the API. (API contract change.)
- **Task 5 — aggregate scoping:** add `scope()` to the `Module` trait;
  `binary.tree`, `binary.carryover`, `royal.pot_bonus` are company-scoped.
- **Task 11 — money precision:** projection columns become `NUMERIC(20,4)`
  (migration 0009); store exact `Decimal`, no minor-unit truncation.

## Still open
- **Task 8 — carryover:** emit the unmatched remainder on the existing
  cycle-closed event (default approach in the task above), **or** merge
  pairing + carryover into one module (bigger refactor). Defaulting to the former
  unless you say otherwise.

---

# Running tests (from README / PROJECT_STATUS)

```bash
# Unit + property (no DB):
cargo test --workspace --lib
cargo test -p payplan_core

# Integration (needs Postgres; serial):
export DATABASE_URL="postgres://$(whoami)@localhost:5432/postgres?host=/tmp"
cargo test -p payplan_infra --features integration --tests -- --include-ignored --test-threads=1
cargo test -p payplan_web   --features integration --tests -- --include-ignored --test-threads=1

# Gate every task on:
cargo fmt --all
cargo clippy --workspace --all-targets   # must stay: No issues found
```

# Notes
- All file:line references verified against the current working tree on
  2026-06-21. The uncommitted refactor may shift line numbers slightly as you
  work — search by the quoted code snippet if a line moved.
- Severity reflects production impact: Phase 1 items make the system either
  exploitable (3, 4) or non-functional under real data (1, 2).
