//! Property-based tests for Royal Flushline invariants.
//!
//! Uses `proptest` to fuzz the `apply_points` + `weekly_reset` machinery over
//! all `u32` point grants and assert that:
//!
//! - After applying N points, the cumulative points never decrease.
//! - Graduation only happens at exactly 15 cumulative points.
//! - Graduated accounts stay graduated (no further point application changes them).
//! - Weekly reset moves graduated accounts to King (tier 4) with points 0,
//!   non-graduated accounts to Ten (tier 0) with points 0.
//! - Tier transitions always strictly increase (or stay equal for repeat grants).
//! - The cumulative-points-to-tier mapping is monotonic.

use payplan_core::modules::royal::flushline::{
    RoyalFlushlineAccount, RoyalTier, ROYAL_GRADUATION_POINTS,
};
use payplan_core::shared::ids::{CompanyId, EnrollmentId, UserId};
use proptest::prelude::*;

fn arb_account() -> impl Strategy<Value = RoyalFlushlineAccount> {
    (any::<u32>(), any::<bool>(), any::<bool>()).prop_map(
        |(points, graduated, had_graduated_at)| RoyalFlushlineAccount {
            id: payplan_core::shared::ids::RoyalAccountId::new(),
            company_id: CompanyId::new(),
            enrollment_id: EnrollmentId::new(),
            owner_user_id: UserId::new(),
            current_tier: tier_for_points(points),
            current_points: points,
            graduated,
            graduated_at: if had_graduated_at {
                Some(chrono::Utc::now())
            } else {
                None
            },
            created_at: chrono::Utc::now(),
        },
    )
}

fn tier_for_points(points: u32) -> RoyalTier {
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

proptest! {
    /// Applying N points never reduces the cumulative points count.
    #[test]
    fn apply_points_is_monotonic(
        account in arb_account(),
        grant in 0u32..1000,
    ) {
        let before = account.current_points;
        let after = account.apply_points(grant);
        prop_assert!(after.current_points >= before, "points went down: {} -> {}", before, after.current_points);
    }

    /// Graduation flag only flips true at exactly 15 cumulative points (or above).
    #[test]
    fn graduation_only_at_threshold(
        account in arb_account(),
        grant in 0u32..100,
    ) {
        let after = account.apply_points(grant);
        if after.graduated {
            prop_assert!(after.current_points >= ROYAL_GRADUATION_POINTS,
                "graduated at {} < {}", after.current_points, ROYAL_GRADUATION_POINTS);
        } else {
            prop_assert!(after.current_points < ROYAL_GRADUATION_POINTS,
                "not graduated but reached {} >= {}", after.current_points, ROYAL_GRADUATION_POINTS);
        }
    }

    /// Once graduated, the account is "stuck": additional points have no effect.
    #[test]
    fn graduated_accounts_are_immutable(
        mut account in arb_account(),
        grant in 0u32..1000,
    ) {
        // Force graduation.
        account.current_points = ROYAL_GRADUATION_POINTS;
        account.graduated = true;
        account.current_tier = RoyalTier::Ace;
        let before_tier = account.current_tier;
        let before_graduated_at = account.graduated_at;
        let after = account.apply_points(grant);
        prop_assert_eq!(after.current_tier, before_tier);
        prop_assert_eq!(after.current_points, ROYAL_GRADUATION_POINTS,
            "graduated points should not change");
        prop_assert!(after.graduated);
        prop_assert_eq!(after.graduated_at, before_graduated_at,
            "graduated_at timestamp must not change");
    }

    /// Weekly reset of a graduated account moves to King (tier 4) with 0 points,
    /// preserving the graduated flag.
    #[test]
    fn weekly_reset_graduated_lands_on_king(
        mut account in arb_account(),
    ) {
        account.graduated = true;
        account.current_tier = RoyalTier::Ace;
        account.current_points = 99;
        let after = account.weekly_reset();
        prop_assert_eq!(after.current_tier, RoyalTier::King);
        prop_assert_eq!(after.current_points, 0);
        prop_assert!(after.graduated, "graduated flag preserved across reset");
    }

    /// Weekly reset of a non-graduated account moves back to Ten with 0 points.
    #[test]
    fn weekly_reset_non_graduated_lands_on_ten(
        mut account in arb_account(),
    ) {
        account.graduated = false;
        account.current_points = account.current_points.min(ROYAL_GRADUATION_POINTS - 1);
        let after = account.weekly_reset();
        prop_assert_eq!(after.current_tier, RoyalTier::Ten);
        prop_assert_eq!(after.current_points, 0);
        prop_assert!(!after.graduated);
    }

    /// Tier index never decreases when applying points.
    #[test]
    fn tier_never_decreases_with_points(
        account in arb_account(),
        grant in 0u32..1000,
    ) {
        let before = account.current_tier;
        let after = account.apply_points(grant);
        prop_assert!(
            tier_index(after.current_tier) >= tier_index(before),
            "tier went from {:?} to {:?}", before, after.current_tier
        );
    }

    /// Cumulative thresholds to reach each tier are 1, 3, 6, 10, 15.
    #[test]
    fn canonical_thresholds_are_stable(_unused in 0u8..1) {
        prop_assert_eq!(RoyalTier::Ten.cumulative_points(), 1);
        prop_assert_eq!(RoyalTier::Jack.cumulative_points(), 3);
        prop_assert_eq!(RoyalTier::Queen.cumulative_points(), 6);
        prop_assert_eq!(RoyalTier::King.cumulative_points(), 10);
        prop_assert_eq!(RoyalTier::Ace.cumulative_points(), 15);
    }

    /// Applying points to a Ten-tier account with >=1 point always advances
    /// to Jack (since Ten threshold is 1).
    #[test]
    fn one_point_advances_ten_to_jack(
        grant in 1u32..1000,
    ) {
        let account = RoyalFlushlineAccount::new(CompanyId::new(), EnrollmentId::new(), UserId::new());
        let after = account.apply_points(grant);
        prop_assert!(tier_index(after.current_tier) >= tier_index(RoyalTier::Jack));
    }
}

fn tier_index(t: RoyalTier) -> u8 {
    match t {
        RoyalTier::Ten => 0,
        RoyalTier::Jack => 1,
        RoyalTier::Queen => 2,
        RoyalTier::King => 3,
        RoyalTier::Ace => 4,
    }
}
