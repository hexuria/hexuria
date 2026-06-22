use leptos::prelude::*;

use crate::app::request_query;

#[component]
pub(crate) fn SearchForm() -> impl IntoView {
    let query = request_query().query.unwrap_or_default();
    let search_attributes = view! {
        <{..}
            aria-label="Search administration records"
            autocomplete="off"
        />
    };

    view! {
        <form class="search-form" method="get">
            <label>
                <span class="sr-only">"Search"</span>
                <input
                    name="query"
                    type="search"
                    value=query
                    placeholder="Search"
                    {..search_attributes}
                />
            </label>
            <button type="submit">"Search"</button>
        </form>
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct PaginationState {
    last_page: u64,
    previous: u32,
    next: u64,
}

fn pagination_state(page: u32, page_size: u32, total: u64) -> PaginationState {
    let last_page = total.div_ceil(u64::from(page_size)).max(1);
    PaginationState {
        last_page,
        previous: page.saturating_sub(1).max(1),
        next: u64::from(page).saturating_add(1).min(last_page),
    }
}

#[component]
pub(crate) fn Pagination(page: u32, page_size: u32, total: u64) -> impl IntoView {
    let state = pagination_state(page, page_size, total);
    view! {
        <nav class="pagination" aria-label="Pagination">
            <a
                aria-disabled=(page <= 1).to_string()
                href=format!("?page={}&page_size={page_size}", state.previous)
            >
                "Previous"
            </a>
            <span>{format!("Page {page} of {}", state.last_page)}</span>
            <a
                aria-disabled=(u64::from(page) >= state.last_page).to_string()
                href=format!("?page={}&page_size={page_size}", state.next)
            >
                "Next"
            </a>
        </nav>
    }
}

#[cfg(test)]
mod tests {
    use super::{pagination_state, PaginationState};

    #[test]
    fn pagination_clamps_first_page_links() {
        assert_eq!(
            pagination_state(1, 25, 0),
            PaginationState {
                last_page: 1,
                previous: 1,
                next: 1,
            }
        );
    }

    #[test]
    fn pagination_calculates_middle_page_links() {
        assert_eq!(
            pagination_state(2, 25, 75),
            PaginationState {
                last_page: 3,
                previous: 1,
                next: 3,
            }
        );
    }
}
