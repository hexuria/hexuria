//! Optional sqlx support for core types.
//!
//! Enabled via the `sqlx` feature flag on this crate. Keeps `payplan_core`
//! free of an unconditional sqlx dependency.

#![cfg(feature = "sqlx")]

use sqlx::postgres::{PgArgumentBuffer, PgTypeInfo, PgValueRef};
use sqlx::{Decode, Encode, Postgres, Type};

use crate::shared::ids::{
    BillingPlanId, BinaryNodeId, CatalogItemId, EnrollmentId, EntitlementId, EventId,
    LedgerEntryId, PackageId, PayPlanStackId, ProductPayPlanAllocationId, PurchaseId,
    RoyalAccountId, RoyalMatrixId, SubscriptionId, UserId,
};

macro_rules! sqlx_transparent {
    ($name:ident) => {
        impl Type<Postgres> for $name {
            fn type_info() -> PgTypeInfo {
                <uuid::Uuid as Type<Postgres>>::type_info()
            }
            fn compatible(ty: &PgTypeInfo) -> bool {
                <uuid::Uuid as Type<Postgres>>::compatible(ty)
            }
        }

        impl<'r> Decode<'r, Postgres> for $name {
            fn decode(value: PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
                let inner = <uuid::Uuid as Decode<Postgres>>::decode(value)?;
                Ok(Self(inner))
            }
        }

        impl<'q> Encode<'q, Postgres> for $name {
            fn encode_by_ref(
                &self,
                buf: &mut PgArgumentBuffer,
            ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
                <uuid::Uuid as Encode<Postgres>>::encode_by_ref(&self.0, buf)
            }
        }
    };
}
sqlx_transparent!(UserId);
sqlx_transparent!(CatalogItemId);
sqlx_transparent!(BillingPlanId);
sqlx_transparent!(PackageId);
sqlx_transparent!(PurchaseId);
sqlx_transparent!(SubscriptionId);
sqlx_transparent!(EntitlementId);
sqlx_transparent!(EnrollmentId);
sqlx_transparent!(PayPlanStackId);
sqlx_transparent!(LedgerEntryId);
sqlx_transparent!(EventId);
sqlx_transparent!(RoyalAccountId);
sqlx_transparent!(RoyalMatrixId);
sqlx_transparent!(BinaryNodeId);
sqlx_transparent!(ProductPayPlanAllocationId);

// Money's currency is a String, amount is Decimal. sqlx supports both natively
// when the `rust_decimal` feature is enabled.

use rust_decimal::Decimal;

#[derive(sqlx::FromRow, Debug, Clone)]
pub struct MoneyRow {
    pub amount: Decimal,
    pub currency: String,
}
