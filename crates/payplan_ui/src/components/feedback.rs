use leptos::prelude::*;

use super::AdminShell;

#[component]
pub(crate) fn Loading() -> impl IntoView {
    view! { <p class="panel">"Loading…"</p> }
}

#[component]
pub(crate) fn LoadError() -> impl IntoView {
    view! { <p class="error-banner">"The requested data could not be loaded."</p> }
}

#[component]
pub(crate) fn LoginRequired() -> impl IntoView {
    view! {
        <main class="page-shell">
            <h1>"Sign in required"</h1>
            <a href="/login">"Sign in"</a>
        </main>
    }
}

#[component]
pub(crate) fn Forbidden() -> impl IntoView {
    view! {
        <AdminShell>
            <main class="page-heading">
                <h1>"Forbidden"</h1>
                <p>"Your account cannot access this page."</p>
            </main>
        </AdminShell>
    }
}

#[component]
pub(crate) fn NotFound() -> impl IntoView {
    view! {
        <main class="page-shell">
            <h1>"Page not found"</h1>
            <a href="/">"Return to dashboard"</a>
        </main>
    }
}
