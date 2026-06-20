use crate::shared::ids::{CompanyId, EnrollmentId, PackageId, PurchaseId, UserId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enrollment {
    pub id: EnrollmentId,
    pub company_id: CompanyId,
    pub user_id: UserId,
    pub package_id: PackageId,
    pub purchase_id: PurchaseId,
    pub sponsor_user_id: Option<UserId>,
    pub status: EnrollmentStatus,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnrollmentStatus {
    Active,
    Suspended,
    Cancelled,
    Expired,
}
