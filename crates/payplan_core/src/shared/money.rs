use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    pub amount: Decimal,
    pub currency: String,
}

impl Money {
    pub fn new(amount: Decimal, currency: impl Into<String>) -> Self {
        Self {
            amount,
            currency: currency.into(),
        }
    }

    pub fn zero(currency: impl Into<String>) -> Self {
        Self::new(Decimal::ZERO, currency)
    }

    pub fn is_zero(&self) -> bool {
        self.amount.is_zero()
    }
}
