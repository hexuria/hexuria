use axum::http::{header::CONTENT_SECURITY_POLICY, HeaderValue};
use leptos::prelude::*;
use leptos_axum::ResponseOptions;
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};

use crate::app::App;

pub fn shell(options: LeptosOptions) -> impl IntoView {
    provide_meta_context();
    let stylesheet = stylesheet_href(&options);
    let nonce = leptos::nonce::use_nonce();
    if let (Some(ref nonce_str), Some(response)) =
        (nonce.clone(), use_context::<ResponseOptions>())
    {
        let policy = format!(
            "default-src 'self'; script-src 'self' 'nonce-{nonce_str}' 'wasm-unsafe-eval'; \
             style-src 'self'; img-src 'self' data:; object-src 'none'; \
             base-uri 'self'; frame-ancestors 'none'"
        );
        if let Ok(value) = HeaderValue::from_str(&policy) {
            response.insert_header(CONTENT_SECURITY_POLICY, value);
        }
    }

    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <script nonce=nonce>
                    "if (localStorage.theme === 'dark' || (!('theme' in localStorage) && window.matchMedia('(prefers-color-scheme: dark)').matches)) { \
                        document.documentElement.classList.add('dark'); \
                     } else { \
                        document.documentElement.classList.remove('dark'); \
                     }"
                </script>
                <AutoReload options=options.clone()/>
                <HydrationScripts options=options islands=true/>
                <Stylesheet id="payplan-ui" href=stylesheet/>
                <Title text="PayPlan Administration"/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

fn stylesheet_href(options: &LeptosOptions) -> String {
    let output = options.output_name.as_ref();
    let pkg = options.site_pkg_dir.as_ref();
    if !options.hash_files {
        return format!("/{pkg}/{output}.css");
    }
    let hash = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.parent()
                .map(|parent| parent.join(options.hash_file.as_ref()))
        })
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|contents| {
            contents.lines().find_map(|line| {
                line.strip_prefix("css:")
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            })
        });
    match hash {
        Some(hash) => format!("/{pkg}/{output}.{hash}.css"),
        None => format!("/{pkg}/{output}.css"),
    }
}
