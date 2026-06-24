use serde::{Deserialize, Serialize};

use crate::shared::ids::{BinaryNodeId, PurchaseId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryVolumeConfig {
    pub count_purchase_volume: bool,
    pub count_renewal_volume: bool,
    pub carryover_enabled: bool,
}

impl Default for BinaryVolumeConfig {
    fn default() -> Self {
        Self {
            count_purchase_volume: true,
            count_renewal_volume: true,
            carryover_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryVolumeEntry {
    pub id: uuid::Uuid,
    pub source_purchase_id: Option<PurchaseId>,
    pub leg: crate::modules::binary::tree::BinaryLeg,
    pub volume: i64,
    pub status: BinaryVolumeStatus,
    /// Node the volume applies to. Populated when the triggering event carries
    /// `node_id` (e.g. `BinaryNodePlaced`); otherwise `None` and the volume
    /// projector skips the row until node linkage is resolved (Track B).
    #[serde(default)]
    pub node_id: Option<BinaryNodeId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryVolumeStatus {
    Open,
    Matched,
    Carried,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BinaryLegTotals {
    pub left: i64,
    pub right: i64,
}

impl BinaryLegTotals {
    #[must_use]
    pub fn add(&self, leg: crate::modules::binary::tree::BinaryLeg, v: i64) -> Self {
        let mut next = self.clone();
        match leg {
            crate::modules::binary::tree::BinaryLeg::Left => next.left += v,
            crate::modules::binary::tree::BinaryLeg::Right => next.right += v,
        }
        next
    }

    #[must_use]
    pub fn matched_pairs(&self) -> i64 {
        self.left.min(self.right)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::binary::tree::BinaryLeg;

    #[test]
    fn default_config_volume_on_with_carryover() {
        let c = BinaryVolumeConfig::default();
        assert!(c.count_purchase_volume);
        assert!(c.count_renewal_volume);
        assert!(c.carryover_enabled);
    }

    #[test]
    fn matched_pairs_is_min_of_legs() {
        let t = BinaryLegTotals {
            left: 100,
            right: 40,
        };
        assert_eq!(t.matched_pairs(), 40);

        let t = t.add(BinaryLeg::Left, 50);
        assert_eq!(t.matched_pairs(), 40);

        let t = t.add(BinaryLeg::Right, 100);
        assert_eq!(t.matched_pairs(), 140);
    }
}
