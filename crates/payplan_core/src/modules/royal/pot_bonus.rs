use serde::{Deserialize, Serialize};

use crate::shared::ids::UserId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoyalPotBonusConfig {
    /// Percentage (0..=100) of the pool distributed equally to qualified users.
    pub profit_share_percent: u8,
    /// Percentage (0..=100) of the pool distributed to top cyclers.
    pub top_cycler_percent: u8,
    /// Weights for top cycler slots (must sum to 100 ideally).
    pub top_cycler_weights: Vec<u8>,
}

impl Default for RoyalPotBonusConfig {
    fn default() -> Self {
        Self {
            profit_share_percent: 75,
            top_cycler_percent: 25,
            top_cycler_weights: vec![40, 30, 20, 10],
        }
    }
}

/// User-level qualification: PRD §7.3 requires BOTH a Flushline graduation AND a Matrix cycle.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoyalQualification {
    pub user_id: UserId,
    pub total_graduations: u32,
    pub total_matrix_cycles: u32,
    pub is_qualified: bool,
}

impl RoyalQualification {
    #[must_use]
    pub fn new(user_id: UserId) -> Self {
        Self {
            user_id,
            ..Self::default()
        }
    }

    pub fn record_graduation(&mut self) {
        self.total_graduations = self.total_graduations.saturating_add(1);
        self.recompute();
    }

    pub fn record_matrix_cycle(&mut self) {
        self.total_matrix_cycles = self.total_matrix_cycles.saturating_add(1);
        self.recompute();
    }

    fn recompute(&mut self) {
        self.is_qualified = self.total_graduations > 0 && self.total_matrix_cycles > 0;
    }
}

/// Outcome of distributing a pot bonus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoyalPotDistribution {
    pub profit_share_total: i64,
    pub per_qualified_user: i64,
    pub qualified_user_count: u32,
    pub top_cycler_payouts: Vec<i64>,
}

/// Split a pool of points according to the config.
///
/// Returns `None` if there are no qualified users AND no top cyclers (no distribution possible).
#[must_use]
pub fn distribute(
    pool: i64,
    config: &RoyalPotBonusConfig,
    qualified_users: u32,
) -> Option<RoyalPotDistribution> {
    debug_assert!(config.profit_share_percent + config.top_cycler_percent <= 100);

    let profit_share_total = pool * i64::from(config.profit_share_percent) / 100;
    let top_cycler_total = pool * i64::from(config.top_cycler_percent) / 100;

    let per_qualified_user = if qualified_users > 0 {
        profit_share_total / i64::from(qualified_users)
    } else {
        0
    };

    let weight_sum: u32 = config
        .top_cycler_weights
        .iter()
        .map(|&w| u32::from(w))
        .sum();
    let top_cycler_payouts: Vec<i64> = if weight_sum == 0 {
        vec![]
    } else {
        config
            .top_cycler_weights
            .iter()
            .map(|&w| top_cycler_total * i64::from(w) / i64::from(weight_sum))
            .collect()
    };

    if qualified_users == 0 && top_cycler_payouts.is_empty() {
        return None;
    }

    Some(RoyalPotDistribution {
        profit_share_total,
        per_qualified_user,
        qualified_user_count: qualified_users,
        top_cycler_payouts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_75_25() {
        let c = RoyalPotBonusConfig::default();
        assert_eq!(c.profit_share_percent, 75);
        assert_eq!(c.top_cycler_percent, 25);
        assert_eq!(c.top_cycler_weights, vec![40, 30, 20, 10]);
    }

    #[test]
    fn qualification_requires_both() {
        let mut q = RoyalQualification::new(UserId::new());
        assert!(!q.is_qualified);
        q.record_graduation();
        assert!(!q.is_qualified);
        q.record_matrix_cycle();
        assert!(q.is_qualified);
    }

    #[test]
    fn distribute_no_qualified_users() {
        let cfg = RoyalPotBonusConfig::default();
        let d = distribute(1000, &cfg, 0).expect("some distribution");
        assert_eq!(d.per_qualified_user, 0);
        assert_eq!(d.qualified_user_count, 0);
        // top cycler weights still paid: 40% of 250 = 100, 30% = 75, 20% = 50, 10% = 25
        assert_eq!(
            d.top_cycler_payouts,
            vec![100, 75, 50, 25]
        );
    }

    #[test]
    fn distribute_splits_correctly() {
        let cfg = RoyalPotBonusConfig::default();
        let d = distribute(1000, &cfg, 4).expect("some distribution");
        assert_eq!(d.profit_share_total, 750);
        assert_eq!(d.per_qualified_user, 187); // 750 / 4 = 187 (floored integer)
        assert_eq!(d.top_cycler_payouts.len(), 4);
    }

    #[test]
    fn distribute_returns_none_when_nothing_to_pay() {
        let cfg = RoyalPotBonusConfig {
            profit_share_percent: 0,
            top_cycler_percent: 0,
            top_cycler_weights: vec![],
        };
        assert!(distribute(1000, &cfg, 0).is_none());
    }
}
