use leptos::prelude::*;
use payplan_core::platform::user::UserRole;
use payplan_web::AppContext;

use crate::{
    app::{current_user, page_request},
    components::{
        BillingContent, CatalogContent, Forbidden, LoadError, Loading,
        LoginRequired, PackageContent, PageFilters, PageFrame, PurchaseContent, SearchForm,
        UserContent,
    },
};

macro_rules! list_page {
    ($name:ident, $title:literal, $method:ident, $content:ident) => {
        #[component]
        pub(crate) fn $name() -> impl IntoView {
            let Some(_auth) = current_user() else {
                return view! { <LoginRequired/> }.into_any();
            };
            let context = expect_context::<AppContext>();
            let service = context.admin_queries.clone();
            let request = page_request();
            let data = Resource::new_blocking(
                || (),
                move |_| {
                    let service = service.clone();
                    let request = request.clone();
                    async move {
                        service
                            .$method(request)
                            .await
                            .map_err(|_| concat!(stringify!($method), " query failed").to_string())
                    }
                },
            );
            view! {
                <PageFrame title=$title>
                    <PageFilters slot:filters>
                        <SearchForm/>
                    </PageFilters>
                    <Suspense fallback=|| view! { <Loading/> }>
                        {move || Suspend::new(async move {
                            match data.await {
                                Ok(page) => view! { <$content page/> }.into_any(),
                                Err(_) => view! { <LoadError/> }.into_any(),
                            }
                        })}
                    </Suspense>
                </PageFrame>
            }
            .into_any()
        }
    };
}

list_page!(PackagesPage, "Packages", packages, PackageContent);
list_page!(CatalogPage, "Catalog", catalog, CatalogContent);
list_page!(BillingPage, "Billing plans", billing, BillingContent);
list_page!(PurchasesPage, "Purchases", purchases, PurchaseContent);
list_page!(UsersPage, "Users", users, UserContent);
