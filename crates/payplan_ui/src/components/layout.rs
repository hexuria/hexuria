use leptos::prelude::*;

use crate::islands::{MobileNavToggle, ThemeToggle};

pub(crate) const ADMIN_NAV_ITEMS: [(&str, &str); 8] = [
    ("/dashboard", "Dashboard"),
    ("/packages", "Packages"),
    ("/companies", "Companies"),
    ("/catalog", "Catalog"),
    ("/billing", "Billing"),
    ("/purchases", "Purchases"),
    ("/users", "Users"),
    ("/jobs", "Jobs"),
];

#[component]
pub(crate) fn AdminShell(children: Children) -> impl IntoView {
    view! {
        <div class="admin-layout">
            <aside class="sidebar">
                <div class="flex items-center justify-between pb-2 border-b border-border/20">
                    <div class="brand">"PayPlan"</div>
                    <ThemeToggle/>
                </div>
                <nav aria-label="Administration">
                    {ADMIN_NAV_ITEMS
                        .into_iter()
                        .map(|(href, label)| view! { <a href=href>{label}</a> })
                        .collect_view()}
                </nav>
                <form method="post" action="/logout" class="mt-auto">
                    <button type="submit" class="w-full text-left py-2 px-3 hover:bg-white/5 rounded-md text-[#d7e9df] hover:text-white transition-all cursor-pointer">
                        "Sign out"
                    </button>
                </form>
            </aside>
            <div class="content">
                <MobileNavToggle/>
                {children()}
            </div>
        </div>
    }
}

#[slot]
pub(crate) struct PageActions {
    children: ChildrenFn,
}

#[slot]
pub(crate) struct PageFilters {
    children: ChildrenFn,
}

#[component]
pub(crate) fn PageFrame(
    title: &'static str,
    #[prop(optional)] eyebrow: Option<&'static str>,
    #[prop(optional)] actions: Option<PageActions>,
    #[prop(optional)] filters: Option<PageFilters>,
    children: Children,
) -> impl IntoView {
    view! {
        <AdminShell>
            <header class="page-heading">
                <div>
                    {eyebrow.map(|eyebrow| view! { <p class="eyebrow">{eyebrow}</p> })}
                    <h1>{title}</h1>
                </div>
                {actions.map(|slot| (slot.children)())}
            </header>
            {filters.map(|slot| (slot.children)())}
            {children()}
        </AdminShell>
    }
}

#[cfg(test)]
mod tests {
    use super::ADMIN_NAV_ITEMS;
    use std::collections::HashSet;

    #[test]
    fn admin_navigation_paths_are_unique() {
        let unique = ADMIN_NAV_ITEMS
            .iter()
            .map(|(path, _)| *path)
            .collect::<HashSet<_>>();

        assert_eq!(unique.len(), ADMIN_NAV_ITEMS.len());
    }

    #[test]
    fn admin_navigation_paths_are_absolute() {
        assert!(ADMIN_NAV_ITEMS
            .iter()
            .all(|(path, _)| path.starts_with('/')));
    }
}
