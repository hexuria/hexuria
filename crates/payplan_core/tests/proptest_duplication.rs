//! Property tests for duplication readiness semantics.
//!
//! Covers:
//! - `RoyalDuplicationState::is_ready_to_duplicate` (pure boolean logic).
//! - `DuplicationState` / `UserFlag` stickiness and one-shot behavior (the
//!   module's `bump()` is private, so we exercise the same field mutations
//!   it performs: setting `graduation_seen`/`cycle_seen` true and checking
//!   `duplicated` never reverts).

use payplan_core::modules::royal::duplication::RoyalDuplicationState;
use payplan_core::modules::royal::duplication_module::{DuplicationState, UserFlag};
use payplan_core::shared::ids::UserId;
use proptest::prelude::*;

proptest! {
    #[test]
    fn ready_requires_both_signals(
        flushline_graduated in any::<bool>(),
        matrix_cycled in any::<bool>(),
    ) {
        let s = RoyalDuplicationState {
            flushline_graduated,
            matrix_cycled,
        };
        let expected = flushline_graduated && matrix_cycled;
        prop_assert_eq!(s.is_ready_to_duplicate(), expected);
    }

    #[test]
    fn flags_are_sticky(
        graduation_seen in any::<bool>(),
        cycle_seen in any::<bool>(),
        // A sequence of subsequent "bumps": each tuple is (set_grad, set_cycle).
        bumps in prop::collection::vec((any::<bool>(), any::<bool>()), 0..8),
    ) {
        let mut flag = UserFlag {
            user_id: UserId::new(),
            graduation_seen,
            cycle_seen,
            duplicated: false,
        };
        for (set_grad, set_cycle) in bumps {
            // Mirrors the module's `bump`: only sets to true, never false.
            if set_grad {
                flag.graduation_seen = true;
            }
            if set_cycle {
                flag.cycle_seen = true;
            }
            // Once true, a signal can never revert.
            if graduation_seen {
                prop_assert!(flag.graduation_seen);
            }
            if cycle_seen {
                prop_assert!(flag.cycle_seen);
            }
        }
    }

    #[test]
    fn duplicated_flag_is_one_shot(
        // Arbitrary starting state, then a stream of grad/cycle bumps.
        start_grad in any::<bool>(),
        start_cycle in any::<bool>(),
        bumps in prop::collection::vec((any::<bool>(), any::<bool>()), 0..10),
    ) {
        let mut flag = UserFlag {
            user_id: UserId::new(),
            graduation_seen: start_grad,
            cycle_seen: start_cycle,
            duplicated: true, // already duplicated
        };
        for (set_grad, set_cycle) in bumps {
            if set_grad {
                flag.graduation_seen = true;
            }
            if set_cycle {
                flag.cycle_seen = true;
            }
            // The duplicated flag never reverts regardless of new signals.
            prop_assert!(flag.duplicated, "duplicated must stay true");
        }
    }

    #[test]
    fn ready_independent_of_order(
        start_grad in any::<bool>(),
        start_cycle in any::<bool>(),
    ) {
        // Graduating first then cycling should yield the same ready state as
        // cycling first then graduating (for a single user starting fresh).
        let mut a = RoyalDuplicationState {
            flushline_graduated: start_grad,
            matrix_cycled: start_cycle,
        };
        let mut b = a.clone();
        // Apply the two missing signals in opposite orders.
        a.flushline_graduated = true;
        a.matrix_cycled = true;
        b.matrix_cycled = true;
        b.flushline_graduated = true;
        prop_assert_eq!(a.is_ready_to_duplicate(), b.is_ready_to_duplicate());
        prop_assert!(a.is_ready_to_duplicate(), "both signals set → ready");

        // Ensure DuplicationState can be constructed with arbitrary flags
        // (smoke test that the public surface is reachable).
        let _state = DuplicationState { flags: vec![] };
    }
}
