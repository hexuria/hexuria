//! Property-based tests for Binary tree auto-balance placement.
//!
//! Verifies that the auto-balance strategy always produces a valid binary tree:
//! - Each parent has at most one Left and one Right child.
//! - The first user becomes the root.
//! - Total placed == expected.

use chrono::Utc;
use payplan_core::modules::binary::tree::BinaryLeg;
use payplan_core::modules::binary::tree_module::{
    BinaryTreeConfig, BinaryTreeModule, BinaryTreeState,
};
use payplan_core::payplan::events::{DomainEvent, EventType};
use payplan_core::payplan::module::ModuleContext;
use payplan_core::payplan::runner::{StackRunner, StateCache};
use payplan_core::payplan::stack::{PayPlanStack, PayPlanStackStatus, StackModule};
use payplan_core::shared::ids::{
    BinaryNodeId, EnrollmentId, PackageId, PayPlanStackId, UserId,
};
use proptest::prelude::*;
use serde_json::json;
use std::collections::HashMap;

fn fresh_stack() -> PayPlanStack {
    PayPlanStack {
        id: PayPlanStackId::new(),
        name: "Binary Test".into(),
        version: 1,
        status: PayPlanStackStatus::Active,
        modules: vec![StackModule {
            module_key: "binary.tree".into(),
            module_version: "1.0.0".into(),
            sort_order: 10,
            config: json!({"strategy": "auto_balance"}),
            active: true,
        }],
        created_at: Utc::now(),
    }
}

fn run_one(
    stack: &PayPlanStack,
    state: &BinaryTreeState,
    user_id: UserId,
    enrollment_id: EnrollmentId,
) -> BinaryTreeState {
    let module = BinaryTreeModule::new(BinaryTreeConfig::default());
    let mut registry = payplan_core::payplan::registry::ModuleRegistry::new();
    registry.register(module);
    let runner = StackRunner::new(registry);
    let event = DomainEvent {
        id: payplan_core::shared::ids::EventId::new(),
        event_type: EventType::EnrollmentCreated,
        payload: json!({"user_id": user_id, "package_id": PackageId::new()}),
        created_at: Utc::now(),
    };
    let ctx = ModuleContext::new(PackageId::new())
        .with_enrollment(enrollment_id)
        .with_event(event)
        .with_module_state(serde_json::to_value(state).unwrap());
    let mut cache = StateCache::new();
    let result = runner
        .run(
            stack,
            &ctx.clone().triggering_event.unwrap(),
            &ctx,
            &mut cache,
        )
        .expect("run");
    result
        .state_changes
        .iter()
        .find(|sc| sc.module_key == "binary.tree")
        .and_then(|sc| serde_json::from_value::<BinaryTreeState>(sc.value.clone()).ok())
        .unwrap_or_default()
}

proptest! {
    /// Each parent has at most one Left and one Right child.
    #[test]
    fn auto_balance_respects_leg_uniqueness(n_users in 1usize..15) {
        let stack = fresh_stack();
        let mut state = BinaryTreeState::default();
        for _ in 0..n_users {
            state = run_one(&stack, &state, UserId::new(), EnrollmentId::new());
        }
        let mut per_parent: HashMap<BinaryNodeId, (i32, i32)> = HashMap::new();
        for n in &state.nodes {
            if let (Some(pid), Some(leg)) = (n.parent_node_id, n.leg) {
                let entry = per_parent.entry(pid).or_insert((0, 0));
                match leg {
                    BinaryLeg::Left => entry.0 += 1,
                    BinaryLeg::Right => entry.1 += 1,
                }
            }
        }
        for (pid, (l, r)) in &per_parent {
            prop_assert!(*l <= 1, "parent {:?} has {} left children", pid, l);
            prop_assert!(*r <= 1, "parent {:?} has {} right children", pid, r);
        }
        prop_assert_eq!(state.nodes.len(), n_users);
    }

    /// The first user becomes the root.
    #[test]
    fn first_user_is_root(_dummy in 0u8..1) {
        let stack = fresh_stack();
        let state = run_one(&stack, &BinaryTreeState::default(), UserId::new(), EnrollmentId::new());
        prop_assert_eq!(state.nodes.len(), 1);
        prop_assert!(state.nodes[0].parent_node_id.is_none());
        prop_assert!(state.nodes[0].leg.is_none());
    }
}
