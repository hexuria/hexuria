use serde::{Deserialize, Serialize};

use crate::shared::ids::{BinaryNodeId, CompanyId, EnrollmentId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BinaryLeg {
    Left,
    Right,
}

impl BinaryLeg {
    #[must_use]
    pub fn opposite(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BinaryPlacementStrategy {
    Manual,
    SponsorPreference,
    #[default]
    AutoBalance,
    OutsideLegPreference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryNode {
    pub id: BinaryNodeId,
    pub user_id: UserId,
    pub sponsor_user_id: Option<UserId>,
    pub parent_node_id: Option<BinaryNodeId>,
    pub leg: Option<BinaryLeg>,
    /// Owning company. Populated by the tree module from `ctx.company_id` so the
    /// projector can materialize `binary_nodes` without a DB lookup.
    #[serde(default)]
    pub company_id: Option<CompanyId>,
    /// Enrollment that triggered placement. Populated by the tree module from
    /// `ctx.enrollment_id`.
    #[serde(default)]
    pub enrollment_id: Option<EnrollmentId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposite_leg() {
        assert_eq!(BinaryLeg::Left.opposite(), BinaryLeg::Right);
        assert_eq!(BinaryLeg::Right.opposite(), BinaryLeg::Left);
    }

    #[test]
    fn default_strategy_is_autobalance() {
        assert_eq!(
            BinaryPlacementStrategy::default(),
            BinaryPlacementStrategy::AutoBalance
        );
    }
}
