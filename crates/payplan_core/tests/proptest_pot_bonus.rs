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
        pool in 0i64..10_000,
        config in arb_config(),
        qualified_users in 0u32..20,
    ) {
        let outcome = distribute(pool, &config, qualified_users).expect("Some outcome");
        let expected = pool * i64::from(config.profit_share_percent) / 100;
        prop_assert_eq!(outcome.profit_share_total, expected);
    }

    #[test]
    fn top_cycler_payouts_sum_to_total(
        pool in 0i64..10_000,
        config in arb_config(),
        qualified_users in 0u32..20,
    ) {
        // Need at least one qualified user so distribute returns Some
        // (top_cycler_payouts are still produced when qualified_users==0,
        // but be defensive).
        let outcome = distribute(pool, &config, qualified_users).expect("Some outcome");
        let top_cycler_total = pool * i64::from(config.top_cycler_percent) / 100;
        let sum: i64 = outcome.top_cycler_payouts.iter().copied().sum();
        // With integer division, the sum of payouts may be slightly less than top_cycler_total due to rounding.
        prop_assert!(sum <= top_cycler_total);
        prop_assert!(top_cycler_total - sum <= 4, "top_cycler_total = {}, sum = {}", top_cycler_total, sum);
    }

    #[test]
    fn per_qualified_user_divides_evenly(
        pool in 0i64..10_000,
        config in arb_config(),
        // 1..20 so division is meaningful (0 handled separately).
        qualified_users in 1u32..20,
    ) {
        let outcome = distribute(pool, &config, qualified_users).expect("Some outcome");
        // per_qualified_user * qualified_users must not exceed the total.
        // Due to integer division truncation, per_qualified_user * qualified_users <= profit_share_total.
        let distributed = outcome.per_qualified_user * i64::from(qualified_users);
        prop_assert!(
            distributed <= outcome.profit_share_total,
            "distributed ({distributed}) cannot exceed profit_share_total ({})",
            outcome.profit_share_total
        );
    }

    #[test]
    fn distribution_never_exceeds_pool(
        pool in 0i64..10_000,
        config in arb_config(),
        qualified_users in 0u32..20,
    ) {
        let outcome = distribute(pool, &config, qualified_users).expect("Some outcome");
        let top_cycler_sum: i64 = outcome.top_cycler_payouts.iter().copied().sum();
        let total = outcome.profit_share_total + top_cycler_sum;
        prop_assert!(
            total <= pool,
            "total distributed ({total}) must not exceed pool ({pool})"
        );
    }

    #[test]
    fn zero_qualified_users_means_zero_per_user(
        pool in 0i64..10_000,
        config in arb_config(),
    ) {
        let outcome = distribute(pool, &config, 0).expect("Some outcome (top cycler weights present)");
        prop_assert_eq!(outcome.per_qualified_user, 0);
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
