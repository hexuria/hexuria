use crate::shared::ids::{BillingPlanId, CatalogItemId, CompanyId, PackageId, PayPlanStackId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub id: PackageId,
    pub company_id: CompanyId,
    pub name: String,
    pub description: Option<String>,
    pub status: PackageStatus,
    pub pay_plan_stack_id: Option<PayPlanStackId>,
    pub default_billing_plan_id: Option<BillingPlanId>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
    pub items: Vec<PackageItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackageStatus {
    Draft,
    Active,
    Inactive,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageItem {
    pub catalog_item_id: CatalogItemId,
    pub billing_plan_id: BillingPlanId,
    pub quantity: u32,
    pub role: PackageItemRole,
    pub is_commissionable: bool,
    pub commissionable_volume: u32,
    pub points_value: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackageItemRole {
    Included,
    Required,
    OptionalAddon,
    Upsell,
}
