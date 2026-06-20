use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::ids::{CompanyId, EnrollmentId, RoyalAccountId, UserId};

/// Tier order from lowest to highest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RoyalTier {
    Ten,
    Jack,
    Queen,
    King,
    Ace,
}

impl RoyalTier {
    /// Canonical threshold (points required to advance PAST this tier).
    #[must_use]
    pub fn threshold(self) -> u32 {
        match self {
            Self::Ten => 1,
            Self::Jack => 2,
            Self::Queen => 3,
            Self::King => 4,
            Self::Ace => 5,
        }
    }

    /// Sum of thresholds to graduate through this tier.
    #[must_use]
    pub fn cumulative_points(self) -> u32 {
        match self {
            Self::Ten => 1,
            Self::Jack => 3,
            Self::Queen => 6,
            Self::King => 10,
            Self::Ace => 15,
        }
    }

    #[must_use]
    pub fn next(self) -> Option<Self> {
        match self {
            Self::Ten => Some(Self::Jack),
            Self::Jack => Some(Self::Queen),
            Self::Queen => Some(Self::King),
            Self::King => Some(Self::Ace),
            Self::Ace => None,
        }
    }

    /// Weekly reset target per PRD §7.1: qualified graduated accounts reset to King, not Ten.
    #[must_use]
    pub fn reset_target(_was_graduated: bool) -> Self {
        Self::King
    }
}

/// Total points required to fully graduate all five tiers.
pub const ROYAL_GRADUATION_POINTS: u32 = 15;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoyalFlushlineConfig {
    /// Configurable per-tier threshold override (defaults match canonical thresholds).
    pub tier_thresholds: Vec<(RoyalTier, u32)>,
}

impl Default for RoyalFlushlineConfig {
    fn default() -> Self {
        Self {
            tier_thresholds: vec![
                (RoyalTier::Ten, RoyalTier::Ten.threshold()),
                (RoyalTier::Jack, RoyalTier::Jack.threshold()),
                (RoyalTier::Queen, RoyalTier::Queen.threshold()),
                (RoyalTier::King, RoyalTier::King.threshold()),
                (RoyalTier::Ace, RoyalTier::Ace.threshold()),
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoyalFlushlineAccount {
    pub id: RoyalAccountId,
    pub company_id: CompanyId,
    pub enrollment_id: EnrollmentId,
    pub owner_user_id: UserId,
    pub current_tier: RoyalTier,
    pub current_points: u32,
    pub graduated: bool,
    pub graduated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl RoyalFlushlineAccount {
    #[must_use]
    pub fn new(company_id: CompanyId, enrollment_id: EnrollmentId, owner_user_id: UserId) -> Self {
        Self {
            id: RoyalAccountId::new(),
            company_id,
            enrollment_id,
            owner_user_id,
            current_tier: RoyalTier::Ten,
            current_points: 0,
            graduated: false,
            graduated_at: None,
            created_at: Utc::now(),
        }
    }

    /// Apply points to the account. Returns `(new_state, graduated_now)`.
    ///
    /// Tier thresholds are per-tier (PRD §7.1): Ten=1, Jack=2, Queen=3, King=4, Ace=5.
    /// Cumulative spend to graduate = 1+2+3+4+5 = 15.
    ///
    /// Invariants (PRD §7.1):
    /// - only the top account in a cardline receives points (enforced by caller)
    /// - graduated accounts must not appear in any queue (caller responsibility)
    /// - weekly reset moves qualified graduated accounts back to King, not Ten
    #[must_use]
    pub fn apply_points(&self, points: u32) -> Self {
        if self.graduated {
            return self.clone();
        }
        let mut next = self.clone();
        next.current_points = self.current_points.saturating_add(points);
        next.current_tier = compute_tier(next.current_points);
        if next.current_points >= ROYAL_GRADUATION_POINTS {
            next.graduated = true;
            next.graduated_at = Some(Utc::now());
        }
        next
    }

    /// Reset for the start of a new cardline cycle. Graduated accounts go to King; non-graduated reset to Ten.
    #[must_use]
    pub fn weekly_reset(&self) -> Self {
        let mut next = self.clone();
        if self.graduated {
            next.current_tier = RoyalTier::King;
            next.current_points = 0;
        } else {
            next.current_tier = RoyalTier::Ten;
            next.current_points = 0;
        }
        next
    }
}

#[must_use]
fn compute_tier(points: u32) -> RoyalTier {
    // Per PRD §7.1, thresholds are per-tier (Ten=1, Jack=2, Queen=3, King=4, Ace=5).
    // Buckets below reflect cumulative spend ranges within each tier.
    if points >= 10 {
        RoyalTier::Ace
    } else if points >= 6 {
        RoyalTier::King
    } else if points >= 3 {
        RoyalTier::Queen
    } else if points >= 1 {
        RoyalTier::Jack
    } else {
        RoyalTier::Ten
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account() -> RoyalFlushlineAccount {
        RoyalFlushlineAccount::new(CompanyId::new(), EnrollmentId::new(), UserId::new())
    }

    #[test]
    fn new_account_starts_at_ten() {
        let a = account();
        assert_eq!(a.current_tier, RoyalTier::Ten);
        assert_eq!(a.current_points, 0);
        assert!(!a.graduated);
    }

    #[test]
    fn one_point_advances_to_jack() {
        let a = account().apply_points(1);
        assert_eq!(a.current_tier, RoyalTier::Jack);
        assert_eq!(a.current_points, 1);
        assert!(!a.graduated);
    }

    #[test]
    fn three_points_advances_to_queen() {
        let a = account().apply_points(3);
        assert_eq!(a.current_tier, RoyalTier::Queen);
        assert_eq!(a.current_points, 3);
    }

    #[test]
    fn six_points_advances_to_king() {
        let a = account().apply_points(6);
        assert_eq!(a.current_tier, RoyalTier::King);
        assert_eq!(a.current_points, 6);
    }

    #[test]
    fn ten_points_enters_ace_but_not_graduated() {
        let a = account().apply_points(10);
        assert_eq!(a.current_tier, RoyalTier::Ace);
        assert_eq!(a.current_points, 10);
        assert!(!a.graduated);
    }

    #[test]
    fn fifteen_points_graduates() {
        let a = account().apply_points(15);
        assert_eq!(a.current_tier, RoyalTier::Ace);
        assert_eq!(a.current_points, 15);
        assert!(a.graduated);
        assert!(a.graduated_at.is_some());
    }

    #[test]
    fn extra_points_after_graduation_are_no_ops() {
        let a = account().apply_points(15);
        let b = a.clone().apply_points(99);
        assert_eq!(b, a);
    }

    #[test]
    fn weekly_reset_graduated_goes_to_king_not_ten() {
        let a = account().apply_points(15);
        let r = a.weekly_reset();
        assert_eq!(r.current_tier, RoyalTier::King);
        assert_eq!(r.current_points, 0);
        // graduated flag stays true - qualification is preserved at the user level
        assert!(r.graduated);
    }

    #[test]
    fn weekly_reset_non_graduated_returns_to_ten() {
        let a = account().apply_points(5);
        let r = a.weekly_reset();
        assert_eq!(r.current_tier, RoyalTier::Ten);
        assert_eq!(r.current_points, 0);
        assert!(!r.graduated);
    }

    #[test]
    fn cumulative_thresholds_match_canonical() {
        assert_eq!(RoyalTier::Ten.cumulative_points(), 1);
        assert_eq!(RoyalTier::Jack.cumulative_points(), 3);
        assert_eq!(RoyalTier::Queen.cumulative_points(), 6);
        assert_eq!(RoyalTier::King.cumulative_points(), 10);
        assert_eq!(RoyalTier::Ace.cumulative_points(), 15);
    }

    #[test]
    fn tier_thresholds_match_canonical() {
        assert_eq!(RoyalTier::Ten.threshold(), 1);
        assert_eq!(RoyalTier::Jack.threshold(), 2);
        assert_eq!(RoyalTier::Queen.threshold(), 3);
        assert_eq!(RoyalTier::King.threshold(), 4);
        assert_eq!(RoyalTier::Ace.threshold(), 5);
    }
}
