use crate::shared::ids::{
    CatalogItemId, CompanyId, EntitlementId, PackageId, PurchaseId, SubscriptionId, UserId,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entitlement {
    pub id: EntitlementId,
    pub company_id: CompanyId,
    pub user_id: UserId,
    pub package_id: PackageId,
    pub catalog_item_id: CatalogItemId,
    pub source_purchase_id: Option<PurchaseId>,
    pub source_subscription_id: Option<SubscriptionId>,
    pub status: EntitlementStatus,
    pub starts_at: DateTime<Utc>,
    pub ends_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntitlementStatus {
    Active,
    Suspended,
    Expired,
    Revoked,
}
