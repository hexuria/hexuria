use leptos::prelude::*;

#[island]
pub fn ThemeToggle() -> impl IntoView {
    let is_dark = RwSignal::new(false);

    // Initialize state on mount in the browser
    Effect::new(move |_| {
        #[cfg(feature = "hydrate")]
        {
            let doc = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.document_element());
            if let Some(el) = doc {
                let dark = el.class_list().contains("dark");
                is_dark.set(dark);
            }
        }
    });

    let toggle_theme = move |_| {
        #[cfg(feature = "hydrate")]
        {
            let doc = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.document_element());
            if let Some(el) = doc {
                let new_dark = !is_dark.get();
                if new_dark {
                    let _ = el.class_list().add_1("dark");
                    let _ = web_sys::window()
                        .and_then(|w| w.local_storage().ok().flatten())
                        .map(|s| s.set_item("theme", "dark"));
                } else {
                    let _ = el.class_list().remove_1("dark");
                    let _ = web_sys::window()
                        .and_then(|w| w.local_storage().ok().flatten())
                        .map(|s| s.set_item("theme", "light"));
                }
                is_dark.set(new_dark);
            }
        }
    };

    view! {
        <button
            type="button"
            class="theme-toggle inline-flex items-center justify-center p-2 rounded-lg border border-border hover:bg-paper/50 transition-colors text-ink cursor-pointer"
            aria-label="Toggle theme"
            on:click=toggle_theme
        >
            {move || if is_dark.get() {
                // Moon/Dark icon
                view! {
                    <svg class="w-5 h-5 text-yellow-400" fill="currentColor" viewBox="0 0 20 20" xmlns="http://www.w3.org/2000/svg">
                        <path d="M17.293 13.293A8 8 0 016.707 2.707a8.001 8.001 0 1010.586 10.586z"></path>
                    </svg>
                }.into_any()
            } else {
                // Sun/Light icon
                view! {
                    <svg class="w-5 h-5 text-[#196c4a]" fill="currentColor" viewBox="0 0 20 20" xmlns="http://www.w3.org/2000/svg">
                        <path fill-rule="evenodd" d="M10 2a1 1 0 011 1v1a1 1 0 11-2 0V3a1 1 0 011-1zm4 8a4 4 0 11-8 0 4 4 0 018 0zm-.464 4.95l.707.707a1 1 0 001.414-1.414l-.707-.707a1 1 0 00-1.414 1.414zm2.12-10.607a1 1 0 010 1.414l-.706.707a1 1 0 11-1.414-1.414l.707-.707a1 1 0 011.414 0zM17 11a1 1 0 100-2h-1a1 1 0 100 2h1zm-7 4a1 1 0 011 1v1a1 1 0 11-2 0v-1a1 1 0 011-1zM5.05 6.464A1 1 0 106.46 5.05l-.707-.707a1 1 0 00-1.414 1.414l.707.707zm1.414 8.486l-.707.707a1 1 0 01-1.414-1.414l.707-.707a1 1 0 011.414 1.414zM4 11a1 1 0 100-2H3a1 1 0 000 2h1z" clip-rule="evenodd"></path>
                    </svg>
                }.into_any()
            }}
        </button>
    }
}
