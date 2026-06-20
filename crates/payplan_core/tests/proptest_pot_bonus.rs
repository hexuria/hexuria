//! Property tests for `distribute()` and `RoyalQualification` in
//! `modules/royal/pot_bonus.rs`.
//!
//! Config percents are constrained so `profit_share_percent + top_cycler_percent
//! <= 100` (the `debug_assert!` in `distribute`). The default weights
//! `[40, 30, 20, 10]` sum to 100 so the top-cycler payouts sum cleanly to
//! `top_cycler_total`.

use payplan_core::modules::royal::pot_bonus::{
    distribute, RoyalPotBonusConfig, RoyalQualification,
};
use payplan_core::shared::ids::UserId;
use proptest::prelude::*;
use rust_decimal::Decimal;

/// Build a config with both percents in `0..=50` so their sum never exceeds
/// 100 (satisfying `distribute`'s `debug_assert!`).
fn arb_config() -> impl Strategy<Value = RoyalPotBonusConfig> {
    (0u8..=50u8, 0u8..=50u8).prop_map(|(profit_share_percent, top_cycler_percent)| {
        RoyalPotBonusConfig {
            profit_share_percent,
            top_cycler_percent,
            top_cycler_weights: vec![40, 30, 20, 10],
        }
    })
}

proptest! {
    #[test]
    fn profit_share_total_is_proportional(
        pool_minor in 0i64..10_000,
        config in arb_config(),
        qualified_users in 0u32..20,
    ) {
        let pool = Decimal::from(pool_minor);
        let outcome = distribute(pool, &config, qualified_users).expect("Some outcome");
        let expected = pool
            * Decimal::from(config.profit_share_percent)
            / Decimal::from(100u32);
        prop_assert_eq!(outcome.profit_share_total, expected);
    }

    #[test]
    fn top_cycler_payouts_sum_to_total(
        pool_minor in 0i64..10_000,
        config in arb_config(),
        qualified_users in 0u32..20,
    ) {
        // Need at least one qualified user so distribute returns Some
        // (top_cycler_payouts are still produced when qualified_users==0,
        // but be defensive).
        let pool = Decimal::from(pool_minor);
        let outcome = distribute(pool, &config, qualified_users).expect("Some outcome");
        let top_cycler_total =
            pool * Decimal::from(config.top_cycler_percent) / Decimal::from(100u32);
        let sum: Decimal = outcome.top_cycler_payouts.iter().copied().sum();
        // Decimal division of integer weights sums back exactly (no rounding
        // loss because the default weights divide evenly into 100).
        prop_assert_eq!(sum, top_cycler_total);
    }

    #[test]
    fn per_qualified_user_divides_evenly(
        pool_minor in 0i64..10_000,
        config in arb_config(),
        // 1..20 so division is meaningful (0 handled separately).
        qualified_users in 1u32..20,
    ) {
        let pool = Decimal::from(pool_minor);
        let outcome = distribute(pool, &config, qualified_users).expect("Some outcome");
        // per_qualified_user * qualified_users must not exceed the total by
        // more than a tiny Decimal rounding epsilon. Decimal division rounds
        // to 28 places of precision, so the round-trip product can differ
        // from the dividend by a fraction of the smallest unit.
        let distributed = outcome.per_qualified_user * Decimal::from(qualified_users);
        let epsilon = Decimal::new(1, 6); // 0.000001
        let overshoot = distributed - outcome.profit_share_total;
        prop_assert!(
            overshoot <= epsilon,
            "distributed ({distributed}) overshoots profit_share_total ({}) by {overshoot}",
            outcome.profit_share_total
        );
    }

    #[test]
    fn distribution_never_exceeds_pool(
        pool_minor in 0i64..10_000,
        config in arb_config(),
        qualified_users in 0u32..20,
    ) {
        let pool = Decimal::from(pool_minor);
        let outcome = distribute(pool, &config, qualified_users).expect("Some outcome");
        let top_cycler_sum: Decimal = outcome.top_cycler_payouts.iter().copied().sum();
        let total = outcome.profit_share_total + top_cycler_sum;
        prop_assert!(
            total <= pool,
            "total distributed ({total}) must not exceed pool ({pool})"
        );
    }

    #[test]
    fn zero_qualified_users_means_zero_per_user(
        pool_minor in 0i64..10_000,
        config in arb_config(),
    ) {
        let pool = Decimal::from(pool_minor);
        let outcome = distribute(pool, &config, 0).expect("Some outcome (top cycler weights present)");
        prop_assert_eq!(outcome.per_qualified_user, Decimal::ZERO);
        prop_assert_eq!(outcome.qualified_user_count, 0);
    }

    #[test]
    fn qualification_requires_both_signals(
        graduations in 0u32..5,
        cycles in 0u32..5,
        extra_grad in 0u32..3,
        extra_cycles in 0u32..3,
    ) {
        let mut q = RoyalQualification::new(UserId::new());
        // Apply initial counts.
        for _ in 0..graduations {
            q.record_graduation();
        }
        for _ in 0..cycles {
            q.record_matrix_cycle();
        }
        let expected_qualified = graduations > 0 && cycles > 0;
        prop_assert_eq!(q.is_qualified, expected_qualified);

        // record_* is monotonic: totals never decrease, and calling them
        // extra times only strengthens qualification.
        let grad_before = q.total_graduations;
        let cycles_before = q.total_matrix_cycles;
        for _ in 0..extra_grad {
            q.record_graduation();
        }
        for _ in 0..extra_cycles {
            q.record_matrix_cycle();
        }
        prop_assert!(q.total_graduations >= grad_before);
        prop_assert!(q.total_matrix_cycles >= cycles_before);
        // If it was qualified, it stays qualified.
        if expected_qualified {
            prop_assert!(q.is_qualified);
        }
    }
}
