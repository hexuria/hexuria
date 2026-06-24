use async_trait::async_trait;
use sqlx::PgConnection;

use payplan_core::payplan::events::DomainEvent;
use payplan_core::payplan::ledger::RewardLedgerEntry;
use payplan_core::payplan::runner::StateChange;
use payplan_core::payplan::stack::PayPlanStack;
use payplan_core::platform::catalog::{BillingPlan, CatalogItem, ProductPayPlanAllocation};
use payplan_core::platform::enrollment::Enrollment;
use payplan_core::platform::entitlement::Entitlement;
use payplan_core::platform::package::Package;
use payplan_core::platform::purchase::Purchase;
use payplan_core::platform::subscription::Subscription;
use payplan_core::platform::user::User;
use payplan_core::shared::ids::{
    BillingPlanId, CatalogItemId, EnrollmentId, LedgerEntryId, PackageId,
    PayPlanStackId, ProductPayPlanAllocationId, PurchaseId, SubscriptionId, UserId,
};

use crate::error::AppResult;

/// Append-only event store. Implementations must guarantee durable persistence
/// and MUST participate in the caller's transaction (`conn`).
#[async_trait]
pub trait EventStore: Send + Sync {
    async fn append(&self, events: &[DomainEvent], conn: &mut PgConnection) -> AppResult<()>;
}

/// Reward ledger store. Entries are append-only and link back to the originating
/// event. MUST participate in the caller's transaction.
#[async_trait]
pub trait RewardLedgerStore: Send + Sync {
    async fn append(
        &self,
        entries: &[RewardLedgerEntry],
        conn: &mut PgConnection,
    ) -> AppResult<Vec<LedgerEntryId>>;
}

/// Password hasher port. Implementations live in `payplan_infra` (argon2).
#[async_trait]
pub trait PasswordPort: Send + Sync {
    async fn hash(&self, plaintext: &str) -> AppResult<String>;
    async fn verify(&self, plaintext: &str, hash: &str) -> AppResult<bool>;
}

/// Distinguishes access (short-lived) from refresh (long-lived) JWTs. Carried
/// as a custom claim so `TokenService::verify` can reject tokens of the wrong
/// kind (e.g. an access token used at the refresh endpoint).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TokenKind {
    Access,
    Refresh,
}

impl TokenKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Access => "access",
            Self::Refresh => "refresh",
        }
    }
}

/// JWT claims for HS256 tokens. `exp`/`iat` are unix seconds (the jsonwebtoken
/// crate requires `usize`). `jti` is the unique token id used for revocation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenClaims {
    /// Subject: the user id.
    pub sub: uuid::Uuid,
    /// `"user"` | `"admin"`.
    pub role: String,
    /// Unique token id; used as the `revoked_jti` primary key.
    pub jti: String,
    /// Access vs refresh.
    pub kind: TokenKind,
    /// Issuer.
    #[serde(default)]
    pub iss: String,
    /// Audience.
    #[serde(default)]
    pub aud: String,
    /// Expiry (unix seconds).
    pub exp: usize,
    /// Issued-at (unix seconds).
    pub iat: usize,
}

/// Issues and verifies HS256 JWTs. Implementations live in `payplan_infra`.
/// The `verify` method is synchronous because JWT decode is CPU-only.
#[async_trait]
pub trait TokenService: Send + Sync {
    /// Build a short-lived access token (caller supplies the claims shell;
    /// the service sets `kind=Access`, `iat`, and `exp`).
    async fn issue_access(
        &self,
        sub: uuid::Uuid,
        role: &str,
    ) -> AppResult<TokenClaims>;
    /// Build a long-lived refresh token.
    async fn issue_refresh(
        &self,
        sub: uuid::Uuid,
        role: &str,
    ) -> AppResult<TokenClaims>;
    /// Encode a claims shell into a signed token string.
    async fn encode(&self, claims: &TokenClaims) -> AppResult<String>;
    /// Decode + signature/expiry check + kind match. Returns the claims.
    fn verify(&self, token: &str, expected_kind: TokenKind) -> AppResult<TokenClaims>;
}

/// Persists revoked JWT ids so logout / refresh-rotation can invalidate tokens
/// before their natural expiry. MUST participate in the caller's transaction.
#[async_trait]
pub trait RevokedJtiStore: Send + Sync {
    /// Revoke a JTI. Returns `true` if a new row was inserted (token was not
    /// previously revoked), `false` if the JTI was already present (idempotent
    /// `ON CONFLICT DO NOTHING`). Callers use the boolean to detect single-use
    /// refresh-token reuse without a separate `is_revoked` round-trip.
    async fn revoke(
        &self,
        jti: &str,
        user_id: uuid::Uuid,
        kind: TokenKind,
        expires_at: chrono::DateTime<chrono::Utc>,
        conn: &mut PgConnection,
    ) -> AppResult<bool>;
    async fn is_revoked(&self, jti: &str, conn: &mut PgConnection) -> AppResult<bool>;
}

/// Per-aggregate module state read/write port. MUST participate in the caller's
/// transaction (`conn`).
#[async_trait]
pub trait ModuleStateStore: Send + Sync {
    /// Load all state for one aggregate. The returned map is keyed by
    /// `(module_key, module_version)`.
    async fn load_for_aggregate(
        &self,
        aggregate_id: uuid::Uuid,
        conn: &mut PgConnection,
    ) -> AppResult<std::collections::HashMap<(String, String), serde_json::Value>>;

    async fn save(&self, change: ModuleStateChange<'_>, conn: &mut PgConnection) -> AppResult<()>;
}

/// State change descriptor passed to [`ModuleStateStore::save`].
#[derive(Debug, Clone)]
pub struct ModuleStateChange<'a> {
    pub module_key: &'a str,
    pub module_version: &'a str,
    pub aggregate_id: uuid::Uuid,
    pub state: &'a serde_json::Value,
}

/// Projects `module_state` JSON changes into the per-module relational
/// tables (e.g. `royal_flushline_accounts`, `binary_nodes`). MUST participate
/// in the caller's transaction.
#[async_trait]
pub trait ModuleProjector: Send + Sync {
    async fn project(&self, changes: &[StateChange], conn: &mut PgConnection) -> AppResult<()>;
}

/// Projects emitted domain events into relational tables that can't be
/// derived from module state alone (e.g. a `RoyalAccountDuplicated` event
/// materialises a new enrollment + flushline account). MUST participate in
/// the caller's transaction. Distinct from [`ModuleProjector`] which only
/// reacts to per-aggregate state blobs.
#[async_trait]
pub trait EventProjector: Send + Sync {
    async fn project(&self, events: &[DomainEvent], conn: &mut PgConnection) -> AppResult<()>;
}

/// Atomic purchase writer port. Implementations wrap every write (events,
/// ledger, state, projections) inside one DB transaction.
#[async_trait]
pub trait PurchaseWriter: Send + Sync {
    async fn write(&self, writes: PurchaseWrites<'_>) -> AppResult<()>;
}

/// All domain writes the purchase flow needs to persist in a single
/// atomic transaction.
#[derive(Clone)]
pub struct PurchaseWrites<'a> {
    pub subscriptions: &'a [Subscription],
    pub entitlements: &'a [Entitlement],
    pub purchase: &'a Purchase,
    pub enrollment: &'a Enrollment,
    pub events: &'a [DomainEvent],
    pub ledger: &'a [RewardLedgerEntry],
    /// Per-aggregate module state changes produced by the engine cascade.
    pub module_state_changes: &'a [StateChange],
    /// Optional projector for writing per-module relational tables.
    pub projector: Option<&'a dyn ModuleProjector>,
    /// Optional projector for materialising rows from emitted events.
    pub event_projector: Option<&'a dyn EventProjector>,
}

#[async_trait]
pub trait UnitOfWork: Send + Sync {
    async fn commit(self: Box<Self>) -> AppResult<()>;
}

#[async_trait]
pub trait UserRepo: Send + Sync {
    async fn insert(&self, user: &User, conn: &mut PgConnection) -> AppResult<()>;
    async fn get(&self, id: UserId, conn: &mut PgConnection) -> AppResult<Option<User>>;
    async fn find_by_email(&self, email: &str, conn: &mut PgConnection) -> AppResult<Option<User>>;
}

#[async_trait]
pub trait CatalogRepo: Send + Sync {
    async fn insert_item(&self, item: &CatalogItem, conn: &mut PgConnection) -> AppResult<()>;
    async fn get_item(
        &self,
        id: CatalogItemId,
        conn: &mut PgConnection,
    ) -> AppResult<Option<CatalogItem>>;
    async fn list_items(
        &self,
        conn: &mut PgConnection,
    ) -> AppResult<Vec<CatalogItem>>;
    async fn insert_billing_plan(
        &self,
        plan: &BillingPlan,
        conn: &mut PgConnection,
    ) -> AppResult<()>;
    async fn get_billing_plan(
        &self,
        id: BillingPlanId,
        conn: &mut PgConnection,
    ) -> AppResult<Option<BillingPlan>>;
}

#[async_trait]
pub trait PackageRepo: Send + Sync {
    async fn insert(&self, package: &Package, conn: &mut PgConnection) -> AppResult<()>;
    async fn get(&self, id: PackageId, conn: &mut PgConnection) -> AppResult<Option<Package>>;
    async fn list(&self, conn: &mut PgConnection) -> AppResult<Vec<Package>>;
}

#[async_trait]
pub trait PayPlanStackRepo: Send + Sync {
    async fn insert(&self, stack: &PayPlanStack, conn: &mut PgConnection) -> AppResult<()>;
    async fn get(
        &self,
        id: PayPlanStackId,
        conn: &mut PgConnection,
    ) -> AppResult<Option<PayPlanStack>>;
    async fn next_version(
        &self,
        name: &str,
        conn: &mut PgConnection,
    ) -> AppResult<u32>;
}

#[async_trait]
pub trait PurchaseRepo: Send + Sync {
    async fn insert(&self, purchase: &Purchase, conn: &mut PgConnection) -> AppResult<()>;
    async fn get(&self, id: PurchaseId, conn: &mut PgConnection) -> AppResult<Option<Purchase>>;
}

#[async_trait]
pub trait SubscriptionRepo: Send + Sync {
    async fn insert(&self, sub: &Subscription, conn: &mut PgConnection) -> AppResult<()>;
    async fn get(
        &self,
        id: SubscriptionId,
        conn: &mut PgConnection,
    ) -> AppResult<Option<Subscription>>;
    async fn list_active_for_user(
        &self,
        user_id: UserId,
        conn: &mut PgConnection,
    ) -> AppResult<Vec<Subscription>>;
}

#[async_trait]
pub trait EntitlementRepo: Send + Sync {
    async fn insert(&self, ent: &Entitlement, conn: &mut PgConnection) -> AppResult<()>;
    async fn list_for_user(
        &self,
        user_id: UserId,
        conn: &mut PgConnection,
    ) -> AppResult<Vec<Entitlement>>;
}

#[async_trait]
pub trait EnrollmentRepo: Send + Sync {
    async fn insert(&self, enrollment: &Enrollment, conn: &mut PgConnection) -> AppResult<()>;
    async fn get(&self, id: EnrollmentId, conn: &mut PgConnection)
        -> AppResult<Option<Enrollment>>;
    async fn list_for_user(
        &self,
        user_id: UserId,
        conn: &mut PgConnection,
    ) -> AppResult<Vec<Enrollment>>;
}

#[async_trait]
pub trait AllocationRepo: Send + Sync {
    async fn insert(&self, allocation: &ProductPayPlanAllocation, conn: &mut PgConnection) -> AppResult<()>;
    async fn get(&self, id: ProductPayPlanAllocationId, conn: &mut PgConnection) -> AppResult<Option<ProductPayPlanAllocation>>;
    async fn list_for_products(&self, product_ids: &[CatalogItemId], conn: &mut PgConnection) -> AppResult<Vec<ProductPayPlanAllocation>>;
    async fn list_all(&self, conn: &mut PgConnection) -> AppResult<Vec<ProductPayPlanAllocation>>;
    async fn delete(&self, id: ProductPayPlanAllocationId, conn: &mut PgConnection) -> AppResult<()>;
}
