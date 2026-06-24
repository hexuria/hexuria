use crate::shared::ids::{PackageId, PurchaseId, UserId};
use crate::shared::money::Money;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Purchase {
    pub id: PurchaseId,
    pub user_id: UserId,
    pub package_id: PackageId,
    pub sponsor_user_id: Option<UserId>,
    pub gross: Money,
    pub net: Money,
    pub status: PurchaseStatus,
    pub purchased_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PurchaseStatus {
    Pending,
    Paid,
    Failed,
    Refunded,
    Cancelled,
}
