use crate::shared::ids::{CompanyId, UserId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub email: String,
    pub password_hash: String,
    pub email_verified: bool,
    pub role: UserRole,
    pub company_id: Option<CompanyId>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserRole {
    User,
    CompanyAdmin,
    PlatformAdmin,
}

impl UserRole {
    pub fn can_admin_company(self) -> bool {
        matches!(self, Self::CompanyAdmin | Self::PlatformAdmin)
    }

    pub fn can_admin_platform(self) -> bool {
        matches!(self, Self::PlatformAdmin)
    }
}
