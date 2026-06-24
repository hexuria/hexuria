//! Constructors + validators for every domain entity.
//!
//! Invariants are encoded once here and reused by both the `handle_create_*`
//! command handlers and the API layer's request deserializers.

use crate::error::{CoreError, CoreResult};
use crate::platform::catalog::{
    BillingPlan, BillingType, CatalogItem, CatalogItemStatus, CatalogItemType, RecurrenceInterval,
    RecurringSettings,
};
use crate::platform::enrollment::Enrollment;
use crate::platform::entitlement::Entitlement;
use crate::platform::package::{Package, PackageItem, PackageStatus};
use crate::platform::purchase::{Purchase, PurchaseStatus};
use crate::platform::subscription::{Subscription, SubscriptionStatus};
use crate::platform::user::{User, UserRole};
use crate::shared::ids::{
    BillingPlanId, CatalogItemId, EnrollmentId, PackageId, PurchaseId, UserId,
};
use crate::shared::money::Money;
use chrono::Utc;
use rust_decimal::Decimal;
use uuid::Uuid;

/// Slug must match: lowercase letters, digits, single hyphens between segments, 2..=64 chars.
pub const SLUG_PATTERN: &str = r"^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$";
/// Currency codes are 3 uppercase ASCII letters (ISO 4217-ish).
pub const CURRENCY_PATTERN: &str = r"^[A-Z]{3}$";

pub fn validate_slug(slug: &str) -> CoreResult<()> {
    static RE: std::sync::OnceLock<regex_lite::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex_lite::Regex::new(SLUG_PATTERN).expect("valid slug regex"));
    if !re.is_match(slug) {
        return Err(CoreError::Validation(format!(
            "invalid slug '{slug}'; must be 2..=64 lowercase letters/digits/hyphens"
        )));
    }
    Ok(())
}

pub fn validate_currency(code: &str) -> CoreResult<()> {
    static RE: std::sync::OnceLock<regex_lite::Regex> = std::sync::OnceLock::new();
    let re =
        RE.get_or_init(|| regex_lite::Regex::new(CURRENCY_PATTERN).expect("valid currency regex"));
    if !re.is_match(code) {
        return Err(CoreError::Validation(format!(
            "invalid currency '{code}'; must be 3 uppercase ASCII letters"
        )));
    }
    Ok(())
}

// ------------------------------- User ----------------------------------------

impl User {
    pub fn new(
        email: impl Into<String>,
        password_hash: impl Into<String>,
        role: UserRole,
    ) -> CoreResult<Self> {
        let email = email.into();
        let password_hash = password_hash.into();
        if !email.contains('@') || email.len() < 3 {
            return Err(CoreError::Validation(format!("invalid email '{email}'")));
        }
        if password_hash.is_empty() {
            return Err(CoreError::Validation("password_hash is required".into()));
        }
        Ok(Self {
            id: UserId::new(),
            email,
            password_hash,
            email_verified: false,
            role,
            created_at: Utc::now(),
        })
    }

    pub fn validate(&self) -> CoreResult<()> {
        if !self.email.contains('@') || self.email.len() < 3 {
            return Err(CoreError::Validation(format!(
                "invalid email '{}'",
                self.email
            )));
        }
        if self.password_hash.is_empty() {
            return Err(CoreError::Validation("password_hash is required".into()));
        }
        Ok(())
    }
}

// ----------------------------- CatalogItem -----------------------------------

impl CatalogItem {
    pub fn new(
        name: impl Into<String>,
        item_type: CatalogItemType,
    ) -> CoreResult<Self> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(CoreError::Validation(
                "catalog item name is required".into(),
            ));
        }
        Ok(Self {
            id: CatalogItemId::new(),
            name,
            description: None,
            item_type,
            sku: None,
            status: CatalogItemStatus::Active,
            metadata: serde_json::json!({}),
            created_at: Utc::now(),
        })
    }

    pub fn validate(&self) -> CoreResult<()> {
        if self.name.trim().is_empty() {
            return Err(CoreError::Validation(
                "catalog item name is required".into(),
            ));
        }
        Ok(())
    }
}

// ------------------------------ BillingPlan ----------------------------------

impl BillingPlan {
    pub fn one_time(catalog_item_id: CatalogItemId, price: Money) -> CoreResult<Self> {
        Self::new(catalog_item_id, BillingType::OneTime, price, None)
    }

    pub fn recurring(
        catalog_item_id: CatalogItemId,
        price: Money,
        settings: RecurringSettings,
    ) -> CoreResult<Self> {
        Self::new(
            catalog_item_id,
            BillingType::Recurring,
            price,
            Some(settings),
        )
    }

    pub fn new(
        catalog_item_id: CatalogItemId,
        billing_type: BillingType,
        price: Money,
        recurring: Option<RecurringSettings>,
    ) -> CoreResult<Self> {
        if price.amount.is_sign_negative() {
            return Err(CoreError::Validation("price cannot be negative".into()));
        }
        validate_currency(&price.currency)?;
        if matches!(billing_type, BillingType::Recurring) && recurring.is_none() {
            return Err(CoreError::Validation(
                "recurring billing requires settings".into(),
            ));
        }
        if matches!(billing_type, BillingType::OneTime) && recurring.is_some() {
            return Err(CoreError::Validation(
                "one-time billing cannot have recurring settings".into(),
            ));
        }
        Ok(Self {
            id: BillingPlanId::new(),
            catalog_item_id,
            billing_type,
            price,
            recurring,
            active: true,
            created_at: Utc::now(),
        })
    }

    pub fn validate(&self) -> CoreResult<()> {
        if self.price.amount.is_sign_negative() {
            return Err(CoreError::Validation("price cannot be negative".into()));
        }
        validate_currency(&self.price.currency)?;
        Ok(())
    }
}

// ------------------------------- Package -------------------------------------

impl Package {
    pub fn new(
        name: impl Into<String>,
        items: Vec<PackageItem>,
    ) -> CoreResult<Self> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(CoreError::Validation("package name is required".into()));
        }
        if items.is_empty() {
            return Err(CoreError::Validation(
                "package must have at least one item".into(),
            ));
        }
        Ok(Self {
            id: PackageId::new(),
            name,
            description: None,
            status: PackageStatus::Draft,
            default_billing_plan_id: None,
            metadata: serde_json::json!({}),
            created_at: Utc::now(),
            items,
        })
    }

    pub fn validate(&self) -> CoreResult<()> {
        if self.name.trim().is_empty() {
            return Err(CoreError::Validation("package name is required".into()));
        }
        if self.items.is_empty() {
            return Err(CoreError::Validation(
                "package must have at least one item".into(),
            ));
        }
        for (idx, item) in self.items.iter().enumerate() {
            if item.quantity == 0 {
                return Err(CoreError::Validation(format!(
                    "package item {idx} has zero quantity"
                )));
            }
        }
        Ok(())
    }
}

// ------------------------------- Purchase ------------------------------------

impl Purchase {
    pub fn new(
        user_id: UserId,
        package_id: PackageId,
        gross: Money,
        sponsor_user_id: Option<UserId>,
    ) -> CoreResult<Self> {
        if gross.amount.is_sign_negative() {
            return Err(CoreError::Validation(
                "gross amount cannot be negative".into(),
            ));
        }
        validate_currency(&gross.currency)?;
        Ok(Self {
            id: PurchaseId::new(),
            user_id,
            package_id,
            sponsor_user_id,
            gross: gross.clone(),
            net: gross,
            status: PurchaseStatus::Pending,
            purchased_at: Utc::now(),
        })
    }

    pub fn validate(&self) -> CoreResult<()> {
        validate_currency(&self.gross.currency)?;
        if self.gross.amount.is_sign_negative() {
            return Err(CoreError::Validation(
                "gross amount cannot be negative".into(),
            ));
        }
        if self.net.amount.is_sign_negative() {
            return Err(CoreError::Validation(
                "net amount cannot be negative".into(),
            ));
        }
        if self.gross.currency != self.net.currency {
            return Err(CoreError::Validation(
                "gross and net currencies differ".into(),
            ));
        }
        Ok(())
    }
}

// ----------------------------- Subscription ----------------------------------

impl Subscription {
    pub fn new(
        user_id: UserId,
        package_id: PackageId,
        billing_plan_id: BillingPlanId,
    ) -> Self {
        Self {
            id: crate::shared::ids::SubscriptionId::new(),
            user_id,
            package_id,
            billing_plan_id,
            status: SubscriptionStatus::Active,
            current_period: None,
            cancelled_at: None,
            created_at: Utc::now(),
        }
    }
}

// ------------------------------ Enrollment -----------------------------------

impl Enrollment {
    pub fn new(
        user_id: UserId,
        package_id: PackageId,
        purchase_id: crate::shared::ids::PurchaseId,
        sponsor_user_id: Option<UserId>,
    ) -> Self {
        Self {
            id: EnrollmentId::new(),
            user_id,
            package_id,
            purchase_id,
            sponsor_user_id,
            status: crate::platform::enrollment::EnrollmentStatus::Active,
            joined_at: Utc::now(),
        }
    }
}

// ------------------------------ Entitlement ----------------------------------

impl Entitlement {
    pub fn new(
        user_id: UserId,
        package_id: PackageId,
        catalog_item_id: CatalogItemId,
        source_purchase_id: Option<crate::shared::ids::PurchaseId>,
        source_subscription_id: Option<crate::shared::ids::SubscriptionId>,
    ) -> Self {
        Self {
            id: crate::shared::ids::EntitlementId::new(),
            user_id,
            package_id,
            catalog_item_id,
            source_purchase_id,
            source_subscription_id,
            status: crate::platform::entitlement::EntitlementStatus::Active,
            starts_at: Utc::now(),
            ends_at: None,
            revoked_at: None,
        }
    }
}

// ----------------------------- RecurringSettings -----------------------------

impl RecurringSettings {
    pub fn new(
        interval: RecurrenceInterval,
        interval_count: u32,
        trial_days: u32,
        grace_period_days: u32,
    ) -> CoreResult<Self> {
        if interval_count == 0 {
            return Err(CoreError::Validation("interval_count must be > 0".into()));
        }
        Ok(Self {
            interval,
            interval_count,
            trial_days,
            grace_period_days,
        })
    }
}

// --------------------------- Decimal zero helpers ----------------------------

#[allow(dead_code)]
pub fn decimal_zero() -> Decimal {
    Decimal::ZERO
}

#[allow(dead_code)]
pub fn uuid_nil() -> Uuid {
    Uuid::nil()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::catalog::{RecurrenceInterval, RecurringSettings};

    #[test]
    fn slug_accepts_valid_patterns() {
        assert!(validate_slug("acme").is_ok());
        assert!(validate_slug("acme-corp").is_ok());
        assert!(validate_slug("a").is_ok());
        assert!(validate_slug("a1-b2-c3").is_ok());
    }

    #[test]
    fn slug_rejects_invalid_patterns() {
        assert!(validate_slug("").is_err());
        assert!(validate_slug("-leading").is_err());
        assert!(validate_slug("trailing-").is_err());
        assert!(validate_slug("UPPER").is_err());
        assert!(validate_slug("with space").is_err());
        assert!(validate_slug(&"a".repeat(65)).is_err());
    }

    #[test]
    fn currency_accepts_iso_4217() {
        assert!(validate_currency("USD").is_ok());
        assert!(validate_currency("EUR").is_ok());
        assert!(validate_currency("PHP").is_ok());
    }

    #[test]
    fn currency_rejects_invalid() {
        assert!(validate_currency("usd").is_err());
        assert!(validate_currency("US").is_err());
        assert!(validate_currency("USDD").is_err());
        assert!(validate_currency("US1").is_err());
    }

    #[test]
    fn user_rejects_invalid_email() {
        assert!(User::new("not-an-email", "hash", UserRole::User).is_err());
        assert!(User::new("", "hash", UserRole::User).is_err());
    }

    #[test]
    fn catalog_item_requires_name() {
        assert!(CatalogItem::new("", CatalogItemType::Service).is_err());
    }

    #[test]
    fn billing_plan_one_time_rejects_negative() {
        let money = Money::new(rust_decimal_macros::dec!(-1), "USD");
        assert!(BillingPlan::one_time(CatalogItemId::new(), money).is_err());
    }

    #[test]
    fn billing_plan_recurring_requires_settings() {
        let money = Money::new(rust_decimal_macros::dec!(10), "USD");
        assert!(
            BillingPlan::new(CatalogItemId::new(), BillingType::Recurring, money, None).is_err()
        );
    }

    #[test]
    fn billing_plan_one_time_rejects_settings() {
        let money = Money::new(rust_decimal_macros::dec!(10), "USD");
        let settings = RecurringSettings {
            interval: RecurrenceInterval::Monthly,
            interval_count: 1,
            trial_days: 0,
            grace_period_days: 0,
        };
        assert!(BillingPlan::new(
            CatalogItemId::new(),
            BillingType::OneTime,
            money,
            Some(settings)
        )
        .is_err());
    }

    #[test]
    fn package_rejects_empty_items() {
        assert!(Package::new("Starter", vec![]).is_err());
    }

    #[test]
    fn purchase_rejects_currency_mismatch() {
        let p = Purchase::new(
            UserId::new(),
            PackageId::new(),
            Money::new(rust_decimal_macros::dec!(10), "USD"),
            None,
        )
        .unwrap();
        // Mutate net currency to trigger mismatch
        let mut bad = p.clone();
        bad.net = Money::new(rust_decimal_macros::dec!(10), "EUR");
        assert!(bad.validate().is_err());
    }

    #[test]
    fn purchase_validate_rejects_negative_gross() {
        let p = Purchase::new(
            UserId::new(),
            PackageId::new(),
            Money::new(rust_decimal_macros::dec!(10), "USD"),
            None,
        )
        .unwrap();
        let mut bad = p.clone();
        bad.gross = Money::new(rust_decimal_macros::dec!(-1), "USD");
        bad.net = Money::new(rust_decimal_macros::dec!(-1), "USD");
        assert!(
            bad.validate().is_err(),
            "negative gross must fail validation"
        );
    }

    #[test]
    fn package_item_zero_quantity_is_invalid() {
        let pkg = Package::new(
            "Starter",
            vec![PackageItem {
                catalog_item_id: CatalogItemId::new(),
                billing_plan_id: BillingPlanId::new(),
                quantity: 0,
                role: crate::platform::package::PackageItemRole::Included,
                is_commissionable: false,
            }],
        )
        .unwrap();
        assert!(pkg.validate().is_err());
    }
}
