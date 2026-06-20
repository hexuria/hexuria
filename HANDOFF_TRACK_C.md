# Track C (JWT Auth) — Progress & Handoff

**Last updated:** session boundary. Track C is **~80% complete and compiles clean**. Two steps remain: tests + final verification.

## Current state

- ✅ `cargo check --workspace --all-targets` — **0 errors**
- ✅ `cargo test -p payplan_core` — **96 passed** (77 unit + 19 property)
- ✅ `cargo test --workspace --lib` — **62 passed**
- ⏳ Step 9 (tests) — NOT started
- ⏳ Step 10 (final verification incl. integration tests) — NOT started

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

### Step 9 — Tests (NOT started)

**Two test files to create:**

#### A. `crates/payplan_infra/tests/auth.rs` (integration feature-gated)

Follow the exact pattern of the existing integration tests (`projections.rs`, `operations.rs`):
```rust
#![cfg(feature = "integration")]
use payplan_infra::postgres::{connect, PgConfig};
use payplan_infra::migrator;
use payplan_infra::auth::{JwtService, PgRevokedJtiStore};
use payplan_app::ports::{RevokedJtiStore, TokenService, TokenKind};
use sqlx::PgPool;

async fn pool() -> PgPool { /* same as other tests: from_env → connect → migrator::run */ }
```

Tests to write:
1. **`revoked_jti_round_trip`** — `PgRevokedJtiStore::revoke` then `is_revoked` returns true; unknown jti returns false; re-revoke is idempotent (no error). Seed a `users` row first (FK target).
2. **`revoked_jti_truncate_helper`** — add `revoked_jti` to the TRUNCATE list.

Note: the JwtService unit tests already exist inline in `auth.rs` (4 tests, all passing), so the integration test only needs the `PgRevokedJtiStore` coverage.

#### B. `crates/payplan_web/tests/auth.rs` (HTTP-level, needs a new dev-dep)

**This is the harder one.** Testing axum handlers end-to-end requires building the `AppContext` with a real pool + a JWT secret, then using `axum::body::Body` + `tower::ServiceExt::oneshot` to send requests.

**Decision needed before writing**: does `payplan_web` have `tower` with `util` feature + `http-body-util` for the test? Check `payplan_web/Cargo.toml` `[dev-dependencies]`. You may need to add:
```toml
[dev-dependencies]
tower = { workspace = true, features = ["util"] }
http-body-util = "0.1"
tokio = { workspace = true, features = ["rt-multi-thread", "macros"] }
```

End-to-end test to write:
1. **`login_then_purchase_then_logout_blocks_reuse`**:
   - Seed: company + user (via SQL, argon2-hash the password via `PasswordService`)
   - POST `/api/auth/login` with email/password → assert 200 + token pair
   - POST `/api/purchases` with the access token + matching `user_id` → assert 201
   - POST `/api/auth/logout` with the access token → assert 204
   - POST `/api/purchases` again with the now-revoked access token → assert 401
2. **`purchase_for_other_user_is_403`**:
   - Login as a regular `UserRole::User`
   - POST `/api/purchases` with a DIFFERENT `user_id` → assert 403
3. **`missing_token_is_401`**:
   - POST `/api/purchases` with no Authorization header → assert 401
4. **`admin_job_requires_platform_admin`** (optional):
   - POST `/admin/jobs/renewals/run` with a regular user token → assert 403
   - POST `/admin/jobs/renewals/run` with no token → assert 401

**The axum 0.7 oneshot pattern**:
```rust
use tower::ServiceExt;
use http_body_util::BodyExt;

let app = build_router(ctx);
let resp = app
    .oneshot(
        Request::builder()
            .method("POST")
            .uri("/api/auth/login")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    )
    .await
    .unwrap();
assert_eq!(resp.status(), StatusCode::OK);
let body = resp.into_body().collect().await.unwrap().to_bytes();
let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
```

### Step 10 — Final verification

```bash
export DATABASE_URL="postgres://$(whoami)@localhost:5432/postgres?host=/tmp"
cargo check --workspace --all-targets          # 0 errors
cargo test --workspace --lib                    # 62+ passed
cargo test -p payplan_core                      # 96 passed
cargo test -p payplan_infra --features integration --tests -- --include-ignored --test-threads=1
cargo test -p payplan_web --tests               # new auth HTTP tests
cargo clippy --workspace --all-targets          # no new warnings
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
