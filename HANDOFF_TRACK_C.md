# Track C (JWT Auth) — Progress & Handoff

**Last updated:** Track C is **✅ COMPLETE**. All tests pass; full verification suite green.

## Current state

- ✅ `cargo check --workspace --all-targets` — **0 errors**
- ✅ `cargo test -p payplan_core` — **96 passed** (77 unit + 19 property)
- ✅ `cargo test --workspace --lib` — **62 passed**
- ✅ `cargo test -p payplan_infra --features integration --tests -- --include-ignored --test-threads=1` — **33 passed** (7 files; 3 new auth tests)
- ✅ `cargo test -p payplan_web --features integration --tests -- --include-ignored --test-threads=1` — **8 passed** (2 files; all new HTTP-level auth tests)
- ✅ `cargo clippy --workspace --all-targets` — 0 errors, 21 warnings (all pre-existing or Track E cleanup)

## What's done (Steps 1–8)

### Step 1 — `jsonwebtoken` dependency ✅
- `Cargo.toml` (workspace): added `jsonwebtoken = "9"` (resolves to 9.3.1)
- `crates/payplan_infra/Cargo.toml`: added `jsonwebtoken.workspace = true`
- `crates/payplan_web/Cargo.toml`: added `jsonwebtoken.workspace = true`

### Step 2 — migration `0008_revoked_jti.sql` ✅
- Created at both `crates/payplan_infra/migrations/0008_revoked_jti.sql` AND `migrations/0008_revoked_jti.sql` (mirror)
- Schema:
  ```sql
  CREATE TABLE revoked_jti (
      jti         TEXT PRIMARY KEY,
      user_id     UUID NOT NULL REFERENCES users(id),
      token_type  TEXT NOT NULL CHECK (token_type IN ('access', 'refresh')),
      revoked_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
      expires_at  TIMESTAMPTZ NOT NULL
  );
  CREATE INDEX revoked_jti_expires_at_idx ON revoked_jti(expires_at);
  ```

### Step 3 — ports in `payplan_app/src/ports.rs` ✅
Added after `PasswordPort`:
- `enum TokenKind { Access, Refresh }` with `as_str()` (serde-derived, Copy)
- `struct TokenClaims { sub, company_id, role, jti, kind, exp, iat }` (serde-derived)
- `trait TokenService` — `issue_access`, `issue_refresh`, `encode`, `verify(token, expected_kind)`
- `trait RevokedJtiStore` — `revoke(jti, user_id, kind, expires_at, &mut conn)`, `is_revoked(jti, &mut conn)`
- Both traits take `&mut PgConnection` (join caller's tx)

### Step 4 — infra impls in `payplan_infra/src/auth.rs` ✅
Extended the existing file (alongside `PasswordService`). Added:
- `JwtService` — holds `EncodingKey`/`DecodingKey`/`Header` (HS256). `new(secret)`. Access TTL = 15min, refresh TTL = 7d. `verify` decodes + checks `kind` matches.
- `PgRevokedJtiStore` — `revoke` does `INSERT ... ON CONFLICT (jti) DO NOTHING`; `is_revoked` does `SELECT EXISTS(...)`.
- **4 unit tests** for JwtService (all pass): round-trip, wrong-kind rejection, expired-token rejection, wrong-secret rejection.

### Step 5 — `AppContext` + `main.rs` ✅
- `crates/payplan_web/src/context.rs`:
  - Added fields `tokens: Arc<dyn TokenService>` + `revoked_jti: Arc<dyn RevokedJtiStore>`
  - **Changed `AppContext::new` signature** from `new(pool)` → `new(pool, jwt_secret: String)`
  - Added `dev_jwt_secret()` helper: reads `JWT_SECRET` env, falls back to `"dev-secret-change-me"`
  - `from_lazy_pool` now calls `Self::new(pool, Self::dev_jwt_secret())`
- `crates/payplan_server/src/main.rs`:
  - Reads `JWT_SECRET` from env; **hard-errors in release** if unset/empty, dev-default with warning otherwise

### Step 6 — `session.rs` (auth middleware + extractor) ✅
Filled the previously-empty stub at `crates/payplan_web/src/session.rs`. Wired `pub mod session;` into `lib.rs`.
- `AuthUser { user_id, company_id, role }` + `can_impersonate()` (true for CompanyAdmin+)
- `enum AuthError { MissingToken, InvalidToken(String), Revoked, Forbidden }` → maps to 401/403 via `IntoResponse`
- `authenticate(ctx, &HeaderMap) -> Result<AuthUser, AuthError>` — shared core: parses `Authorization: Bearer <token>`, verifies via `TokenService`, checks `revoked_jti` (fail-closed on store error)
- `AuthUser: FromRequestParts<AppContext>` — handlers can take `auth: AuthUser` directly
- `require_authenticated` — `async fn` middleware (any authenticated user)
- `require_company_admin()` / `require_platform_admin()` — boxed-closure middleware factories using `UserRole::can_admin_company` / `can_admin_platform`
- **Key design**: `authenticate` takes `&HeaderMap` (not `&mut Parts`) so it works from both the extractor path (`FromRequestParts`) and the middleware path (`Request::headers()`)

### Step 7 — auth handlers + security fixes ✅
In `crates/payplan_web/src/handlers.rs`:
- **`login_handler`** — `POST /api/auth/login` `{email, password}` → `TokenPair { access_token, refresh_token, user_id, role }`. Uses `UserRepo::find_by_email` + `PasswordPort::verify`. Same error for bad user / bad password (no enumeration).
- **`refresh_handler`** — `POST /api/auth/refresh` `{refresh_token}` → fresh `TokenPair`. **Single-use rotation**: revokes the presented refresh token's jti before issuing the new pair.
- **`logout_handler`** — `POST /api/auth/logout` `{access_token, refresh_token?}` → `204`. Requires `AuthUser` (so a valid access token is needed to log out). Revokes both jtis (best-effort on refresh).
- **Security fix — `RegisterUserBody`**: dropped the `role` field entirely. `register_user_handler` now forces `UserRole::User`. Closes the privilege-escalation hole. Also strips `password_hash` from the response.
- **Security fix — `purchase_package_handler`**: now takes `auth: AuthUser`. If `!auth.can_impersonate()` (regular user), requires `body.user_id == auth.user_id` else 403. Admins (CompanyAdmin+) may initiate purchases for any user.
- Helper `user_role_str(UserRole) -> &'static str`.

### Step 8 — router split ✅
`crates/payplan_web/src/routes.rs` rewritten into 4 sub-routers:
- **public**: `/health`, `/api/auth/login`, `/api/auth/refresh`, `/api/users` (signup, role forced)
- **authenticated** (any user): `/api/auth/logout`, `/api/purchases`, `GET /api/packages` — `from_fn_with_state(ctx.clone(), require_authenticated)`
- **company_admin** (CompanyAdmin+): `POST /api/companies`, `/api/catalog_items`, `/api/billing_plans`, `/api/packages` — `from_fn_with_state(ctx.clone(), require_company_admin())`
- **platform_admin** (PlatformAdmin only): `/admin/jobs/renewals/run`, `/admin/jobs/royal_pot_distribution/run`, `/admin/jobs/binary_cycle_close/run` — `from_fn_with_state(ctx.clone(), require_platform_admin())`
- All merged + `.with_state(ctx)` + `TraceLayer`
- **Key gotcha**: axum 0.7 requires `from_fn_with_state(state, fn)` (NOT bare `from_fn`) when the middleware extracts `State<AppContext>` — because the sub-routers are built before the outer `.with_state(ctx)` is applied.

---

## What's left

~~### Step 9 — Tests (NOT started)~~ — DONE

~~### Step 10 — Final verification~~ — DONE

### What was added in this session

#### A. `crates/payplan_infra/tests/auth.rs` — `PgRevokedJtiStore` integration tests
3 tests:
1. **`revoked_jti_round_trip`** — unknown jti → not revoked; revoke → revoked; other jti still not revoked; idempotent re-revoke (row count stays 1); `token_type='access'` round-trips.
2. **`revoked_jti_stores_refresh_kind`** — `TokenKind::Refresh` round-trips as `'refresh'` and is queryable.
3. **`revoked_jti_persists_expires_at`** — `expires_at` column receives the caller's timestamp verbatim.

#### B. `crates/payplan_web/tests/auth.rs` — HTTP-level end-to-end tests
8 tests, all gated behind `integration` feature:
1. **`login_then_purchase_then_logout_blocks_reuse`** — full happy path: login → 201 purchase → logout → 401 on reuse.
2. **`purchase_for_other_user_is_403_for_regular_user`** — `body.user_id != auth.user_id` → 403; self-purchase still 201.
3. **`missing_token_is_401`** — no `Authorization` header → 401.
4. **`admin_job_requires_platform_admin`** — no token → 401; regular user token → 403.
5. **`invalid_signature_is_401`** — token signed with wrong secret → 401.
6. **`refresh_rotates_and_revokes_old_refresh_jti`** — first `/refresh` succeeds; reusing the rotated refresh token fails; `revoked_jti` is populated.
7. **`logout_revokes_both_jtis`** — logout with access+refresh inserts exactly 2 rows; subsequent request with revoked access token → 401.
8. **`password_service_smoke`** — argon2 round-trip sanity (catches silent hash-format regressions).

**Run command** (the integration tests share one DB and must be serialized):
```bash
export DATABASE_URL="postgres://$(whoami)@localhost:5432/postgres?host=/tmp"
cargo test -p payplan_web --features integration --test auth -- --include-ignored --test-threads=1
```

---

## Key files changed (all Track C)

| File | Change |
|---|---|
| `Cargo.toml` | +`jsonwebtoken = "9"` |
| `crates/payplan_infra/Cargo.toml` | +`jsonwebtoken.workspace = true` |
| `crates/payplan_web/Cargo.toml` | +`jsonwebtoken.workspace = true` |
| `crates/payplan_infra/migrations/0008_revoked_jti.sql` | NEW |
| `migrations/0008_revoked_jti.sql` | NEW (mirror) |
| `crates/payplan_app/src/ports.rs` | +`TokenKind`, `TokenClaims`, `TokenService`, `RevokedJtiStore` |
| `crates/payplan_infra/src/auth.rs` | +`JwtService`, `PgRevokedJtiStore`, 4 unit tests |
| `crates/payplan_web/src/context.rs` | +`tokens`/`revoked_jti` fields, `new(pool, jwt_secret)`, `dev_jwt_secret()` |
| `crates/payplan_web/src/session.rs` | FILLED (was stub) — `AuthUser`, `AuthError`, `authenticate`, middleware |
| `crates/payplan_web/src/lib.rs` | +`pub mod session;` |
| `crates/payplan_web/src/handlers.rs` | +login/refresh/logout; register forces role=User; purchase gate |
| `crates/payplan_web/src/routes.rs` | split into 4 auth-tiered sub-routers |
| `crates/payplan_server/src/main.rs` | reads `JWT_SECRET`, hard-errors in release if unset |

---

## Design decisions already made (do NOT re-litigate)

1. **Registration**: public `POST /api/users` stays public but forces `role=User`; `body.role` field dropped entirely.
2. **Token model**: access (15min) + refresh (7d), both revocable via `revoked_jti`. Refresh is single-use (rotated on each `/api/auth/refresh` call).
3. **Purchase gate**: regular users require `body.user_id == auth.user_id`; admins (`can_impersonate()`) may impersonate.
4. **Route coverage**: full role gating — PlatformAdmin on `/admin/jobs/*`, CompanyAdmin+ on catalog/billing/package/company creation, any-auth on purchases + list_packages, health/login/register public.
5. **Auth header parsing**: manual `strip_prefix("Bearer ")` — avoids an `axum-extra` dependency.
6. **Middleware state**: `from_fn_with_state(ctx.clone(), fn)` (NOT bare `from_fn`) because sub-routers are built before `.with_state(ctx)`.
7. **Fail-closed**: `is_revoked` store error → deny (treat as revoked).
8. **JWT secret**: `JWT_SECRET` env var; hard-error in release if unset, dev-default `"dev-secret-change-me"` with a `warn!`.

---

## Known gotchas for the next session

- **`payplan_app/tests/atomicity.rs`** still has 2 pre-existing failures (DNS to fake host `postgres://x@y/z`) — unrelated to Track C, pre-existing from the port refactor.
- **`From<AppError> for ApiError`** always maps to 500 (pre-existing). The auth handlers use `AuthError` (401/403) separately, so they're fine, but `ApiError`-returning handlers still don't distinguish 404/409. That's a separate cleanup (Track E).
- **`logout_handler` requires a valid access token** (via `AuthUser` extractor) — this is intentional (you must be authenticated to log out), but it means the integration test must login before logout.
- **`payplan_web` has no `[dev-dependencies]` for HTTP testing** — you'll need to add `tower` (with `util`), `http-body-util`, and `tokio` (with `macros`) before writing the HTTP-level auth tests.
- The `seeds/dev.sql` file may need a `platform_admin` user for manual testing — check whether it already has one before assuming.
