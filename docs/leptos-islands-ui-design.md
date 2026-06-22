# Leptos Islands Admin UI — Validated Implementation Plan

**Project:** PayPlan Platform  
**Objective:** Add a server-rendered admin UI beside the existing JSON API while keeping the browser WASM bundle proportional to interactive controls, not to the number or size of pages.  
**Validated:** 2026-06-22 against the current workspace, stable crates, `cargo-leptos 0.3.6`, and the official [Leptos Islands guide](https://book.leptos.dev/islands.html).  
**Status:** Implemented and verified in the workspace; not yet committed.

---

## 1. Outcome and non-negotiable architecture

The first release is a traditional server-rendered multi-page application (MPA) on top-level UI
routes. The JSON API remains isolated under `/api`.
Leptos renders complete HTML documents on the server. Only small controls that require browser
state or browser events use `#[island]`.

This is the rule that protects the bundle:

> Page shells, navigation, tables, page data, forms, and ordinary content are server components.
> An island contains only the minimum interactive control and small serializable props it needs.

Required Leptos islands setup:

```rust
// payplan_ui/src/lib.rs
#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    #[allow(unused_imports)]
    use crate::islands::*;
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_islands();
}
```

```rust
// In the server-rendered document <head>.
<HydrationScripts options=options islands=true/>
```

The allowed `use crate::islands::*` import must remain in the hydration entrypoint. In a workspace
build, it keeps the island exports reachable for `wasm-bindgen`.

Do **not** enable the islands router in the first release. Navigation uses ordinary links and full
document requests. This avoids shipping client router code and avoids depending on limited
client-side router behavior in islands mode.

---

## 2. Validated dependency baseline

### 2.1 Versions

Use the stable versions available at validation time:

| Dependency | Current workspace | Target | Notes |
|---|---:|---:|---|
| `leptos` | — | `0.8.19` | Stable; enable `islands` in the UI crate |
| `leptos_axum` | — | `0.8.9` | Resolves `leptos ^0.8.19` and `axum ^0.8` |
| `leptos_router` | — | `0.8.13` | Server route generation; exclude from the WASM feature set |
| `leptos_meta` | — | `0.8.6` | SSR metadata support |
| `cargo-leptos` | installed | `0.3.6` | Workspace frontend/server build coordinator |
| `axum` | `0.7` | `0.8.9` | Required by `leptos_axum` |
| `axum-extra` | — | `0.12.6` | Cookie parsing/building through the `cookie` feature |
| `tower` | `0.5` | keep `0.5` | Compatible |
| `tower-http` | `0.6` | keep `0.6` | Matches `leptos_axum`; do not introduce `0.7` |
| `serde` | `1` | keep `1` | Island props require serialization |
| `wasm-bindgen` | — | compatible `0.2` | Let the lockfile and cargo-leptos keep CLI/crate versions aligned |

Do not use `leptos 0.9.0-alpha`. It is a prerelease and provides no required capability for this
work.

Do not add direct dependencies on `tachys`, `leptos_integration_utils`, or `server_fn` until code
directly imports their public APIs. `leptos` and `leptos_axum` already bring in the required
internals.

### 2.2 Axum 0.7 to 0.8 migration

Treat the compiler and tests as the source of truth. The known change in this repository is the
custom `AuthUser` extractor:

- Remove `#[axum::async_trait]` from `FromRequestParts<AppContext>`.
- Implement the Axum 0.8 trait using `async fn` or a returned `impl Future`.
- Recompile all handlers and middleware to catch new `Send`/`Sync` requirements.
- Preserve the current route paths and state behavior unless a compiler error requires a change.

The current routes do not use Axum's changed `/:param` syntax. `State`, `Router::merge`, and the
existing route handlers should not be rewritten speculatively.

Leptos issues
[#2083](https://github.com/leptos-rs/leptos/issues/2083) and
[#2880](https://github.com/leptos-rs/leptos/issues/2880) are closed. They are historical context,
not active blockers. The workspace export precaution described above is retained.

---

## 3. Workspace and crate structure

Keep `payplan_server` as the native binary package. Add one shared UI library package:

```text
mlm/
├── Cargo.toml
├── Cargo.lock
├── crates/
│   ├── payplan_core/
│   ├── payplan_app/
│   ├── payplan_infra/
│   ├── payplan_web/
│   ├── payplan_server/
│   └── payplan_ui/
│       ├── Cargo.toml
│       ├── src/
│       │   ├── lib.rs
│       │   ├── app.rs
│       │   ├── shell.rs
│       │   ├── dto.rs
│       │   ├── auth.rs
│       │   ├── actions/
│       │   ├── components/
│       │   ├── islands/
│       │   └── pages/
│       ├── style/
│       │   └── tailwind.css
│       └── public/
└── target/site/
```

Responsibilities:

- `app.rs`: server route tree and top-level application component.
- `shell.rs`: document shell, metadata, stylesheet, and hydration scripts.
- `pages/`: SSR page composition and server data loading.
- `components/`: reusable server-rendered components.
- `islands/`: the only modules intended to execute in the browser.
- `dto.rs`: compact serializable props shared with islands; never expose domain aggregates or
  secrets directly.
- `actions/`: UI POST workflows and Post/Redirect/Get responses.
- `auth.rs`: UI principal lookup and role guards; browser token issuance remains in application
  services, not in view code.

### 3.1 Feature boundary

`payplan_ui` must build as both an `rlib` and `cdylib`:

```toml
[lib]
crate-type = ["cdylib", "rlib"]

[features]
default = []
hydrate = [
  "leptos/hydrate",
  "dep:console_error_panic_hook",
  "dep:wasm-bindgen",
]
ssr = [
  "leptos/ssr",
  "dep:leptos_meta",
  "dep:leptos_router",
  "leptos_meta/ssr",
  "leptos_router/ssr",
  "dep:payplan_app",
  "dep:payplan_core",
  "dep:payplan_web",
]
```

The exact dependency declarations should follow these rules:

- `leptos` is shared and has `features = ["islands"]`.
- `leptos_meta` and `leptos_router` are optional SSR-only dependencies. Guard `app`, `shell`,
  `pages`, `actions`, and server-only component modules with `cfg(feature = "ssr")`.
- `payplan_app`, `payplan_core`, and `payplan_web` are optional and activated only by `ssr`.
- Axum, SQLx, Tokio, `leptos_axum`, and `tower-http` belong to the server package or SSR-only
  dependencies; none may enter `payplan_ui/hydrate`.
- `serde` is shared because island props are serialized into the server HTML.

Add a `payplan_server/ssr` feature that activates `payplan_ui/ssr`. Cargo-leptos must build:

- `payplan_server` with `--no-default-features --features ssr`;
- `payplan_ui` for `wasm32-unknown-unknown` with
  `--no-default-features --features hydrate`.

### 3.2 cargo-leptos workspace configuration

Add one project to the root `Cargo.toml`:

```toml
[[workspace.metadata.leptos]]
name = "payplan-ui"
bin-package = "payplan_server"
bin-target = "payplan-server"
lib-package = "payplan_ui"

site-root = "target/site"
site-pkg-dir = "pkg"
assets-dir = "crates/payplan_ui/public"
tailwind-input-file = "crates/payplan_ui/style/tailwind.css"
site-addr = "127.0.0.1:3000"
reload-port = 3001

bin-features = ["ssr"]
bin-default-features = false
lib-features = ["hydrate"]
lib-default-features = false
lib-profile-release = "wasm-release"

server-fn-prefix = "/_server"
hash-files = true
js-minify = true
```

Add a dedicated WASM profile:

```toml
[profile.wasm-release]
inherits = "release"
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
```

Use `cargo leptos build --release --precompress` for production. Do not introduce Trunk or a custom
WASM build pipeline unless cargo-leptos fails with a reproduced, documented blocker.

Track `Cargo.lock`. CI already uses `cargo build --locked`, so ignoring the application lockfile is
internally inconsistent.

---

## 4. Runtime integration and routing

Keep both interfaces in one process:

```text
/health                         existing public health endpoint
/api/*                          existing JSON API and bearer authentication
/admin/jobs/*                   existing JSON/admin job endpoints
/login                         public SSR login page and POST action
/logout                        authenticated UI POST action
/{packages,companies,...}      authenticated SSR pages and resource POST actions
/jobs/*                        authenticated operation POST actions
/_server/*                     reserved Leptos server-function endpoints
/pkg/*                         generated JS, WASM, and CSS
```

The JSON API contract remains backward compatible. Browser cookies must not silently become an
alternative credential for `/api/*`.

### 4.1 Server state

Introduce a server-only state that contains both application and Leptos configuration:

```rust
#[derive(Clone)]
pub struct ServerState {
    pub app: AppContext,
    pub leptos: LeptosOptions,
}
```

Implement `FromRef<ServerState>` for `LeptosOptions` and, if handlers need it, for `AppContext`.
The existing API router may continue to call `.with_state(AppContext)` and become `Router<()>`
before it is merged with the finalized UI router.

Build the UI router with `generate_route_list(App)` and
`LeptosRoutes::leptos_routes_with_context`. Provide the cloned `AppContext` to SSR and server
functions through Leptos context. Apply `ServerState` only after all UI routes are registered, then
merge the resulting state-free router with the existing API router.

Use `<Router>` with absolute top-level routes. The API, generated assets, health endpoint, and
server-function namespace already provide the required separation.

### 4.2 Navigation model

- Use normal top-level links such as `<a href="/packages">`.
- Page changes request a new server-rendered document.
- Do not use client router hooks in islands.
- Preserve filter/search/page values in query parameters.
- Use Post/Redirect/Get for successful mutations to prevent accidental duplicate submissions.

---

## 5. WASM ownership rules

Before writing a component, classify it:

| Behavior | Implementation |
|---|---|
| Page layout, sidebar links, headings, cards | Server component |
| Database query and authorization | Server application/query service |
| Table rows and pagination links | Server component |
| Create/update form submission | HTML POST action |
| Validation errors | SSR response or redirect flash state |
| Mobile sidebar open/close | Small island |
| Destructive action confirmation | Small island or native `<dialog>` island |
| Client-only formatting that browser APIs already solve | Prefer native HTML/CSS |
| Chart | SSR HTML/CSS first; island only after a measured UX requirement |

Specific prohibitions:

- Do not mark a page, table, dashboard, navigation shell, or complete form as `#[island]`.
- Do not call the JSON API from SSR code.
- Do not fetch initial page data from an island.
- Do not pass full row collections or domain aggregates as island props.
- Do not import `payplan_app`, `payplan_core`, `payplan_web`, SQLx, Axum, Tokio, or Leptos router
  modules from `islands/`.
- Do not add a crate to the hydrate feature before checking its effect on the release WASM.

Initial allowed islands:

1. `MobileNavToggle` — local open/closed state only.
2. `ConfirmSubmit` — label, message, and target form identifier only.
3. A focused field widget only if native HTML cannot provide the required behavior.

Every new island must include a short source comment explaining why server HTML is insufficient.

---

## 6. Authentication and request security

The current API accepts `Authorization: Bearer <access_token>`. Keep that behavior unchanged.

### 6.1 Extract reusable auth workflows

The login, refresh, and logout logic currently lives in Axum handlers. Move the workflow logic into
`payplan_app` application services so both JSON handlers and UI actions call the same code:

```rust
pub struct LoginInput {
    pub email: String,
    pub password: String,
}

pub struct IssuedTokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: UserId,
    pub company_id: Option<CompanyId>,
    pub role: UserRole,
}

pub async fn login(..., input: LoginInput) -> AppResult<IssuedTokenPair>;
pub async fn refresh_tokens(..., refresh_token: &str) -> AppResult<IssuedTokenPair>;
pub async fn revoke_tokens(..., access: &str, refresh: Option<&str>) -> AppResult<()>;
```

Preserve timing-safe invalid-login behavior, token claims, expiry, refresh rotation, and revocation
semantics. Existing JSON response fields and status codes remain unchanged.

### 6.2 Browser cookies

Use two cookies:

- `payplan_access`: access JWT, `HttpOnly`, `Secure` outside local development,
  `SameSite=Lax`, `Path=/`, max age aligned to the 15-minute token expiry.
- `payplan_refresh`: refresh JWT, same attributes, max age aligned to the seven-day expiry.

Cookie values must never be exposed through Leptos props, HTML, logs, JavaScript, or browser storage.

Add `axum-extra` with only the `cookie` feature to parse and construct cookies.

UI authentication middleware:

1. Allow `/login`, generated assets, and health routes without a UI principal.
2. Read the access cookie.
3. Verify token kind/signature/expiry and check `revoked_jti`.
4. Insert `AuthUser` into request extensions.
5. Redirect document requests to `/login?next=<validated-local-path>` when unauthenticated.
6. Return 401/403 for action/server-function requests rather than returning login HTML.

On access-token expiry, the UI may rotate once using the refresh cookie, set the new pair, and
continue. If refresh fails, clear both cookies and redirect to login.

Only accept exact known UI paths as `next` values; otherwise redirect to `/`.

### 6.3 CSRF and authorization

For every state-changing UI request:

- Require POST.
- Verify `Origin` matches the configured public origin; if `Origin` is absent, verify `Host`.
- Keep `SameSite=Lax` cookies.
- Enforce the authenticated role and tenant in the application service, not only in rendered UI.
- Never trust a submitted company ID for a company administrator. Derive company scope from
  `AuthUser`; only platform administrators may select another company explicitly.

Role-based rendering improves UX but is not an authorization boundary.

---

## 7. Query layer required by the UI

`payplan_app/src/queries.rs` currently contains only placeholder types. The listed dashboard pages
cannot be implemented safely until application-level read APIs exist.

Create a query port owned by `payplan_app` and implemented by `payplan_infra`:

```rust
pub struct PageRequest {
    pub page: u32,
    pub page_size: u32,
    pub query: Option<String>,
}

pub struct Page<T> {
    pub items: Vec<T>,
    pub page: u32,
    pub page_size: u32,
    pub total_items: u64,
}

pub enum TenantScope {
    Company(CompanyId),
    Platform,
}

#[async_trait]
pub trait AdminQueryService: Send + Sync {
    async fn dashboard(&self, scope: TenantScope) -> AppResult<DashboardView>;
    async fn companies(&self, request: PageRequest) -> AppResult<Page<CompanyRow>>;
    async fn users(&self, scope: TenantScope, request: PageRequest) -> AppResult<Page<UserRow>>;
    async fn catalog(&self, scope: TenantScope, request: PageRequest) -> AppResult<Page<CatalogRow>>;
    async fn billing(&self, scope: TenantScope, request: PageRequest) -> AppResult<Page<BillingRow>>;
    async fn packages(&self, scope: TenantScope, request: PageRequest) -> AppResult<Page<PackageRow>>;
    async fn purchases(&self, scope: TenantScope, request: PageRequest) -> AppResult<Page<PurchaseRow>>;
}
```

Use purpose-built view structs rather than exposing persistence rows. View structs contain only
fields needed by the page.

Defaults:

- `page = 1`;
- `page_size = 25`;
- maximum `page_size = 100`;
- trim search input and treat an empty value as no filter;
- stable ordering by newest/created timestamp where available, then ID as a deterministic tie-break.

Add the query service to `AppContext`. Implement queries in `payplan_infra` with parameterized SQL
and tenant predicates included in the database query.

Do not add UI-only query logic directly to Leptos pages or Axum handlers.

---

## 8. Page contract and delivery order

### 8.1 First vertical slice

Deliver these routes first:

| Route | Access | Server-rendered behavior | Island |
|---|---|---|---|
| `/login` | Public | Login form and errors | None |
| `/` | Authenticated | Summary counts and recent activity | Mobile nav only |
| `/packages` | Authenticated | Scoped package list, search, pagination | None |
| `/logout` | Authenticated POST | Revoke tokens, clear cookies, redirect | Confirm control optional |

This slice proves routing, cookies, SSR data, tenant scope, cargo-leptos, Tailwind, and bundle size
before the broad UI is built.

### 8.2 Remaining pages

Implement in this order:

1. `/companies` — platform administrators; list and create using the existing create command.
2. `/catalog` — company administrators; list and create.
3. `/billing` — company administrators; list and create.
4. `/purchases` — authenticated and tenant-scoped; list history.
5. `/users` — platform/company administrator list only until admin user-management commands
   exist.
6. `/jobs` — platform administrators; POST actions for the three existing jobs.

Do not label a page “CRUD” unless corresponding create/read/update/delete application operations
exist. The current application supports several create operations but does not provide complete
edit/delete command coverage.

### 8.3 Error behavior

- Validation failure: return the same form page with field/global errors and no secret values.
- Not authenticated: redirect document GET requests to login.
- Not authorized: render a 403 page.
- Missing entity: render a 404 page.
- Query/database failure: render a generic 500 page and log the internal error with a request ID.
- Mutation success: redirect to the canonical list/detail page with a short success message.
- Job actions: include an idempotency/duplicate-run warning in the UI; authorization remains
  platform-admin only.

---

## 9. Styling and asset pipeline

Use Tailwind CSS v4 through cargo-leptos:

```css
/* crates/payplan_ui/style/tailwind.css */
@import "tailwindcss";
```

Cargo-leptos 0.3.6 defaults to a Tailwind v4 standalone CLI and does not require
`tailwind.config.js`. Keep content discoverable through ordinary Rust source files and avoid
runtime-generated class names that Tailwind cannot detect.

Use custom components, semantic HTML, and accessible native controls. Desktop-first is acceptable,
but the navigation and data tables must remain usable on narrow screens. Dark mode is not part of
the first vertical slice.

Production assets:

- release-minified CSS and JS;
- hashed filenames;
- precompressed gzip and Brotli variants;
- immutable cache headers for hashed `/pkg/*` assets;
- no-cache or short-cache policy for SSR HTML.

---

## 10. Execution phases

### Phase 0 — Baseline and Axum upgrade

1. Ensure `Cargo.lock` and this document are tracked.
2. Record the current green commands.
3. Upgrade workspace Axum to `0.8.9`.
4. Update the custom extractor and fix compiler-proven incompatibilities.
5. Run all existing unit, integration, and HTTP auth tests.

**Exit criteria:** Existing API behavior is unchanged and the workspace is green on Axum 0.8.

### Phase 1 — Islands build proof

1. Add `payplan_ui`, feature boundaries, cargo-leptos metadata, Tailwind input, and WASM profile.
2. Add a static `/login` page and one `MobileNavToggle` island.
3. Wire the route list, document shell, static fallback, and merged router.
4. Build development and release outputs.
5. Inspect HTML for one `<leptos-island>` and verify the static page remains functional without JS.

**Exit criteria:** `cargo leptos build --release --precompress` succeeds and no server crate appears
in the WASM dependency tree.

### Phase 2 — Browser authentication

1. Extract reusable auth application services.
2. Keep JSON handlers as adapters around those services.
3. Add UI cookie actions, refresh rotation, route middleware, role checks, and CSRF origin checks.
4. Add login/logout tests with and without JavaScript.

**Exit criteria:** Browser login survives page navigation, logout revokes both tokens, and the JSON
API remains bearer-only.

### Phase 3 — Query service and vertical slice

1. Implement `AdminQueryService` and add it to `AppContext`.
2. Build the SSR admin shell, dashboard, and package list.
3. Add query parameters, pagination, empty states, error pages, and tenant tests.
4. Measure and record bundle sizes.

**Exit criteria:** The vertical slice meets the size budgets below and is usable with JavaScript
disabled except for the explicitly interactive control.

### Phase 4 — Administration pages

Add companies, catalog, billing, purchases, users, and jobs in the order defined above. Reuse
application commands and query services. Introduce no island without a measured interaction need.

**Exit criteria:** Each page has role/tenant tests, no-JS coverage, and no regression in existing
API contracts.

### Phase 5 — Production hardening

1. Add release cargo-leptos and bundle-budget jobs to CI.
2. Verify precompressed asset serving and cache headers.
3. Add CSP/nonces, public-origin configuration, request IDs, and sanitized error pages.
4. Document environment variables, deployment artifact layout, health checks, and rollback.

**Exit criteria:** A clean checkout produces the deployable server and `target/site` assets using
the documented commands.

---

## 11. Verification and acceptance criteria

### 11.1 Required commands

```bash
rtk cargo fmt --all -- --check
rtk cargo clippy --workspace --all-targets -- -D warnings
rtk cargo test --workspace
rtk cargo test -p payplan_infra --features integration -- --include-ignored
rtk cargo leptos build --release --precompress
```

Also run a hydrate-only dependency inspection:

```bash
rtk cargo tree -p payplan_ui \
  --target wasm32-unknown-unknown \
  --no-default-features \
  --features hydrate
```

The tree must not contain `axum`, `sqlx`, `tokio`, `payplan_infra`, `payplan_web`,
`leptos_axum`, or `leptos_router`.

### 11.2 Bundle budget

For the first vertical slice:

- raw optimized WASM: at most **300 KiB**;
- gzip WASM: at most **120 KiB**;
- adding a server-only content page: no more than **5 KiB** raw WASM growth;
- CI fails when the agreed baseline grows by more than **10%** without an explicit budget update.

Record raw, gzip, and Brotli sizes in CI output. Compare release builds only.

### 11.3 Test matrix

Required automated scenarios:

- Existing API login, refresh, logout, role gates, and revocation remain green after Axum upgrade.
- UI login sets two correctly scoped `HttpOnly` cookies.
- Invalid credentials do not reveal whether an email exists.
- Expired access plus valid refresh rotates once and continues.
- Reused refresh token is rejected.
- Logout revokes tokens and clears both cookies.
- `/api/*` rejects cookie-only authentication.
- Unauthenticated UI document requests redirect to a validated local `next` path.
- Company administrators cannot list or mutate another company's resources.
- Platform administrators receive platform scope only on routes that allow it.
- Search and pagination are deterministic and enforce the page-size maximum.
- SSR output contains islands only for explicitly approved components.
- Login, navigation, filters, forms, and authorization errors work without JavaScript.
- Each island hydrates independently without hydrating the page shell.
- A clean release build produces hashed and precompressed assets.

---

## 12. Handoff checklist

Before closing each phase, the implementing agent must update this document with:

- completed phase and commit reference;
- exact versions resolved in `Cargo.lock`;
- commands run and their result;
- current raw/gzip/Brotli WASM sizes;
- any approved deviation from the WASM ownership rules;
- remaining risks or blocked acceptance criteria.

Do not report “islands implemented” based only on a successful compile. The proof must include:

1. server-rendered page HTML;
2. visible `<leptos-island>` boundaries;
3. independent island interaction in a browser;
4. hydrate-only dependency-tree inspection;
5. measured release WASM sizes;
6. no-JavaScript page and form behavior.

---

## 13. Implementation result — 2026-06-22

All five phases are implemented in the current workspace.

- The admin UI lives in `crates/payplan_ui`; `MobileNavToggle` is the only browser island.
- Top-level UI routes are integrated into `payplan_server`; `/api/*` remains bearer-token-only.
- UI authentication uses site-scoped access and refresh cookies, refresh rotation, logout
  revocation, local-only `next` redirects, role checks, and same-origin mutation checks.
- Admin reads use explicit query DTOs, deterministic pagination, and tenant scope.
- Release assets are hashed and precompressed. The CSP permits WebAssembly compilation through
  `'wasm-unsafe-eval'` without enabling general JavaScript `'unsafe-eval'`.

Verification results:

| Check | Result |
|---|---|
| Workspace check and all features | Passed |
| Clippy with warnings denied | Passed |
| Workspace tests, one test thread | 118 passed across 28 suites |
| Infra integration tests, one test thread | 36 passed across 8 suites |
| Release cargo-leptos build with precompression | Passed |
| Hydrate-only forbidden dependency check | Passed |
| Browser hydration | One island; menu state changed independently; no console errors |
| Server-rendered packages page | Complete rows in initial HTML |
| Protected form action | Created a company and redirected to the SSR list |
| Cross-origin mutation | Rejected with HTTP 403 |

Measured release WASM:

- raw: **116,098 bytes**;
- gzip: **56,038 bytes**;
- Brotli: **41,926 bytes**.

No deviation from the WASM ownership rules was required.

---

## References

- [PayPlan advanced Leptos features guide](./leptos-advanced-features-guide.md)
- [Leptos Islands guide](https://book.leptos.dev/islands.html)
- [Leptos WASM binary-size guidance](https://book.leptos.dev/deployment/binary_size.html)
- [cargo-leptos workspace configuration](https://github.com/leptos-rs/cargo-leptos#workspace-setup)
- [Leptos Axum workspace starter](https://github.com/leptos-rs/start-axum-workspace)
- [Axum changelog](https://github.com/tokio-rs/axum/blob/main/axum/CHANGELOG.md)
