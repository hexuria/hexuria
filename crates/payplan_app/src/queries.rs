use async_trait::async_trait;
use chrono::{DateTime, Utc};
use payplan_core::shared::ids::UserId;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageRequest {
    pub page: u32,
    pub page_size: u32,
    pub query: Option<String>,
}

impl PageRequest {
    #[must_use]
    pub fn normalized(self) -> Self {
        Self {
            page: self.page.max(1),
            page_size: self.page_size.clamp(1, 100),
            query: self.query.and_then(|query| {
                let query = query.trim().to_string();
                (!query.is_empty()).then_some(query)
            }),
        }
    }

    #[must_use]
    pub fn offset(&self) -> i64 {
        i64::from(self.page.saturating_sub(1) * self.page_size)
    }
}

impl Default for PageRequest {
    fn default() -> Self {
        Self {
            page: 1,
            page_size: 25,
            query: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub page: u32,
    pub page_size: u32,
    pub total_items: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardView {
    pub user_count: u64,
    pub package_count: u64,
    pub purchase_count: u64,
    pub recent_purchases: Vec<PurchaseRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRow {
    pub id: UserId,
    pub email: String,
    pub role: String,
    pub email_verified: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogRow {
    pub id: uuid::Uuid,
    pub name: String,
    pub item_type: String,
    pub sku: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingRow {
    pub id: uuid::Uuid,
    pub catalog_item_name: String,
    pub billing_type: String,
    pub price: String,
    pub currency: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageRow {
    pub id: uuid::Uuid,
    pub name: String,
    pub status: String,
    pub item_count: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseRow {
    pub id: uuid::Uuid,
    pub user_id: UserId,
    pub package_name: String,
    pub amount: String,
    pub currency: String,
    pub status: String,
    pub purchased_at: DateTime<Utc>,
}

#[async_trait]
pub trait AdminQueryService: Send + Sync {
    async fn dashboard(&self) -> AppResult<DashboardView>;
    async fn users(&self, request: PageRequest) -> AppResult<Page<UserRow>>;
    async fn catalog(&self, request: PageRequest) -> AppResult<Page<CatalogRow>>;
    async fn billing(&self, request: PageRequest) -> AppResult<Page<BillingRow>>;
    async fn packages(&self, request: PageRequest) -> AppResult<Page<PackageRow>>;
    async fn purchases(&self, request: PageRequest) -> AppResult<Page<PurchaseRow>>;
}

#[cfg(test)]
mod tests {
    use super::PageRequest;

    #[test]
    fn page_request_enforces_safe_bounds() {
        let request = PageRequest {
            page: 0,
            page_size: 1_000,
            query: Some("   ".into()),
        }
        .normalized();

        assert_eq!(request.page, 1);
        assert_eq!(request.page_size, 100);
        assert_eq!(request.query, None);
    }
}
