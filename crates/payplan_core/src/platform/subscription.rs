use crate::shared::ids::{BillingPlanId, CompanyId, PackageId, SubscriptionId, UserId};
use crate::shared::period::Period;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: SubscriptionId,
    pub company_id: CompanyId,
    pub user_id: UserId,
    pub package_id: PackageId,
    pub billing_plan_id: BillingPlanId,
    pub status: SubscriptionStatus,
    pub current_period: Option<Period>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubscriptionStatus {
    Trialing,
    Active,
    PastDue,
    Cancelled,
    Expired,
}
