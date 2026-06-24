use crate::shared::ids::{EnrollmentId, EventId, LedgerEntryId, PackageId, UserId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardLedgerEntry {
    pub id: LedgerEntryId,
    pub user_id: UserId,
    pub enrollment_id: Option<EnrollmentId>,
    pub package_id: Option<PackageId>,
    pub source_module: String,
    pub source_event_id: Option<EventId>,
    pub points: i64,
    pub status: LedgerStatus,
    pub reason: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LedgerStatus {
    Pending,
    Approved,
    Paid,
    Reversed,
    Voided,
}
