use leptos::prelude::*;

use crate::app::request_query;

#[component]
pub(crate) fn LoginPage() -> impl IntoView {
    let query = request_query();
    let error = query.error.is_some();
    let next = query.next.unwrap_or_else(|| "/".into());
    view! {
        <main class="login-shell">
            <p class="eyebrow">"PayPlan administration"</p>
            <h1>"Sign in"</h1>
            {error.then(|| view! { <p class="error-banner">"Invalid email or password."</p> })}
            <form method="post" action="/login">
                <input name="next" type="hidden" value=next/>
                <label>
                    "Email"
                    <input name="email" type="email" autocomplete="email" required/>
                </label>
                <label>
                    "Password"
                    <input
                        name="password"
                        type="password"
                        autocomplete="current-password"
                        required
                    />
                </label>
                <button type="submit">"Sign in"</button>
            </form>
        </main>
    }
}
