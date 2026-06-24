use serde::{Deserialize, Serialize};

use crate::shared::ids::BinaryNodeId;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BinaryCarryover {
    pub left_volume: i64,
    pub right_volume: i64,
    /// Node the carryover applies to. Resolved via the `BinaryCycleClosed`
    /// event payload's `node_id` (Track B); `None` until then and the
    /// projector skips the row.
    #[serde(default)]
    pub node_id: Option<BinaryNodeId>,
}

impl BinaryCarryover {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply a carryover from the previous cycle: any unmatched volume stays for next cycle.
    #[must_use]
    pub fn from_unmatched(left: i64, right: i64) -> Self {
        Self {
            left_volume: left.max(0),
            right_volume: right.max(0),
            // Keys are populated by the caller (the carryover module) from
            // `ctx`/event payload after construction.
            node_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_negative_inputs_to_zero() {
        let c = BinaryCarryover::from_unmatched(-3, 5);
        assert_eq!(c.left_volume, 0);
        assert_eq!(c.right_volume, 5);
    }

    #[test]
    fn carries_unmatched_volume() {
        // After pairing 50 matched, unmatched: left=0, right=30
        let c = BinaryCarryover::from_unmatched(0, 30);
        assert_eq!(c.left_volume, 0);
        assert_eq!(c.right_volume, 30);
    }
}
