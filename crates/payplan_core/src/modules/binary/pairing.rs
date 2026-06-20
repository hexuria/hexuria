use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryPairingConfig {
    /// Number of left-volume units required to match one right unit.
    pub left_ratio: u32,
    /// Number of right-volume units required to match one left unit.
    pub right_ratio: u32,
    /// Commission percentage (0..=100) applied to matched volume.
    pub commission_percent: u8,
    /// Optional cap (in minor currency units) on payout per cycle.
    pub max_payout_amount_minor: Option<i64>,
}

impl Default for BinaryPairingConfig {
    fn default() -> Self {
        Self {
            left_ratio: 1,
            right_ratio: 1,
            commission_percent: 10,
            max_payout_amount_minor: None,
        }
    }
}

/// Result of running a pairing computation for one node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryPairingOutcome {
    pub matched_volume: i64,
    pub commission: Decimal,
    pub capped: bool,
}

/// Compute the matched volume and commission for one cycle.
#[must_use]
pub fn compute_pairing(left: i64, right: i64, cfg: &BinaryPairingConfig) -> BinaryPairingOutcome {
    debug_assert!(cfg.left_ratio > 0 && cfg.right_ratio > 0);
    debug_assert!(cfg.commission_percent <= 100);

    // Effective volume on each leg after applying the ratio.
    let left_eff = left / i64::from(cfg.left_ratio);
    let right_eff = right / i64::from(cfg.right_ratio);
    let matched = left_eff.min(right_eff).max(0);

    let mut commission =
        Decimal::from(matched) * Decimal::from(cfg.commission_percent) / Decimal::from(100u32);
    let mut capped = false;
    if let Some(cap) = cfg.max_payout_amount_minor {
        let cap_d = Decimal::from(cap);
        if commission > cap_d {
            commission = cap_d;
            capped = true;
        }
    }

    BinaryPairingOutcome {
        matched_volume: matched,
        commission,
        capped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn equal_legs_match_full_volume() {
        let cfg = BinaryPairingConfig::default();
        let out = compute_pairing(100, 100, &cfg);
        assert_eq!(out.matched_volume, 100);
        assert_eq!(out.commission, dec!(10));
        assert!(!out.capped);
    }

    #[test]
    fn min_leg_bounds_match() {
        let cfg = BinaryPairingConfig::default();
        let out = compute_pairing(100, 40, &cfg);
        assert_eq!(out.matched_volume, 40);
        assert_eq!(out.commission, dec!(4));
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
    fn cap_limits_commission() {
        let cfg = BinaryPairingConfig {
            max_payout_amount_minor: Some(5),
            ..Default::default()
        };
        let out = compute_pairing(100, 100, &cfg);
        assert_eq!(out.matched_volume, 100);
        assert_eq!(out.commission, dec!(5));
        assert!(out.capped);
    }
}
