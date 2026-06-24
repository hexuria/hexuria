use crate::shared::ids::PayPlanStackId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayPlanStack {
    pub id: PayPlanStackId,
    pub name: String,
    pub version: u32,
    pub status: PayPlanStackStatus,
    pub modules: Vec<StackModule>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PayPlanStackStatus {
    Draft,
    Active,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackModule {
    pub module_key: String,
    pub module_version: String,
    pub sort_order: u32,
    pub config: serde_json::Value,
    pub active: bool,
}
