use leptos::prelude::*;
use leptos_router::{
    components::{Route, Router, Routes},
    path, SsrMode,
};

use crate::{
    components::NotFound,
    pages::{
        BillingPage, CatalogPage, DashboardPage, ForgotPasswordPage, JobsPage,
        LandingPage, LoginPage, PackagesPage, PurchasesPage, ResetPasswordPage, UsersPage,
    },
};

#[component]
pub fn App() -> impl IntoView {
    view! {
        <Router>
            <Routes fallback=|| view! { <NotFound/> }>
                <Route path=path!("") view=LandingPage ssr=SsrMode::Async/>
                <Route path=path!("/dashboard") view=DashboardPage ssr=SsrMode::Async/>
                <Route path=path!("/login") view=LoginPage ssr=SsrMode::Async/>
                <Route path=path!("/forgot-password") view=ForgotPasswordPage ssr=SsrMode::Async/>
                <Route path=path!("/reset-password") view=ResetPasswordPage ssr=SsrMode::Async/>
                <Route path=path!("/packages") view=PackagesPage ssr=SsrMode::Async/>
                <Route path=path!("/catalog") view=CatalogPage ssr=SsrMode::Async/>
                <Route path=path!("/billing") view=BillingPage ssr=SsrMode::Async/>
                <Route path=path!("/purchases") view=PurchasesPage ssr=SsrMode::Async/>
                <Route path=path!("/users") view=UsersPage ssr=SsrMode::Async/>
                <Route path=path!("/jobs") view=JobsPage ssr=SsrMode::Async/>
            </Routes>
        </Router>
    }
}
