use serde::{Deserialize, Serialize};

/// Duplication is gated on BOTH a Flushline graduation AND a Matrix cycle (PRD §7.4).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoyalDuplicationState {
    pub flushline_graduated: bool,
    pub matrix_cycled: bool,
}

impl RoyalDuplicationState {
    #[must_use]
    pub fn is_ready_to_duplicate(&self) -> bool {
        self.flushline_graduated && self.matrix_cycled
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoyalDuplicationConfig {
    /// When true, the workflow emits a new Royal account and assigns the configured sponsor.
    pub enabled: bool,
}

impl Default for RoyalDuplicationConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_both_signals() {
        let mut s = RoyalDuplicationState::default();
        assert!(!s.is_ready_to_duplicate());
        s.flushline_graduated = true;
        assert!(!s.is_ready_to_duplicate());
        s.matrix_cycled = true;
        assert!(s.is_ready_to_duplicate());
    }
}
