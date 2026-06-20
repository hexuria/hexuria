use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            #[must_use]
            pub fn from_uuid(id: Uuid) -> Self {
                Self(id)
            }

            #[must_use]
            pub fn nil() -> Self {
                Self(Uuid::nil())
            }

            #[must_use]
            pub fn is_nil(&self) -> bool {
                self.0.is_nil()
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }

        impl From<Uuid> for $name {
            fn from(id: Uuid) -> Self {
                Self(id)
            }
        }
    };
}

id_type!(CompanyId);
id_type!(UserId);
id_type!(CatalogItemId);
id_type!(BillingPlanId);
id_type!(PackageId);
id_type!(PurchaseId);
id_type!(SubscriptionId);
id_type!(EntitlementId);
id_type!(EnrollmentId);
id_type!(PayPlanStackId);
id_type!(LedgerEntryId);
id_type!(EventId);
id_type!(RoyalAccountId);
id_type!(RoyalMatrixId);
id_type!(BinaryNodeId);
