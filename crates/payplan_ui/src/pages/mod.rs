mod dashboard;
mod jobs;
mod lists;
mod login;

pub(crate) use dashboard::DashboardPage;
pub(crate) use jobs::JobsPage;
pub(crate) use lists::{
    BillingPage, CatalogPage, CompaniesPage, PackagesPage, PurchasesPage, UsersPage,
};
pub(crate) use login::LoginPage;
