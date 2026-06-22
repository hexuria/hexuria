use leptos::prelude::*;
use payplan_app::queries::DashboardView;
use payplan_web::AppContext;

use crate::{
    app::{current_user, scope},
    components::{LoadError, Loading, LoginRequired, PageFrame, PurchaseTable},
};

#[component]
pub(crate) fn DashboardPage() -> impl IntoView {
    let Some(auth) = current_user() else {
        return view! { <LoginRequired/> }.into_any();
    };
    let context = expect_context::<AppContext>();
    let query = context.admin_queries.clone();
    let tenant = scope(&auth);
    let data = Resource::new_blocking(
        || (),
        move |_| {
            let query = query.clone();
            async move {
                query
                    .dashboard(tenant)
                    .await
                    .map_err(|_| "dashboard query failed".to_string())
            }
        },
    );

    view! {
        <PageFrame title="Dashboard" eyebrow="Overview">
            <Suspense fallback=|| view! { <Loading/> }>
                {move || Suspend::new(async move {
                    match data.await {
                        Ok(data) => view! { <DashboardContent data/> }.into_any(),
                        Err(_) => view! { <LoadError/> }.into_any(),
                    }
                })}
            </Suspense>
        </PageFrame>
    }
    .into_any()
}

#[component]
fn DashboardContent(data: DashboardView) -> impl IntoView {
    view! {
        <section class="stat-grid">
            <StatCard label="Companies" value=data.company_count/>
            <StatCard label="Users" value=data.user_count/>
            <StatCard label="Packages" value=data.package_count/>
            <StatCard label="Purchases" value=data.purchase_count/>
        </section>
        <section class="panel">
            <h2>"Recent purchases"</h2>
            <PurchaseTable items=data.recent_purchases/>
        </section>
    }
}

#[component]
fn StatCard(label: &'static str, value: u64) -> impl IntoView {
    view! {
        <article class="stat-card">
            <span>{label}</span>
            <strong>{value}</strong>
        </article>
    }
}
