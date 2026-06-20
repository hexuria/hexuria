//! Property tests for `BinaryLegTotals` (`add`, `matched_pairs`) in
//! `modules/binary/volume.rs`.
//!
//! Bounds keep leg totals/amounts under `i64::MAX / 4` so chained `add`s
//! cannot overflow.

use payplan_core::modules::binary::tree::BinaryLeg;
use payplan_core::modules::binary::volume::BinaryLegTotals;
use proptest::prelude::*;

fn arb_leg() -> impl Strategy<Value = BinaryLeg> {
    any::<bool>().prop_map(|b| if b { BinaryLeg::Left } else { BinaryLeg::Right })
}

proptest! {
    #[test]
    fn matched_pairs_equals_min_of_legs(
        left in 0i64..100_000,
        right in 0i64..100_000,
    ) {
        let totals = BinaryLegTotals { left, right };
        prop_assert_eq!(totals.matched_pairs(), left.min(right));
    }

    #[test]
    fn matched_pairs_never_negative(
        left in 0i64..100_000,
        right in 0i64..100_000,
    ) {
        let totals = BinaryLegTotals { left, right };
        prop_assert!(totals.matched_pairs() >= 0);
    }

    #[test]
    fn add_targets_only_one_leg(
        left in 0i64..50_000,
        right in 0i64..50_000,
        amount in 0i64..50_000,
        leg in arb_leg(),
    ) {
        let totals = BinaryLegTotals { left, right };
        let next = totals.add(leg, amount);
        match leg {
            BinaryLeg::Left => {
                prop_assert_eq!(next.left, left + amount);
                prop_assert_eq!(next.right, right, "add(Left) must not touch right");
            }
            BinaryLeg::Right => {
                prop_assert_eq!(next.right, right + amount);
                prop_assert_eq!(next.left, left, "add(Right) must not touch left");
            }
        }
    }

    #[test]
    fn add_is_identity_for_zero(
        left in 0i64..100_000,
        right in 0i64..100_000,
        leg in arb_leg(),
    ) {
        let totals = BinaryLegTotals { left, right };
        let next = totals.add(leg, 0);
        prop_assert_eq!(next.left, totals.left);
        prop_assert_eq!(next.right, totals.right);
    }

    #[test]
    fn add_is_commutative_across_legs(
        left in 0i64..25_000,
        right in 0i64..25_000,
        a in 0i64..25_000,
        b in 0i64..25_000,
    ) {
        let t = BinaryLegTotals { left, right };
        let lr = t.add(BinaryLeg::Left, a).add(BinaryLeg::Right, b);
        let rl = t.add(BinaryLeg::Right, b).add(BinaryLeg::Left, a);
        prop_assert_eq!(lr.left, rl.left);
        prop_assert_eq!(lr.right, rl.right);
    }
}
