//! Property tests for `BinaryCarryover::from_unmatched` in
//! `modules/binary/carryover.rs`.

use payplan_core::modules::binary::carryover::BinaryCarryover;
use proptest::prelude::*;

proptest! {
    #[test]
    fn clamps_negatives_to_zero(
        left in any::<i64>(),
        right in any::<i64>(),
    ) {
        let c = BinaryCarryover::from_unmatched(left, right);
        prop_assert_eq!(c.left_volume, left.max(0));
        prop_assert_eq!(c.right_volume, right.max(0));
    }

    #[test]
    fn result_never_negative(
        left in any::<i64>(),
        right in any::<i64>(),
    ) {
        let c = BinaryCarryover::from_unmatched(left, right);
        prop_assert!(c.left_volume >= 0, "left_volume {} must be >= 0", c.left_volume);
        prop_assert!(c.right_volume >= 0, "right_volume {} must be >= 0", c.right_volume);
    }

    #[test]
    fn idempotent_under_clamping(
        left in any::<i64>(),
        right in any::<i64>(),
    ) {
        let c = BinaryCarryover::from_unmatched(left, right);
        // Re-applying from_unmatched to the already-clamped volumes is a no-op
        // (the projection keys company_id/node_id are irrelevant to the math).
        let c2 = BinaryCarryover::from_unmatched(c.left_volume, c.right_volume);
        prop_assert_eq!(c2.left_volume, c.left_volume);
        prop_assert_eq!(c2.right_volume, c.right_volume);
    }

    #[test]
    fn imbalance_equals_absolute_difference(
        left in 0i64..100_000,
        right in 0i64..100_000,
    ) {
        // After pairing drains the matched volume, the carryover represents
        // the absolute imbalance between the two legs.
        let matched = left.min(right);
        let c = BinaryCarryover::from_unmatched(left - matched, right - matched);
        let total = c.left_volume + c.right_volume;
        prop_assert_eq!(total, (left - right).abs());
    }
}
