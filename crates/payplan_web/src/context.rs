use std::sync::Arc;

use payplan_app::commands::default_module_registry;
use payplan_app::ports::{
    CatalogRepo, CompanyRepo, EnrollmentRepo, EntitlementRepo, EventProjector, EventStore,
    ModuleProjector, ModuleStateStore, PackageRepo, PasswordPort, PayPlanStackRepo, PurchaseRepo,
    PurchaseWriter, RevokedJtiStore, RewardLedgerStore, SubscriptionRepo, TokenService, UserRepo,
};
use payplan_core::payplan::registry::ModuleRegistry;
use payplan_infra::aggregate_repos::{
    PgCatalogRepo, PgEnrollmentRepo, PgEntitlementRepo, PgPackageRepo, PgPayPlanStackRepo,
    PgPurchaseRepo, PgSubscriptionRepo,
};
use payplan_infra::auth::{JwtService, PasswordService, PgRevokedJtiStore};
use payplan_infra::event_store::PgEventStore;
use payplan_infra::ledger_store::PgLedgerStore;
use payplan_infra::module_state_store::PgModuleStateStore;
use payplan_infra::projections::{PgEventProjector, PgProjections};
use payplan_infra::purchase_writer::PgPurchaseWriter;
use payplan_infra::repos::{PgCompanyRepo, PgUserRepo};
use sqlx::PgPool;
use tracing::info;

/// Composed application context handed to every handler.
#[derive(Clone)]
pub struct AppContext {
    pub pool: PgPool,
    pub registry: Arc<ModuleRegistry>,
    pub passwords: Arc<dyn PasswordPort>,
    pub companies: Arc<dyn CompanyRepo>,
    pub users: Arc<dyn UserRepo>,
    pub catalog: Arc<dyn CatalogRepo>,
    pub packages: Arc<dyn PackageRepo>,
    pub pay_plan_stacks: Arc<dyn PayPlanStackRepo>,
    pub purchases: Arc<dyn PurchaseRepo>,
    pub subscriptions: Arc<dyn SubscriptionRepo>,
    pub entitlements: Arc<dyn EntitlementRepo>,
    pub enrollments: Arc<dyn EnrollmentRepo>,
    pub events: Arc<dyn EventStore>,
    pub ledger: Arc<dyn RewardLedgerStore>,
    pub purchase_writer: Arc<dyn PurchaseWriter>,
    pub module_state_store: Arc<dyn ModuleStateStore>,
    pub projector: Arc<dyn ModuleProjector>,
    pub event_projector: Arc<dyn EventProjector>,
    pub tokens: Arc<dyn TokenService>,
    pub revoked_jti: Arc<dyn RevokedJtiStore>,
}

impl AppContext {
    /// Build the full set of dependencies against an existing Postgres pool.
    /// `jwt_secret` is the HS256 signing secret; in production it MUST come
    /// from a secure environment source, not be hard-coded.
    #[must_use]
    pub fn new(pool: PgPool, jwt_secret: String) -> Self {
        let registry = Arc::new(default_module_registry());

        let companies: Arc<dyn CompanyRepo> = Arc::new(PgCompanyRepo::new());
        let users: Arc<dyn UserRepo> = Arc::new(PgUserRepo::new());
        let catalog: Arc<dyn CatalogRepo> = Arc::new(PgCatalogRepo::new());
        let packages: Arc<dyn PackageRepo> = Arc::new(PgPackageRepo::new());
        let pay_plan_stacks: Arc<dyn PayPlanStackRepo> = Arc::new(PgPayPlanStackRepo::new());
        let purchases: Arc<dyn PurchaseRepo> = Arc::new(PgPurchaseRepo::new());
        let subscriptions: Arc<dyn SubscriptionRepo> = Arc::new(PgSubscriptionRepo::new());
        let entitlements: Arc<dyn EntitlementRepo> = Arc::new(PgEntitlementRepo::new());
        let enrollments: Arc<dyn EnrollmentRepo> = Arc::new(PgEnrollmentRepo::new());
        let events: Arc<dyn EventStore> = Arc::new(PgEventStore::new(pool.clone()));
        let ledger: Arc<dyn RewardLedgerStore> = Arc::new(PgLedgerStore::new());
        let passwords: Arc<dyn PasswordPort> = Arc::new(PasswordService::new());
        let purchase_writer: Arc<dyn PurchaseWriter> =
            Arc::new(PgPurchaseWriter::new(pool.clone()));
        let module_state_store: Arc<dyn ModuleStateStore> =
            Arc::new(PgModuleStateStore::new(pool.clone()));
        let projector: Arc<dyn ModuleProjector> = Arc::new(PgProjections::new());
        let event_projector: Arc<dyn EventProjector> = Arc::new(PgEventProjector::new());
        let tokens: Arc<dyn TokenService> = Arc::new(JwtService::new(&jwt_secret));
        let revoked_jti: Arc<dyn RevokedJtiStore> = Arc::new(PgRevokedJtiStore::new(pool.clone()));

        info!("AppContext built with all Postgres repos + atomic purchase writer + persistent module state + relational + event projections + JWT auth");

        Self {
            pool,
            registry,
            passwords,
            companies,
            users,
            catalog,
            packages,
            pay_plan_stacks,
            purchases,
            subscriptions,
            entitlements,
            enrollments,
            events,
            ledger,
            purchase_writer,
            module_state_store,
            projector,
            event_projector,
            tokens,
            revoked_jti,
        }
    }

    /// Dev/default JWT secret. Available only in debug builds so the hardcoded
    /// fallback can never be compiled into a release binary.
    #[cfg(debug_assertions)]
    #[must_use]
    pub fn dev_jwt_secret() -> String {
        std::env::var("JWT_SECRET").unwrap_or_else(|_| "dev-secret-change-me".into())
    }
}
