use leptos::prelude::*;

/// Browser state is required to open and close the compact navigation menu.
#[island]
pub fn MobileNavToggle() -> impl IntoView {
    let open = RwSignal::new(false);

    view! {
        <button
            type="button"
            class="mobile-nav-toggle"
            aria-expanded=move || open.get().to_string()
            on:click=move |_| open.update(|value| *value = !*value)
        >
            {move || if open.get() { "Close menu" } else { "Open menu" }}
        </button>
    }
}
