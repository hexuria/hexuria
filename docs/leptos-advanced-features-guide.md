# Leptos Advanced Features for the PayPlan Admin UI

**Project:** PayPlan Platform  
**Validated:** 2026-06-22  
**Workspace baseline:** `leptos 0.8.19`, `leptos_router 0.8.13`,
`leptos_axum 0.8.9`, `server_fn 0.8.12`, `cargo-leptos 0.3.6`  
**Upstream snapshot reviewed:** Leptos commit
[`1d699ffd`](https://github.com/leptos-rs/leptos/commit/1d699ffd26799ce911e78ce39ead6ee6745ef2f0)
from 2026-06-19  
**Purpose:** Curate advanced Leptos capabilities that are demonstrated primarily in repository
examples, explain their real bundle and runtime effects, and define how they should be introduced
without weakening the current SSR-first islands architecture.

---

## 1. Executive decision

The features reviewed are not one stack of switches that should all be enabled together. They
belong to three different application models:

1. **SSR with isolated islands** — the current PayPlan architecture.
2. **SSR with the islands router** — the current architecture plus client-side document
   navigation and DOM reconciliation.
3. **A hydrated client router with lazy routes** — an SPA-style architecture whose route code
   exists in WASM and can therefore be split.

PayPlan should remain in model 1 and experimentally add model 2. Model 3 should only be used for a
future, deliberately isolated SPA-like workflow such as a visual compensation-plan designer.

The most useful features for the current app are:

| Feature | Decision | Best PayPlan use |
|---|---|---|
| `#[island]` | Keep and expand selectively | Mobile navigation, dialogs, complex editors |
| Islands router | Pilot behind a Cargo feature | Faster UI links, GET filters, pagination |
| SSR modes | Adopt route-by-route | Preserve complete initial HTML while tuning TTFB |
| `#[server]` | Use only for island-owned interactions | Job status, autosuggest, live validation |
| `#[slot]` | Adopt for SSR components | Page headers, actions, filters, empty states |
| Attribute spreading | Adopt carefully | Shared button, link, input, and panel primitives |
| `Portal` | Use inside an island only | Modal, confirmation dialog, command palette |
| Reactive stores | Defer until a complex island exists | Package/billing-plan editor |
| WebSocket server functions | Target the jobs page | Live progress and operation logs |
| `#[lazy]` functions | Optional inside a large interactive island | Rare client-only heavy logic |
| `LazyRoute` | Do not use for current SSR pages | Future SPA sub-application only |
| Subsecond hot patching | Do not add to normal workflow | Optional development experiment |

---

## 2. The critical architecture distinction

### 2.1 Current PayPlan ownership

`crates/payplan_ui/src/app/` is compiled only with the `ssr` feature. The browser build compiles
`crates/payplan_ui/src/islands/` and calls:

```rust
#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    use crate::islands::*;

    console_error_panic_hook::set_once();
    leptos::mount::hydrate_islands();
}
```

This means:

- route components do not enter the WASM binary;
- SQL, Axum, Tokio, application services, and admin query code do not enter the WASM binary;
- adding another SSR page does not require route-level code splitting;
- each `#[island]` is an explicit browser-code ownership decision.

This boundary is stronger than lazy route splitting. Code that is not compiled into WASM is
smaller than code compiled into a deferred WASM chunk.

### 2.2 Islands router

The islands router keeps route rendering on the server. It intercepts same-origin links and GET or
POST forms, requests the next HTML document with an `Islands-Router` header, reconciles the new
document into the current DOM, preserves existing island state, and hydrates newly introduced
islands.

It does **not** turn SSR routes into client components and does not require `LazyRoute`.

### 2.3 Lazy routes

`LazyRoute` is for a hydrated client router. Route view code is compiled into WASM, and
`cargo leptos --split` moves selected functions or routes into deferred WASM chunks.

Adopting it for the existing admin pages would replace the current stronger boundary:

```text
Current: page code is server-only -> 0 bytes of page code in WASM
Lazy route: page code is client-capable -> deferred WASM chunk
```

Therefore, lazy routes do not reduce the current PayPlan bundle. They would only help after making
a section of the product substantially more client-driven.

---

## 3. Islands router

Source:
[Leptos islands_router example](https://github.com/leptos-rs/leptos/tree/main/examples/islands_router)

### 3.1 Minimal integration

Forward an opt-in feature through both workspace packages:

```toml
# crates/payplan_ui/Cargo.toml
[features]
islands-router = [
  "leptos/islands-router",
  "leptos_axum/islands-router",
]

# crates/payplan_server/Cargo.toml
[features]
islands-router = [
  "payplan_ui/islands-router",
  "leptos/islands-router",
  "leptos_axum/islands-router",
]
```

Then change the document shell:

```rust
<HydrationScripts
    options=options
    islands=true
    islands_router=cfg!(feature = "islands-router")
/>
```

The existing `leptos_routes_with_context` server integration can remain. With the Axum feature
enabled, Leptos emits branch markers during SSR and recognizes subsequent requests carrying the
`Islands-Router` header.

### 3.2 Where it helps PayPlan

Good candidates:

- sidebar navigation among `/`, `/packages`, `/companies`, and other admin pages;
- search forms using `GET`;
- pagination links;
- create forms that already return redirects to SSR pages;
- preserving mobile-menu or future filter-island state across navigation.

Expected UX improvement:

- no full browser document reload;
- server remains the source of rendered page content;
- existing islands can preserve local state;
- newly returned islands hydrate independently.

### 3.3 Risks that require a pilot

The upstream README calls the feature a work in progress. The implementation is small global
JavaScript that intercepts same-origin navigation. Before enabling it by default, verify:

- login redirects and expired-session redirects;
- logout cookie removal;
- every protected POST form;
- browser back and forward navigation;
- query-string searches and pagination;
- title, metadata, CSP nonce, and error-page changes;
- focus restoration and screen-reader announcements;
- scroll restoration;
- file downloads and external links;
- island state preservation when the surrounding server content changes;
- concurrent navigation cancellation;
- behavior when JavaScript is disabled.

Links that must force normal browser navigation should use a standard escape such as:

```html
<a href="/export.csv" rel="external">Download export</a>
```

The current client script checks same-origin URLs, not PayPlan-specific authorization or route
ownership. Keep API endpoints, binary downloads, OAuth callbacks, and non-HTML routes out of
navigable anchors, or explicitly mark them external.

### 3.4 Recommended rollout

Build and test both variants:

```bash
rtk cargo leptos build --release --precompress
rtk cargo leptos build --release --precompress --features islands-router
```

`cargo-leptos 0.3.6` forwards `--features` to both targets. Explicit `--bin-features` and
`--lib-features` remain available if the package feature sets later diverge.

Do not make it the default until the navigation acceptance matrix is green.

---

## 4. Islands and nested islands

Source: [Leptos islands example](https://github.com/leptos-rs/leptos/tree/main/examples/islands)

### 4.1 Independent island

The existing navigation toggle is the correct pattern:

```rust
#[island]
pub fn MobileNavToggle() -> impl IntoView {
    let open = RwSignal::new(false);

    view! {
        <button
            type="button"
            aria-expanded=move || open.get().to_string()
            on:click=move |_| open.update(|value| *value = !*value)
        >
            {move || if open.get() { "Close menu" } else { "Open menu" }}
        </button>
    }
}
```

### 4.2 Nested islands and context

An island can provide reactive context to nested islands:

```rust
#[derive(Clone, Copy)]
struct DialogState(RwSignal<bool>);

#[island]
fn DialogController(children: Children) -> impl IntoView {
    let state = DialogState(RwSignal::new(false));
    provide_context(state);

    view! {
        <button on:click=move |_| state.0.set(true)>"Open"</button>
        {children()}
    }
}

#[island]
fn DialogCloseButton() -> impl IntoView {
    let state = expect_context::<DialogState>();

    view! {
        <button on:click=move |_| state.0.set(false)>"Close"</button>
    }
}
```

Use this only when nested islands belong to one interaction boundary. Do not create a global
client store merely to avoid passing small serializable props.

### 4.3 PayPlan island rules

An island is justified when it requires at least one of:

- browser events that must update without navigation;
- browser-only APIs;
- persistent transient state across islands-router navigation;
- streaming updates;
- an interaction whose no-JavaScript fallback is still a normal link or form.

Do not make tables, page shells, query loading, authorization checks, or basic forms islands.

---

## 5. SSR modes

Source: [Leptos ssr_modes example](https://github.com/leptos-rs/leptos/tree/main/examples/ssr_modes)

Leptos 0.8.13 provides:

- `SsrMode::OutOfOrder`;
- `SsrMode::PartiallyBlocked`;
- `SsrMode::InOrder`;
- `SsrMode::Async`;
- `SsrMode::Static`.

### 5.1 Route declaration

```rust
<Route
    path=path!("/packages")
    view=PackagesPage
    ssr=SsrMode::Async
/>
```

### 5.2 PayPlan selection policy

| Route type | Recommended mode | Reason |
|---|---|---|
| Login and synchronous error pages | Default/`OutOfOrder` | No suspended data, so blocking adds no value |
| Admin list and dashboard pages | Keep `Async` initially | Complete initial HTML and reliable no-JS behavior |
| Page with sequential content sections | Benchmark `InOrder` | Can stream earlier markup without client fragment insertion |
| Page with independent panels | Pilot `PartiallyBlocked` | Potentially faster shell while retaining blocking-resource HTML |
| Public immutable documentation | Consider `Static` | Not currently an admin requirement |

The current pages use one main blocking resource per route. `Async` is a defensible baseline because
the page is only sent after its table or dashboard data is ready. Changing modes without measuring
TTFB and complete HTML time would be speculative.

### 5.3 Required benchmark

For each candidate route, record:

- time to first byte;
- time to complete HTML;
- bytes before the main table appears;
- behavior with JavaScript disabled;
- behavior with islands router enabled;
- server query duration;
- browser Largest Contentful Paint.

Do not change all routes to streaming globally.

---

## 6. `#[server]` functions

Sources:

- [Leptos islands_router example](https://github.com/leptos-rs/leptos/tree/main/examples/islands_router)
- [Leptos websocket example](https://github.com/leptos-rs/leptos/tree/main/examples/websocket)

Server functions generate a server implementation and a typed browser stub. They are useful when an
island needs to call the server, but they are not a replacement for existing SSR queries or the
public JSON API.

### 6.1 PayPlan-specific shape

The following target shape assumes Phase 6 adds an `admin_jobs` application service to
`AppContext`:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobStatusDto {
    pub state: String,
    pub completed: u64,
    pub total: u64,
}

#[cfg(feature = "ssr")]
fn current_ui_auth() -> Result<AuthUser, ServerFnError> {
    let parts = expect_context::<axum::http::request::Parts>();

    parts
        .extensions
        .get::<AuthUser>()
        .cloned()
        .ok_or_else(|| ServerFnError::ServerError("authentication required".into()))
}

#[server]
pub async fn job_status(job_id: Uuid) -> Result<JobStatusDto, ServerFnError> {
    let context = expect_context::<AppContext>();
    let auth = current_ui_auth()?;

    context
        .admin_jobs
        .job_status(&auth, job_id)
        .await
        .map_err(|error| ServerFnError::ServerError(error.to_string()))
}
```

Keep DTOs small and independent of SQLx, Axum request types, and internal repository models.

### 6.2 Appropriate uses

- live job status;
- type-ahead search after a minimum input threshold;
- server validation inside a multi-step editor;
- mutation initiated by an island where a normal form fallback remains available.

### 6.3 Inappropriate uses

- loading initial page tables;
- duplicating `/api/*`;
- hiding authorization in the client;
- replacing simple POST-and-redirect forms;
- passing large page models into WASM.

### 6.4 Security requirements

Server functions under `/_server` must receive the same protections as UI actions:

- cookie authentication middleware;
- authorization inside the operation;
- origin validation for mutations;
- safe error mapping;
- request-size limits;
- rate limits for search or validation endpoints;
- WebSocket origin validation before upgrade.

The browser stub is not a security boundary.

---

## 7. Lazy functions and lazy routes

Source: [Leptos lazy_routes example](https://github.com/leptos-rs/leptos/tree/main/examples/lazy_routes)

The APIs exist in the workspace’s stable versions:

- `#[lazy]`;
- `#[lazy_route]`;
- `Lazy<T>`;
- `LazyRoute`;
- `leptos::mount::hydrate_lazy`;
- `cargo leptos build --split`.

### 7.1 Lazy function

`#[lazy]` moves a concrete function into a deferred WASM chunk:

```rust
#[lazy]
async fn build_large_preview(input: PreviewInput) -> PreviewModel {
    render_preview_model(input).await
}
```

Constraints:

- parameters and output must be concrete types;
- the generated function is async even when the original is synchronous;
- splitting only occurs when cargo-leptos is run with `--split`;
- common dependencies can be emitted into shared chunks;
- chunk count and total transferred bytes must be measured, not assumed.

Build:

```bash
rtk cargo leptos build --release --precompress --split
```

Potential PayPlan use:

- a future analytics island with a large client formatter;
- a package preview engine used only after opening an editor;
- optional visualization code.

Do not use `#[lazy]` around tiny handlers or formatters. The extra request and split-loader overhead
can cost more than the code saved.

### 7.2 Lazy route

A lazy client route separates synchronous route-data creation from asynchronously loaded view code:

```rust
struct DesignerRoute {
    model: LocalResource<DesignerModel>,
}

#[lazy_route]
impl LazyRoute for DesignerRoute {
    fn data() -> Self {
        Self {
            model: LocalResource::new(load_designer_model),
        }
    }

    fn view(this: Self) -> AnyView {
        view! {
            <Suspense fallback=|| view! { <p>"Loading designer…"</p> }>
                {Suspend::new(async move {
                    let model = this.model.await;
                    view! { <Designer model/> }
                })}
            </Suspense>
        }
        .into_any()
    }
}
```

Router registration:

```rust
<Route
    path=path!("/designer")
    view=Lazy::<DesignerRoute>::new()
/>
```

Hydration entrypoint:

```rust
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    use crate::app::*;

    console_error_panic_hook::set_once();
    leptos::mount::hydrate_lazy(App);
}
```

### 7.3 Why PayPlan should not apply this to current routes

The current entrypoint uses `hydrate_islands()`, not `hydrate_lazy(App)`. Current route components
are excluded from the hydrate feature. Converting `/packages`, `/companies`, and similar pages to
lazy routes would require making the router and route model client-capable.

That would:

- add a hydrated router to the base WASM;
- add route DTO and view code to split chunks;
- weaken the current server-only dependency boundary;
- create two client navigation mechanisms if islands router is also enabled;
- increase testing and cache invalidation complexity.

Do not combine islands router and hydrated lazy routing for the same route tree.

### 7.4 Valid future boundary

If PayPlan gains a strongly interactive tool, mount it as a separate application boundary:

```text
/*                      SSR pages + islands router
/designer/*             dedicated hydrated/lazy client application
```

The designer would have its own:

- hydrate entrypoint;
- route tree;
- WASM bundle and size budget;
- authorization tests;
- CSP and asset manifest;
- fallback link back to the SSR admin.

---

## 8. Portal

Source: [Leptos portal example](https://github.com/leptos-rs/leptos/tree/main/examples/portal)

`Portal` renders browser-owned content elsewhere in the DOM. On the server it emits no portal
content, so it belongs inside an island and must not contain critical initial content.

```rust
#[island]
pub fn ConfirmOperation() -> impl IntoView {
    let open = RwSignal::new(false);

    view! {
        <button type="button" on:click=move |_| open.set(true)>
            "Run operation"
        </button>

        <Show when=move || open.get()>
            <Portal>
                <div class="modal-backdrop">
                    <section role="dialog" aria-modal="true" aria-labelledby="confirm-title">
                        <h2 id="confirm-title">"Confirm operation"</h2>
                        <button type="button" on:click=move |_| open.set(false)>
                            "Cancel"
                        </button>
                    </section>
                </div>
            </Portal>
        </Show>
    }
}
```

PayPlan uses:

- destructive-operation confirmation;
- command palette;
- rich popover that would otherwise be clipped by a table container;
- job progress overlay.

Prefer native `<dialog>` when it satisfies layout and accessibility requirements. Portal adoption
requires focus trapping, Escape handling, focus return, background inertness, and browser tests.

---

## 9. Slots

Source: [Leptos slots example](https://github.com/leptos-rs/leptos/tree/main/examples/slots)

Slots are compile-time component composition. Used in SSR components, they do not add browser code.

```rust
#[slot]
pub struct PageActions {
    children: ChildrenFn,
}

#[slot]
pub struct PageFilters {
    children: ChildrenFn,
}

#[component]
pub fn PageFrame(
    title: &'static str,
    #[prop(optional)] actions: Option<PageActions>,
    #[prop(optional)] filters: Option<PageFilters>,
    children: Children,
) -> impl IntoView {
    view! {
        <header class="page-heading">
            <h1>{title}</h1>
            {actions.map(|slot| (slot.children)())}
        </header>
        {filters.map(|slot| (slot.children)())}
        <section class="panel">{children()}</section>
    }
}
```

Usage:

```rust
<PageFrame title="Companies">
    <PageActions slot>
        <a class="button" href="/companies/new">"New company"</a>
    </PageActions>
    <PageFilters slot>
        <SearchForm/>
    </PageFilters>
    <CompanyTable page/>
</PageFrame>
```

Use slots to replace repeated shell markup while preserving explicit semantic regions. Avoid a
single component with many optional booleans and strings.

---

## 10. Attribute spreading

Source: [Leptos spread example](https://github.com/leptos-rs/leptos/tree/main/examples/spread)

Spreading allows reusable primitives to accept normal HTML attributes without defining one prop per
attribute:

```rust
let accessibility = view! {
    <{..}
        aria-describedby="company-help"
        autocomplete="organization"
    />
};

view! {
    <input name="name" required {..accessibility}/>
}
```

Attributes passed after `{..}` on a component are forwarded to its top-level element or elements:

```rust
<ButtonLink
    href="/companies"
    {..}
    aria-label="View companies"
    data-testid="companies-link"
/>
```

PayPlan uses:

- accessible button and link variants;
- form controls with reusable ARIA/data attributes;
- table and panel primitives;
- test IDs without adding dedicated component props.

Rules:

- components intended for spreading should render one top-level interactive element;
- do not spread the same event twice;
- browser event handlers belong only inside islands;
- prefer explicit props for business meaning and spreading for native HTML concerns.

---

## 11. Reactive stores

Source: [Leptos stores example](https://github.com/leptos-rs/leptos/tree/main/examples/stores)

Reactive stores provide field-level reactivity for nested client state:

```rust
#[derive(Store, Patch, Clone, Serialize, Deserialize)]
struct PackageDraft {
    name: String,
    #[store(key: Uuid = |item| item.id)]
    items: Vec<PackageItemDraft>,
}

#[derive(Store, Patch, Clone, Serialize, Deserialize)]
struct PackageItemDraft {
    id: Uuid,
    quantity: u32,
    enabled: bool,
}

#[island]
fn PackageBuilder(initial: PackageDraft) -> impl IntoView {
    let draft = Store::new(initial);

    view! {
        <For
            each=move || draft.items()
            key=|row| row.id().get()
            let:item
        >
            <PackageItemEditor item/>
        </For>
    }
}
```

Do not add `reactive_stores` for the mobile menu, a modal, or a two-field form. It is justified when
an island has nested collections, multiple independently updated fields, computed state, undo, or
multi-step editing.

The likely first candidate is a package or billing-plan builder. It should be a single bounded
island receiving a compact initial DTO, not the whole admin page.

---

## 12. WebSocket server functions

Source: [Leptos websocket example](https://github.com/leptos-rs/leptos/tree/main/examples/websocket)

The stable `server_fn 0.8.12` used by this workspace supports Axum WebSocket upgrades when its Axum
feature is active.

Server shape:

```rust
#[server(protocol = Websocket<JsonEncoding, JsonEncoding>)]
async fn watch_job(
    input: BoxedStream<JobWatchCommand, ServerFnError>,
) -> Result<BoxedStream<JobProgress, ServerFnError>, ServerFnError> {
    let auth = current_ui_auth()?;
    let jobs = expect_context::<AppContext>().admin_jobs.clone();

    jobs
        .watch_authorized_job(auth, input)
        .await
        .map(Into::into)
        .map_err(|error| ServerFnError::ServerError(error.to_string()))
}
```

Client behavior must live in an island and start only in the browser:

```rust
#[island]
fn JobProgressPanel(job_id: Uuid) -> impl IntoView {
    let latest = RwSignal::new(None::<JobProgress>);

    if cfg!(feature = "hydrate") {
        spawn_local(async move {
            // Open the typed stream, process messages, and update `latest`.
        });
    }

    view! {
        <output>{move || latest.get().map(|item| item.message)}</output>
    }
}
```

PayPlan use:

- renewal job progress;
- royal-pot distribution progress;
- binary-cycle close progress;
- operation logs and completion notifications.

Required production behavior:

- verify `Origin` before the upgrade;
- authorize the requested job and tenant;
- bounded channels and message sizes;
- heartbeat and idle timeout;
- client reconnect with capped exponential backoff;
- cancellation on island cleanup;
- resumable status by job ID after reconnect;
- no secrets or raw internal errors in messages;
- polling fallback and an SSR status page.

Do not introduce WebSockets for data that changes only after user navigation.

---

## 13. Subsecond hot patching

Source:
[Leptos subsecond_hot_patch example](https://github.com/leptos-rs/leptos/tree/main/examples/subsecond_hot_patch)

The example is explicitly experimental and uses the Dioxus CLI rather than cargo-leptos. It
hot-patches reactive view functions, can reset signal state, and does not represent a production
runtime optimization.

Decision:

- do not add `subsecond` to normal workspace dependencies;
- do not replace cargo-leptos development or release commands;
- optionally evaluate it in an isolated developer-only branch;
- never make CI or production builds depend on it.

The existing cargo-leptos watch and hot-reload flow remains the supported workflow.

---

## 14. Proposed PayPlan implementation plan

### Target folder structure

Refactor toward this structure before adding more browser behavior:

```text
crates/payplan_ui/
├── src/
│   ├── lib.rs
│   ├── shell.rs
│   ├── app/
│   │   ├── mod.rs
│   │   ├── router.rs
│   │   ├── request.rs
│   │   └── auth.rs
│   ├── components/
│   │   ├── mod.rs
│   │   ├── page_frame.rs
│   │   ├── pagination.rs
│   │   ├── search_form.rs
│   │   ├── tables.rs
│   │   └── feedback.rs
│   ├── pages/
│   │   ├── mod.rs
│   │   ├── dashboard.rs
│   │   ├── packages.rs
│   │   ├── companies.rs
│   │   ├── catalog.rs
│   │   ├── billing.rs
│   │   ├── purchases.rs
│   │   ├── users.rs
│   │   └── jobs.rs
│   ├── islands/
│   │   ├── mod.rs
│   │   ├── mobile_nav.rs
│   │   ├── confirm_dialog.rs
│   │   └── job_progress.rs
│   └── server_functions/
│       ├── mod.rs
│       └── jobs.rs
└── style/
    └── tailwind.css
```

Compilation ownership:

- `app`, `components`, and `pages` remain `#[cfg(feature = "ssr")]`;
- `islands` compiles for SSR and hydrate;
- `server_functions` contains shared DTOs and generated client stubs, but server-only dependencies
  stay inside generated server implementations;
- a future lazy-routed designer should use a separate crate or entrypoint instead of weakening this
  boundary.

### Dependency changes by phase

Do not add every example dependency up front.

| Phase | Dependency change |
|---|---|
| Slots, spread, SSR modes | None |
| Islands-router pilot | Feature forwarding only |
| Portal dialog | None; `Portal` is provided by `leptos` |
| Typed HTTP server functions | Usually none beyond existing Leptos server-function support |
| WebSocket server functions | Add direct optional `server_fn` and `futures` dependencies for the APIs imported by `payplan_ui` |
| Reactive editor | Add optional `reactive_stores = 0.4.3` to the hydrate feature |
| Lazy split experiment | No new crate; build with cargo-leptos `--split` |
| Subsecond experiment | Isolated branch only; no normal workspace dependency |

### Phase 0 — Preserve the measured baseline

**Status:** Implemented 2026-06-22.

Record before changing behavior:

- current raw/gzip/Brotli WASM sizes;
- JS bootstrap size;
- number and size of emitted assets;
- dashboard and packages TTFB;
- complete HTML time;
- no-JavaScript screenshots;
- browser navigation timings.

Current WASM baseline:

- raw: 116,098 bytes;
- gzip: 56,038 bytes;
- Brotli: 41,926 bytes.

The machine-readable baseline is stored in
[`docs/baselines/leptos-ui-phase0-2026-06-22.json`](./baselines/leptos-ui-phase0-2026-06-22.json).
Use `make ui-baseline` after a release build and `make ui-route-baseline` against an authenticated
running server. CI uploads `ui-asset-baseline.json` for every release build.

The local browser automation surface rejected localhost navigation during this phase, so a
repository screenshot was not captured. Complete no-JavaScript HTML was instead verified directly
from the release server response; capture the visual screenshot in the next browser-capable CI or
developer run.

Exit criteria: CI stores or prints comparable measurements for every release build.

### Phase 1 — SSR-only composition improvements

**Status:** Implemented 2026-06-22.

1. Split `app.rs` into route, layout, table, form, and page modules.
2. Introduce `PageFrame` and narrow slots for actions and filters.
3. Introduce attribute spreading only on primitives with one top-level element.
4. Keep all new components under the `ssr` feature.
5. Add SSR snapshot or structural tests for repeated page layouts.

Expected bundle effect: zero WASM growth.

Implemented structure:

- `src/app/` owns route registration and request-derived context;
- `src/components/` owns the shell, `PageFrame`, slots, forms, tables, pagination, and feedback;
- `src/pages/` owns dashboard, login, list, and jobs pages;
- `PageFrame` provides narrow action and filter slots;
- search-input ARIA and autocomplete attributes demonstrate constrained native-element spreading;
- structural tests cover navigation scope/uniqueness and pagination boundaries.

Measured result:

- WASM remained **116,098 raw / 56,038 gzip / 41,926 Brotli bytes**;
- bootstrap JavaScript remained **12,833 raw / 4,683 gzip / 3,783 Brotli bytes**;
- total generated asset bytes decreased from **257,814** to **257,092**;
- Tailwind source detection is restricted to `crates/payplan_ui/src/**/*.rs`, preventing guide and
  documentation examples from changing production CSS;
- authenticated release HTML for `/packages` contained the heading, search attributes, table,
  pagination, and exactly one island in the initial response.

Exit criteria: pages render the same HTML semantics, and hydrate-only dependency checks remain
unchanged.

### Phase 2 — SSR-mode measurement

1. Keep `Async` as the control.
2. Benchmark dashboard, packages, and companies with `InOrder`.
3. Pilot `PartiallyBlocked` on one route only if it preserves complete no-JS HTML.
4. Keep login and synchronous error pages on the default mode.
5. Document the selected mode next to each route group.

Exit criteria: every mode change has measured TTFB/complete-HTML improvement and no regression in
no-JavaScript behavior.

### Phase 3 — Islands-router feature pilot

1. Add opt-in `islands-router` Cargo feature forwarding.
2. Enable `islands_router=true` in `HydrationScripts` only under that feature.
3. Add browser tests for links, GET search, pagination, create actions, logout, auth expiry,
   redirects, back/forward, title, focus, and scroll.
4. Mark downloads and non-document links as external.
5. Compare WASM, JS, transferred HTML, and navigation latency.
6. Run the full suite with the feature disabled and enabled.

Exit criteria: all protected actions and navigation paths behave correctly, accessibility checks
pass, and the measured UX improvement justifies the additional client script.

### Phase 4 — First modal island

1. Implement one accessible confirmation dialog for a destructive or long-running operation.
2. Prefer native `<dialog>`; use `Portal` only if clipping or stacking requires it.
3. Preserve the normal POST form as the no-JavaScript fallback.
4. Test keyboard focus, Escape, cancel, submit, and islands-router navigation cleanup.

Exit criteria: one bounded island, no page-shell hydration, and measured bundle growth within the
approved per-island budget.

### Phase 5 — Typed server functions for island-owned behavior

1. Add a shared `server_functions` module with compact serializable DTOs.
2. Keep initial page reads in SSR query services.
3. Add cookie auth and authorization tests for `/_server`.
4. Add origin validation for mutations.
5. Start with read-only job status or autosuggest.

Exit criteria: `/api/*` remains bearer-only, `/_server/*` is cookie-authenticated, and no SQLx or
Axum implementation type enters the hydrate tree.

### Phase 6 — Live jobs

1. Persist job execution/status independently of any WebSocket connection.
2. Add a polling status endpoint and SSR job detail page.
3. Add a `JobProgressPanel` island.
4. Add WebSocket streaming with origin checks, bounded channels, reconnect, and resume.
5. Exercise disconnect and server-restart scenarios.

Exit criteria: losing the socket does not lose the job, and users can recover status through SSR or
polling.

### Phase 7 — Complex editor and reactive store

Only begin this phase when a concrete editor requires nested client state.

1. Define a small serializable draft DTO.
2. Add `reactive_stores` only to the hydrate feature.
3. Build one package or billing-plan editor island.
4. Keep validation and final authorization on the server.
5. Add dirty-state, reset, submit, conflict, and no-JavaScript fallback behavior.

Exit criteria: field updates are localized, bundle growth is measured, and the editor does not turn
the surrounding route into a hydrated app.

### Phase 8 — Lazy-loading decision gate

First test `#[lazy]` inside a demonstrably large island:

1. identify a client-only code path large enough to split;
2. build release assets with and without `--split`;
3. compare base WASM, deferred chunks, total bytes, request count, cache behavior, and interaction
   latency;
4. keep the split only if first-load and repeat-load results are better.

Do not implement `LazyRoute` in the existing SSR route tree.

If a future designer becomes SPA-like, create a separate hydrated application boundary and write a
new architecture decision record before implementation.

### Phase 9 — Development tooling experiment

Evaluate subsecond hot patching only after product features are complete. Keep the experiment
outside standard dependencies, CI, and deployment.

---

## 15. Verification matrix

Required commands after each phase:

```bash
rtk cargo fmt --all -- --check
rtk cargo clippy --workspace --all-targets --all-features -- -D warnings
rtk cargo test --workspace -- --test-threads=1
rtk cargo test -p payplan_infra --features integration -- \
  --include-ignored --test-threads=1
rtk cargo leptos build --release --precompress
```

For split experiments:

```bash
rtk cargo leptos build --release --precompress --split
```

Hydrate dependency boundary:

```bash
rtk cargo tree -p payplan_ui \
  --target wasm32-unknown-unknown \
  --no-default-features \
  --features hydrate
```

The hydrate tree must not contain:

- `axum`;
- `sqlx`;
- `tokio`;
- `payplan_infra`;
- `payplan_web`;
- `leptos_axum`;
- route-level admin query implementations.

Browser matrix:

- JavaScript enabled and disabled;
- islands router enabled and disabled;
- direct load and client navigation;
- valid, expired, and revoked sessions;
- POST success and validation failure;
- back and forward;
- keyboard-only operation;
- mobile and desktop widths;
- offline/deferred-chunk failure for lazy experiments;
- WebSocket disconnect/reconnect for live jobs.

---

## 16. Final adoption order

Implement in this order:

1. slots and component spreading;
2. SSR-mode benchmarks;
3. islands-router pilot;
4. one accessible modal island;
5. targeted server functions;
6. live job WebSocket island;
7. reactive store only for a real complex editor;
8. lazy function experiment only after a large island exists;
9. lazy routes only in a separate SPA boundary;
10. subsecond hot patching only as optional developer research.

This order preserves the primary optimization: server-render as much as possible and avoid compiling
page code into WASM at all.

---

## Upstream examples reviewed

- [islands_router](https://github.com/leptos-rs/leptos/tree/main/examples/islands_router)
- [islands](https://github.com/leptos-rs/leptos/tree/main/examples/islands)
- [lazy_routes](https://github.com/leptos-rs/leptos/tree/main/examples/lazy_routes)
- [portal](https://github.com/leptos-rs/leptos/tree/main/examples/portal)
- [slots](https://github.com/leptos-rs/leptos/tree/main/examples/slots)
- [spread](https://github.com/leptos-rs/leptos/tree/main/examples/spread)
- [ssr_modes](https://github.com/leptos-rs/leptos/tree/main/examples/ssr_modes)
- [stores](https://github.com/leptos-rs/leptos/tree/main/examples/stores)
- [subsecond_hot_patch](https://github.com/leptos-rs/leptos/tree/main/examples/subsecond_hot_patch)
- [websocket](https://github.com/leptos-rs/leptos/tree/main/examples/websocket)
