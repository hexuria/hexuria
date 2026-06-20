use crate::shared::ids::{CompanyId, EventId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainEvent {
    pub id: EventId,
    pub company_id: Option<CompanyId>,
    pub event_type: EventType,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    CompanyCreated,
    UserCreated,
    CatalogItemCreated,
    BillingPlanCreated,
    PackageCreated,
    PackagePurchased,
    SubscriptionCreated,
    SubscriptionRenewed,
    SubscriptionCancelled,
    EntitlementGranted,
    EntitlementRevoked,
    EnrollmentCreated,
    EnrollmentSuspended,
    EnrollmentCancelled,
    RewardLedgerEntryCreated,
    RoyalFlushlineAccountCreated,
    RoyalFlushlineGraduated,
    RoyalMatrixCreated,
    RoyalMatrixCycled,
    RoyalPotBonusDistributed,
    RoyalAccountDuplicated,
    RoyalAccountResetToKing,
    BinaryNodePlaced,
    BinaryVolumeAdded,
    BinaryPairMatched,
    BinaryCommissionEarned,
    BinaryCycleClosed,
    BinaryCarryoverUpdated,
}

impl EventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CompanyCreated => "company.created",
            Self::UserCreated => "user.created",
            Self::CatalogItemCreated => "catalog_item.created",
            Self::BillingPlanCreated => "billing_plan.created",
            Self::PackageCreated => "package.created",
            Self::PackagePurchased => "package.purchased",
            Self::SubscriptionCreated => "subscription.created",
            Self::SubscriptionRenewed => "subscription.renewed",
            Self::SubscriptionCancelled => "subscription.cancelled",
            Self::EntitlementGranted => "entitlement.granted",
            Self::EntitlementRevoked => "entitlement.revoked",
            Self::EnrollmentCreated => "enrollment.created",
            Self::EnrollmentSuspended => "enrollment.suspended",
            Self::EnrollmentCancelled => "enrollment.cancelled",
            Self::RewardLedgerEntryCreated => "reward_ledger.entry_created",
            Self::RoyalFlushlineAccountCreated => "royal.flushline_account_created",
            Self::RoyalFlushlineGraduated => "royal.flushline_graduated",
            Self::RoyalMatrixCreated => "royal.matrix_created",
            Self::RoyalMatrixCycled => "royal.matrix_cycled",
            Self::RoyalPotBonusDistributed => "royal.pot_bonus_distributed",
            Self::RoyalAccountDuplicated => "royal.account_duplicated",
            Self::RoyalAccountResetToKing => "royal.account_reset_to_king",
            Self::BinaryNodePlaced => "binary.node_placed",
            Self::BinaryVolumeAdded => "binary.volume_added",
            Self::BinaryPairMatched => "binary.pair_matched",
            Self::BinaryCommissionEarned => "binary.commission_earned",
            Self::BinaryCycleClosed => "binary.cycle_closed",
            Self::BinaryCarryoverUpdated => "binary.carryover_updated",
        }
    }
}
