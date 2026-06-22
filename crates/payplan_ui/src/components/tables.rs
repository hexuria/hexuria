use leptos::prelude::*;
use payplan_app::queries::{
    BillingRow, CatalogRow, CompanyRow, PackageRow, Page, PurchaseRow, UserRow,
};

use super::{CreateBillingForm, CreateCatalogForm, CreateCompanyForm, Pagination};

#[component]
pub(crate) fn PackageContent(page: Page<PackageRow>) -> impl IntoView {
    view! {
        <div class="panel table-wrap">
            <table>
                <thead><tr><th>"Name"</th><th>"Status"</th><th>"Items"</th><th>"Created"</th></tr></thead>
                <tbody>
                    {page.items.into_iter().map(|row| view! {
                        <tr>
                            <td>{row.name}</td><td>{row.status}</td>
                            <td>{row.item_count}</td><td>{date(row.created_at)}</td>
                        </tr>
                    }).collect_view()}
                </tbody>
            </table>
            <Pagination page=page.page page_size=page.page_size total=page.total_items/>
        </div>
    }
}

#[component]
pub(crate) fn CompanyContent(page: Page<CompanyRow>) -> impl IntoView {
    view! {
        <CreateCompanyForm/>
        <div class="panel table-wrap">
            <table>
                <thead><tr><th>"Company"</th><th>"Slug"</th><th>"Status"</th><th>"Created"</th></tr></thead>
                <tbody>{page.items.into_iter().map(|row| view! {
                    <tr><td>{row.name}</td><td>{row.slug}</td><td>{row.status}</td><td>{date(row.created_at)}</td></tr>
                }).collect_view()}</tbody>
            </table>
            <Pagination page=page.page page_size=page.page_size total=page.total_items/>
        </div>
    }
}

#[component]
pub(crate) fn CatalogContent(page: Page<CatalogRow>) -> impl IntoView {
    view! {
        <CreateCatalogForm/>
        <div class="panel table-wrap">
            <table>
                <thead><tr><th>"Item"</th><th>"Type"</th><th>"SKU"</th><th>"Status"</th></tr></thead>
                <tbody>{page.items.into_iter().map(|row| view! {
                    <tr><td>{row.name}</td><td>{row.item_type}</td><td>{row.sku.unwrap_or_default()}</td><td>{row.status}</td></tr>
                }).collect_view()}</tbody>
            </table>
            <Pagination page=page.page page_size=page.page_size total=page.total_items/>
        </div>
    }
}

#[component]
pub(crate) fn BillingContent(page: Page<BillingRow>) -> impl IntoView {
    view! {
        <CreateBillingForm/>
        <div class="panel table-wrap">
            <table>
                <thead><tr><th>"Catalog item"</th><th>"Type"</th><th>"Price"</th><th>"Active"</th></tr></thead>
                <tbody>{page.items.into_iter().map(|row| view! {
                    <tr><td>{row.catalog_item_name}</td><td>{row.billing_type}</td><td>{format!("{} {}", row.price, row.currency)}</td><td>{if row.active { "Yes" } else { "No" }}</td></tr>
                }).collect_view()}</tbody>
            </table>
            <Pagination page=page.page page_size=page.page_size total=page.total_items/>
        </div>
    }
}

#[component]
pub(crate) fn PurchaseContent(page: Page<PurchaseRow>) -> impl IntoView {
    view! {
        <div class="panel table-wrap">
            <PurchaseTable items=page.items/>
            <Pagination page=page.page page_size=page.page_size total=page.total_items/>
        </div>
    }
}

#[component]
pub(crate) fn PurchaseTable(items: Vec<PurchaseRow>) -> impl IntoView {
    view! {
        <table>
            <thead><tr><th>"Package"</th><th>"Amount"</th><th>"Status"</th><th>"Purchased"</th></tr></thead>
            <tbody>{items.into_iter().map(|row| view! {
                <tr><td>{row.package_name}</td><td>{format!("{} {}", row.amount, row.currency)}</td><td>{row.status}</td><td>{date(row.purchased_at)}</td></tr>
            }).collect_view()}</tbody>
        </table>
    }
}

#[component]
pub(crate) fn UserContent(page: Page<UserRow>) -> impl IntoView {
    view! {
        <div class="panel table-wrap">
            <table>
                <thead><tr><th>"Email"</th><th>"Role"</th><th>"Verified"</th><th>"Created"</th></tr></thead>
                <tbody>{page.items.into_iter().map(|row| view! {
                    <tr><td>{row.email}</td><td>{row.role}</td><td>{if row.email_verified { "Yes" } else { "No" }}</td><td>{date(row.created_at)}</td></tr>
                }).collect_view()}</tbody>
            </table>
            <Pagination page=page.page page_size=page.page_size total=page.total_items/>
        </div>
    }
}

fn date(value: chrono::DateTime<chrono::Utc>) -> String {
    value.format("%Y-%m-%d").to_string()
}
