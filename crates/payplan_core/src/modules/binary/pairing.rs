use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryPairingConfig {
    /// Number of left-volume units required to match one right unit.
    pub left_ratio: u32,
    /// Number of right-volume units required to match one left unit.
    pub right_ratio: u32,
    /// Payout percentage (0..=100) applied to matched volume to calculate points.
    pub payout_percent: u8,
    /// Optional cap on points payout per cycle.
    pub max_points_per_cycle: Option<i64>,
}

impl Default for BinaryPairingConfig {
    fn default() -> Self {
        Self {
            left_ratio: 1,
            right_ratio: 1,
            payout_percent: 10,
            max_points_per_cycle: None,
        }
    }
}

/// Result of running a pairing computation for one node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryPairingOutcome {
    pub matched_volume: i64,
    pub points: i64,
    pub capped: bool,
}

/// Compute the matched volume and points for one cycle.
#[must_use]
pub fn compute_pairing(left: i64, right: i64, cfg: &BinaryPairingConfig) -> BinaryPairingOutcome {
    debug_assert!(cfg.left_ratio > 0 && cfg.right_ratio > 0);
    debug_assert!(cfg.payout_percent <= 100);

    // Effective volume on each leg after applying the ratio.
    let left_eff = left / i64::from(cfg.left_ratio);
    let right_eff = right / i64::from(cfg.right_ratio);
    let matched = left_eff.min(right_eff).max(0);

    let mut points = matched * i64::from(cfg.payout_percent) / 100;
    let mut capped = false;
    if let Some(cap) = cfg.max_points_per_cycle {
        if points > cap {
            points = cap;
            capped = true;
        }
    }

    BinaryPairingOutcome {
        matched_volume: matched,
        points,
        capped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_legs_match_full_volume() {
        let cfg = BinaryPairingConfig::default();
        let out = compute_pairing(100, 100, &cfg);
        assert_eq!(out.matched_volume, 100);
        assert_eq!(out.points, 10);
        assert!(!out.capped);
    }

    #[test]
    fn min_leg_bounds_match() {
        let cfg = BinaryPairingConfig::default();
        let out = compute_pairing(100, 40, &cfg);
        assert_eq!(out.matched_volume, 40);
        assert_eq!(out.points, 4);
    }

    #[test]
    fn ratio_scales_legs() {
        let cfg = BinaryPairingConfig {
            left_ratio: 2,
            right_ratio: 1,
            ..Default::default()
        };
        let out = compute_pairing(100, 50, &cfg);
        // left_eff = 100/2 = 50, right_eff = 50/1 = 50 => matched 50
        assert_eq!(out.matched_volume, 50);
    }

    #[test]
    fn cap_limits_points() {
        let cfg = BinaryPairingConfig {
            max_points_per_cycle: Some(5),
            ..Default::default()
        };
        let out = compute_pairing(100, 100, &cfg);
        assert_eq!(out.matched_volume, 100);
        assert_eq!(out.points, 5);
        assert!(out.capped);
    }
}
