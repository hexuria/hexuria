use leptos::prelude::*;

use crate::app::current_user;

#[component]
pub(crate) fn CreateCatalogForm() -> impl IntoView {
    let can_create = current_user()
        .map(|auth| auth.is_admin())
        .unwrap_or(false);
    can_create.then(|| {
        view! {
            <details class="panel create-panel">
                <summary>"Create catalog item"</summary>
                <form method="post" action="/catalog">
                    <label>"Name"<input name="name" required/></label>
                    <label>"Description"<input name="description"/></label>
                    <label>"SKU"<input name="sku"/></label>
                    <label>
                        "Type"
                        <select name="item_type">
                            <option value="product">"Product"</option>
                            <option value="service">"Service"</option>
                        </select>
                    </label>
                    <button type="submit">"Create item"</button>
                </form>
            </details>
        }
    })
}

#[component]
pub(crate) fn CreateBillingForm() -> impl IntoView {
    let can_create = current_user()
        .map(|auth| auth.is_admin())
        .unwrap_or(false);
    can_create.then(|| view! {
        <details class="panel create-panel">
            <summary>"Create billing plan"</summary>
            <form method="post" action="/billing">
                <label>"Catalog item ID"<input name="catalog_item_id" type="text" required/></label>
                <label>
                    "Billing type"
                    <select name="billing_type">
                        <option value="one_time">"One-time"</option>
                        <option value="recurring">"Recurring"</option>
                    </select>
                </label>
                <label>"Amount"<input name="price_amount" inputmode="decimal" required/></label>
                <label>"Currency"<input name="currency" value="USD" maxlength="3" required/></label>
                <label>
                    "Recurrence"
                    <select name="recurrence_interval">
                        <option value="monthly">"Monthly"</option>
                        <option value="weekly">"Weekly"</option>
                        <option value="quarterly">"Quarterly"</option>
                        <option value="yearly">"Yearly"</option>
                    </select>
                </label>
                <button type="submit">"Create plan"</button>
            </form>
        </details>
    })
}

#[component]
pub(crate) fn JobForm(action: &'static str, label: &'static str) -> impl IntoView {
    view! {
        <form class="panel" method="post" action=action>
            <h2>{label}</h2>
            <p>
                "This operation changes production compensation state. Review the result after running."
            </p>
            <button type="submit">{label}</button>
        </form>
    }
}
