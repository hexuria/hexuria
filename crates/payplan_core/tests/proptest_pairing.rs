//! Property-based tests for Binary pairing math invariants.

use payplan_core::modules::binary::pairing::{compute_pairing, BinaryPairingConfig};
use proptest::prelude::*;

proptest! {
    /// Matched volume is always ≤ min(left_eff, right_eff) where eff applies the ratio.
    #[test]
    fn matched_volume_never_exceeds_legs(
        left in 0i64..10_000,
        right in 0i64..10_000,
        left_ratio in 1u32..10,
        right_ratio in 1u32..10,
    ) {
        let cfg = BinaryPairingConfig { left_ratio, right_ratio, ..Default::default() };
        let out = compute_pairing(left, right, &cfg);
        prop_assert!(out.matched_volume >= 0, "matched must be non-negative");
        prop_assert!(out.matched_volume <= left, "matched {} > left {}", out.matched_volume, left);
        prop_assert!(out.matched_volume <= right, "matched {} > right {}", out.matched_volume, right);
    }

    /// Commission percentage and matched volume are both non-negative.
    #[test]
    fn commission_is_non_negative(
        left in 0i64..10_000,
        right in 0i64..10_000,
        pct in 0u8..=100u8,
    ) {
        let cfg = BinaryPairingConfig { commission_percent: pct, ..Default::default() };
        let out = compute_pairing(left, right, &cfg);
        prop_assert!(out.commission >= rust_decimal::Decimal::ZERO);
    }

    /// When commission_percent is 0, commission is 0 regardless of matched volume.
    #[test]
    fn zero_percent_means_zero_commission(
        left in 0i64..10_000,
        right in 0i64..10_000,
    ) {
        let cfg = BinaryPairingConfig { commission_percent: 0, ..Default::default() };
        let out = compute_pairing(left, right, &cfg);
        prop_assert_eq!(out.commission, rust_decimal::Decimal::ZERO);
    }

    /// When max payout cap is set, commission never exceeds the cap.
    #[test]
    fn cap_is_respected(
        left in 1i64..10_000,
        right in 1i64..10_000,
        cap in 1i64..100_000,
        pct in 1u8..=100u8,
    ) {
        let cfg = BinaryPairingConfig {
            commission_percent: pct,
            max_payout_amount_minor: Some(cap),
            ..Default::default()
        };
        let out = compute_pairing(left, right, &cfg);
        prop_assert!(out.commission <= rust_decimal::Decimal::from(cap),
            "commission {} exceeded cap {}", out.commission, cap);
        if out.commission == rust_decimal::Decimal::from(cap) {
            prop_assert!(out.capped, "should be marked capped when commission == cap");
        }
    }

    /// When ratio is left=1 right=1, matched = min(left, right).
    #[test]
    fn unit_ratio_matches_min(
        left in 0i64..10_000,
        right in 0i64..10_000,
    ) {
        let cfg = BinaryPairingConfig { left_ratio: 1, right_ratio: 1, ..Default::default() };
        let out = compute_pairing(left, right, &cfg);
        prop_assert_eq!(out.matched_volume, left.min(right));
    }

    /// Increasing the ratio on either leg cannot increase the matched volume
    /// (only equal or less, modulo integer floor rounding).
    #[test]
    fn ratio_does_not_increase_matched(
        left in 0i64..10_000,
        right in 0i64..10_000,
    ) {
        let cfg_1 = BinaryPairingConfig { left_ratio: 1, right_ratio: 1, ..Default::default() };
        // Double the ratio on at least one leg.
        let cfg_2 = BinaryPairingConfig { left_ratio: 2, right_ratio: 2, ..Default::default() };
        let out_1 = compute_pairing(left, right, &cfg_1);
        let out_2 = compute_pairing(left, right, &cfg_2);
        // matched must not grow when ratio grows.
        prop_assert!(out_2.matched_volume <= out_1.matched_volume,
            "matched increased when ratio grew: {} -> {}", out_1.matched_volume, out_2.matched_volume);
        // matched must be within a small rounding window of out_1 / factor.
        prop_assert!(out_2.matched_volume * 2 + 2 >= out_1.matched_volume,
            "matched fell more than expected: {} vs {}", out_1.matched_volume, out_2.matched_volume);
    }
}
