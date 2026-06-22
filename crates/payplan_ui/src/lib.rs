pub mod islands;

#[cfg(feature = "ssr")]
pub mod app;
#[cfg(feature = "ssr")]
mod components;
#[cfg(feature = "ssr")]
mod pages;
#[cfg(feature = "ssr")]
pub mod shell;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    #[allow(unused_imports)]
    use crate::islands::*;

    console_error_panic_hook::set_once();
    leptos::mount::hydrate_islands();
}
