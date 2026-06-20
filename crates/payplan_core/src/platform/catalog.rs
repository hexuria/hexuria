use crate::shared::ids::{BillingPlanId, CatalogItemId, CompanyId};
use crate::shared::money::Money;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogItem {
    pub id: CatalogItemId,
    pub company_id: CompanyId,
    pub name: String,
    pub description: Option<String>,
    pub item_type: CatalogItemType,
    pub sku: Option<String>,
    pub status: CatalogItemStatus,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CatalogItemType {
    Product,
    Service,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CatalogItemStatus {
    Active,
    Inactive,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingPlan {
    pub id: BillingPlanId,
    pub catalog_item_id: CatalogItemId,
    pub billing_type: BillingType,
    pub price: Money,
    pub recurring: Option<RecurringSettings>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BillingType {
    OneTime,
    Recurring,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurringSettings {
    pub interval: RecurrenceInterval,
    pub interval_count: u32,
    pub trial_days: u32,
    pub grace_period_days: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecurrenceInterval {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Yearly,
}
