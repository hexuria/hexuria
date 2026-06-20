use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::ids::{CompanyId, RoyalAccountId, RoyalMatrixId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RoyalSlot {
    S1,
    S2,
    S3,
    S4,
    S5,
    S6,
    S7,
}

impl RoyalSlot {
    #[must_use]
    pub fn from_number(n: u8) -> Option<Self> {
        match n {
            1 => Some(Self::S1),
            2 => Some(Self::S2),
            3 => Some(Self::S3),
            4 => Some(Self::S4),
            5 => Some(Self::S5),
            6 => Some(Self::S6),
            7 => Some(Self::S7),
            _ => None,
        }
    }

    #[must_use]
    pub fn number(self) -> u8 {
        match self {
            Self::S1 => 1,
            Self::S2 => 2,
            Self::S3 => 3,
            Self::S4 => 4,
            Self::S5 => 5,
            Self::S6 => 6,
            Self::S7 => 7,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoyalMatrixStatus {
    Filling,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoyalMatrix {
    pub id: RoyalMatrixId,
    pub company_id: CompanyId,
    pub owner_account_id: RoyalAccountId,
    pub slots: Vec<Option<RoyalAccountId>>,
    pub status: RoyalMatrixStatus,
    pub cycle_count: u32,
    pub created_at: DateTime<Utc>,
    pub cycled_at: Option<DateTime<Utc>>,
}

impl RoyalMatrix {
    /// Slot 1 is owner; slots 2..=7 are fill slots.
    pub const SLOT_COUNT: usize = 7;

    #[must_use]
    pub fn new(company_id: CompanyId, owner: RoyalAccountId) -> Self {
        let mut slots: Vec<Option<RoyalAccountId>> = vec![None; Self::SLOT_COUNT];
        slots[0] = Some(owner);
        Self {
            id: RoyalMatrixId::new(),
            company_id,
            owner_account_id: owner,
            slots,
            status: RoyalMatrixStatus::Filling,
            cycle_count: 0,
            created_at: Utc::now(),
            cycled_at: None,
        }
    }

    #[must_use]
    pub fn filled_count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    #[must_use]
    pub fn is_full(&self) -> bool {
        self.slots.iter().all(Option::is_some)
    }

    /// Place the given account into the lowest open fill slot (sponsor-first logic is caller's job).
    /// Returns the slot it was placed in, or `None` if the matrix is full.
    pub fn place_next_open(&mut self, account_id: RoyalAccountId) -> Option<RoyalSlot> {
        if self.is_full() {
            return None;
        }
        // Slot 0 is owner; fill slots 1..=6 (slots 2..=7 in 1-indexed).
        let pos = self.slots.iter().position(Option::is_none)?;
        self.slots[pos] = Some(account_id);
        if self.is_full() {
            self.status = RoyalMatrixStatus::Completed;
        }
        RoyalSlot::from_number((pos as u8) + 1)
    }

    /// Mark the matrix as completed and bump cycle counter.
    pub fn cycle(&mut self) {
        if self.is_full() {
            self.status = RoyalMatrixStatus::Completed;
            self.cycle_count = self.cycle_count.saturating_add(1);
            self.cycled_at = Some(Utc::now());
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoyalMatrixConfig {
    /// When true, the matrix cycles automatically when full.
    pub auto_cycle: bool,
}

impl Default for RoyalMatrixConfig {
    fn default() -> Self {
        Self { auto_cycle: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::royal::flushline::RoyalFlushlineAccount;

    fn owner() -> RoyalAccountId {
        RoyalFlushlineAccount::new(
            CompanyId::new(),
            crate::shared::ids::EnrollmentId::new(),
            crate::shared::ids::UserId::new(),
        )
        .id
    }

    fn account_id() -> RoyalAccountId {
        RoyalAccountId::new()
    }

    #[test]
    fn new_matrix_has_owner_in_slot_1() {
        let m = RoyalMatrix::new(CompanyId::new(), owner());
        assert_eq!(m.slots[0], Some(m.owner_account_id));
        assert!(m.slots[1..].iter().all(Option::is_none));
        assert_eq!(m.filled_count(), 1);
        assert_eq!(m.status, RoyalMatrixStatus::Filling);
        assert!(!m.is_full());
    }

    #[test]
    fn places_into_lowest_open_slot() {
        let mut m = RoyalMatrix::new(CompanyId::new(), owner());
        let a = account_id();
        let slot = m.place_next_open(a);
        assert_eq!(slot, Some(RoyalSlot::S2));
        assert_eq!(m.slots[1], Some(a));
        assert_eq!(m.filled_count(), 2);
    }

    #[test]
    fn completing_all_slots_marks_completed() {
        let mut m = RoyalMatrix::new(CompanyId::new(), owner());
        for _ in 0..6 {
            assert!(m.place_next_open(account_id()).is_some());
        }
        assert!(m.is_full());
        assert_eq!(m.status, RoyalMatrixStatus::Completed);
        assert!(m.place_next_open(account_id()).is_none());
    }

    #[test]
    fn cycle_bumps_counter() {
        let mut m = RoyalMatrix::new(CompanyId::new(), owner());
        for _ in 0..6 {
            m.place_next_open(account_id());
        }
        m.cycle();
        assert_eq!(m.cycle_count, 1);
        assert!(m.cycled_at.is_some());
    }

    #[test]
    fn slot_numbers_roundtrip() {
        for n in 1u8..=7 {
            assert_eq!(RoyalSlot::from_number(n).unwrap().number(), n);
        }
        assert!(RoyalSlot::from_number(0).is_none());
        assert!(RoyalSlot::from_number(8).is_none());
    }
}
