use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{CoreError, CoreResult};
use crate::modules::binary::tree::{BinaryLeg, BinaryNode, BinaryPlacementStrategy};
use crate::payplan::events::EventType;
use crate::payplan::module::{ModuleContext, ModuleResult};
use crate::payplan::registry::{AggregateScope, Module};
use crate::shared::ids::{BinaryNodeId, UserId};

/// Per-(system) tree state. Maps `user_id -> BinaryNodeId`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BinaryTreeState {
    pub nodes: Vec<BinaryNode>,
    /// Quick lookup.
    #[serde(skip)]
    pub user_to_node: BTreeMap<UserId, BinaryNodeId>,
}

impl BinaryTreeState {
    pub fn from_nodes(nodes: Vec<BinaryNode>) -> Self {
        let mut user_to_node = BTreeMap::new();
        for n in &nodes {
            user_to_node.insert(n.user_id, n.id);
        }
        Self {
            nodes,
            user_to_node,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryTreeConfig {
    pub strategy: BinaryPlacementStrategy,
}

impl Default for BinaryTreeConfig {
    fn default() -> Self {
        Self {
            strategy: BinaryPlacementStrategy::AutoBalance,
        }
    }
}

pub struct BinaryTreeModule {
    config: BinaryTreeConfig,
}

impl BinaryTreeModule {
    #[must_use]
    pub fn new(config: BinaryTreeConfig) -> Self {
        Self { config }
    }
}

impl Module for BinaryTreeModule {
    fn key(&self) -> &'static str {
        "binary.tree"
    }
    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn handles(&self) -> &'static [EventType] {
        &[EventType::EnrollmentCreated, EventType::BinaryNodePlaced]
    }

    /// The binary tree is a single system-wide genealogy. Scoping it to the
    /// enrollment would make every member load an empty tree and place itself
    /// as a root — no tree would ever form.
    fn scope(&self) -> AggregateScope {
        AggregateScope::Global
    }

    fn run(&self, ctx: &ModuleContext) -> CoreResult<ModuleResult> {
        let mut state: BinaryTreeState = ctx.decode_state().map_err(CoreError::from)?;
        // `user_to_node` is `#[serde(skip)]`, so a state loaded from
        // persistence comes back with an empty index even though `nodes` is
        // populated. Rebuild it from `nodes` or the idempotency guard and
        // sponsor lookups below would always miss (Task 12).
        state = BinaryTreeState::from_nodes(state.nodes);
        let mut result = ModuleResult::empty();

        let Some(event) = &ctx.triggering_event else {
            return Ok(result);
        };
        if event.event_type != EventType::EnrollmentCreated {
            return Ok(result);
        }

        let Some(user_id) = event
            .payload
            .get("user_id")
            .and_then(|v| v.as_str())
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
            .map(UserId::from)
        else {
            result.warn("binary.tree: enrollment event missing user_id");
            return Ok(result);
        };
        let sponsor = event
            .payload
            .get("sponsor_user_id")
            .and_then(|v| v.as_str())
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
            .map(UserId::from);

        // Already placed? Idempotent.
        if state.user_to_node.contains_key(&user_id) {
            return Ok(result);
        }

        let new_id = BinaryNodeId::new();

        // Pick parent + leg using the configured strategy.
        let (parent_id, leg) = pick_placement(&state, sponsor, self.config.strategy);

        let node = BinaryNode {
            id: new_id,
            user_id,
            sponsor_user_id: sponsor,
            parent_node_id: parent_id,
            leg,
            enrollment_id: ctx.enrollment_id,
        };
        state.nodes.push(node.clone());
        state.user_to_node.insert(user_id, new_id);

        result.emit(
            EventType::BinaryNodePlaced,
            json!({
                "node_id": new_id,
                "user_id": user_id,
                "parent_node_id": parent_id,
                "leg": leg.map(|l| match l { BinaryLeg::Left => "left", BinaryLeg::Right => "right" }),
                "strategy": strategy_name(self.config.strategy),
            }),
        );

        result.set_state(
            serde_json::to_value(BinaryTreeState::from_nodes(state.nodes.clone()))
                .map_err(|e| CoreError::Validation(e.to_string()))?,
        );

        Ok(result)
    }
}

fn strategy_name(s: BinaryPlacementStrategy) -> &'static str {
    match s {
        BinaryPlacementStrategy::Manual => "manual",
        BinaryPlacementStrategy::SponsorPreference => "sponsor_preference",
        BinaryPlacementStrategy::AutoBalance => "auto_balance",
        BinaryPlacementStrategy::OutsideLegPreference => "outside_leg_preference",
    }
}

fn pick_placement(
    state: &BinaryTreeState,
    sponsor: Option<UserId>,
    strategy: BinaryPlacementStrategy,
) -> (Option<BinaryNodeId>, Option<BinaryLeg>) {
    if state.nodes.is_empty() {
        return (None, None);
    }
    match strategy {
        BinaryPlacementStrategy::Manual => (None, None),
        BinaryPlacementStrategy::SponsorPreference => sponsor
            .and_then(|s| state.user_to_node.get(&s).copied())
            .map(|pid| (Some(pid), first_open_leg(state, pid)))
            .unwrap_or((state.nodes.first().map(|n| n.id), Some(BinaryLeg::Left))),
        BinaryPlacementStrategy::AutoBalance => pick_autobalance(state),
        BinaryPlacementStrategy::OutsideLegPreference => pick_outside_leg(state),
    }
}

fn pick_autobalance(state: &BinaryTreeState) -> (Option<BinaryNodeId>, Option<BinaryLeg>) {
    // Find the shallowest node whose leg-counts are imbalanced.
    let mut counts: BTreeMap<BinaryNodeId, (i32, i32)> = BTreeMap::new();
    for n in &state.nodes {
        if let (Some(pid), Some(leg)) = (n.parent_node_id, n.leg) {
            let entry = counts.entry(pid).or_insert((0, 0));
            match leg {
                BinaryLeg::Left => entry.0 += 1,
                BinaryLeg::Right => entry.1 += 1,
            }
        }
    }
    let candidate = counts
        .iter()
        .filter(|(_, (l, r))| l != r)
        .min_by_key(|(id, _)| depth_of(state, **id))
        .map(|(id, _)| *id);
    if let Some(pid) = candidate {
        let (l, r) = counts[&pid];
        let leg = if l <= r {
            BinaryLeg::Left
        } else {
            BinaryLeg::Right
        };
        return (Some(pid), Some(leg));
    }
    // All balanced: open a new left leg under the deepest node.
    if let Some(deepest) = state.nodes.iter().max_by_key(|n| depth_of(state, n.id)) {
        return (Some(deepest.id), Some(BinaryLeg::Left));
    }
    (None, None)
}

fn pick_outside_leg(state: &BinaryTreeState) -> (Option<BinaryNodeId>, Option<BinaryLeg>) {
    // Prefer the leftmost available leaf that has an open right leg, else fall back to autobalance.
    let mut by_id: BTreeMap<BinaryNodeId, (i32, i32)> = BTreeMap::new();
    for n in &state.nodes {
        if let (Some(pid), Some(leg)) = (n.parent_node_id, n.leg) {
            let entry = by_id.entry(pid).or_insert((0, 0));
            match leg {
                BinaryLeg::Left => entry.0 += 1,
                BinaryLeg::Right => entry.1 += 1,
            }
        }
    }
    let outside = by_id.iter().find(|(_, (l, _r))| *l == 0);
    if let Some((pid, _)) = outside {
        return (Some(*pid), Some(BinaryLeg::Right));
    }
    pick_autobalance(state)
}

fn first_open_leg(state: &BinaryTreeState, parent: BinaryNodeId) -> Option<BinaryLeg> {
    let mut has_left = false;
    let mut has_right = false;
    for n in &state.nodes {
        if n.parent_node_id == Some(parent) {
            match n.leg {
                Some(BinaryLeg::Left) => has_left = true,
                Some(BinaryLeg::Right) => has_right = true,
                None => {}
            }
        }
    }
    if !has_left {
        Some(BinaryLeg::Left)
    } else if !has_right {
        Some(BinaryLeg::Right)
    } else {
        None
    }
}

fn depth_of(state: &BinaryTreeState, id: BinaryNodeId) -> i32 {
    let mut d = 0;
    let mut cur = state.nodes.iter().find(|n| n.id == id);
    while let Some(node) = cur {
        if let Some(pid) = node.parent_node_id {
            d += 1;
            cur = state.nodes.iter().find(|n| n.id == pid);
        } else {
            break;
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_node_is_root() {
        let state = BinaryTreeState::default();
        let (parent, leg) = pick_placement(&state, None, BinaryPlacementStrategy::AutoBalance);
        assert!(parent.is_none());
        assert!(leg.is_none());
    }

    #[test]
    fn autobalance_fills_left_then_right() {
        let root_id = BinaryNodeId::new();
        let state = BinaryTreeState::from_nodes(vec![BinaryNode {
            id: root_id,
            user_id: UserId::new(),
            sponsor_user_id: None,
            parent_node_id: None,
            leg: None,
            enrollment_id: None,
        }]);
        let (p, l) = pick_autobalance(&state);
        assert_eq!(p, Some(root_id));
        assert_eq!(l, Some(BinaryLeg::Left));
    }
}
