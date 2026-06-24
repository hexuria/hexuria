use async_trait::async_trait;
use chrono::{DateTime, Utc};
use payplan_app::error::{AppError, AppResult};
use payplan_app::ports::UserRepo;
use payplan_core::platform::catalog::ProductPayPlanAllocation;
use payplan_core::platform::user::{User, UserRole};
use payplan_core::shared::ids::{CatalogItemId, PayPlanStackId, ProductPayPlanAllocationId, UserId};
use sqlx::{PgConnection, Row};

#[derive(Default)]
pub struct PgUserRepo {}

impl PgUserRepo {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl UserRepo for PgUserRepo {
    async fn insert(&self, user: &User, conn: &mut PgConnection) -> AppResult<()> {
        let role = match user.role {
            UserRole::User => "user",
            UserRole::Admin => "admin",
        };
        sqlx::query(
            r#"INSERT INTO users (id, email, password_hash, email_verified, role, created_at)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
        )
        .bind(user.id)
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(user.email_verified)
        .bind(role)
        .bind(user.created_at)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        Ok(())
    }

    async fn get(&self, id: UserId, conn: &mut PgConnection) -> AppResult<Option<User>> {
        let row = sqlx::query(
            r#"SELECT id, email, password_hash, email_verified, role, created_at
               FROM users WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        row.map(row_to_user).transpose()
    }

    async fn find_by_email(&self, email: &str, conn: &mut PgConnection) -> AppResult<Option<User>> {
        let row = sqlx::query(
            r#"SELECT id, email, password_hash, email_verified, role, created_at
               FROM users WHERE email = $1"#,
        )
        .bind(email)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        row.map(row_to_user).transpose()
    }
}

fn row_to_user(row: sqlx::postgres::PgRow) -> AppResult<User> {
    let id: uuid::Uuid = row
        .try_get("id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let email: String = row
        .try_get("email")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let password_hash: String = row
        .try_get("password_hash")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let email_verified: bool = row
        .try_get("email_verified")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let role: String = row
        .try_get("role")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let created_at: DateTime<Utc> = row
        .try_get("created_at")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    Ok(User {
        id: UserId::from(id),
        email,
        password_hash,
        email_verified,
        role: parse_user_role(&role)?,
        created_at,
    })
}

fn parse_user_role(s: &str) -> AppResult<UserRole> {
    match s {
        "user" => Ok(UserRole::User),
        "admin" => Ok(UserRole::Admin),
        other => Err(AppError::Validation(format!("unknown user role: {other}"))),
    }
}

#[derive(Default)]
pub struct PgAllocationRepo {}

impl PgAllocationRepo {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl payplan_app::ports::AllocationRepo for PgAllocationRepo {
    async fn insert(&self, allocation: &ProductPayPlanAllocation, conn: &mut PgConnection) -> AppResult<()> {
        sqlx::query(
            r#"INSERT INTO product_payplan_allocations (id, catalog_item_id, pay_plan_stack_id, points, active, created_at)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
        )
        .bind(allocation.id)
        .bind(allocation.catalog_item_id)
        .bind(allocation.pay_plan_stack_id)
        .bind(allocation.points)
        .bind(allocation.active)
        .bind(allocation.created_at)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        Ok(())
    }

    async fn get(&self, id: ProductPayPlanAllocationId, conn: &mut PgConnection) -> AppResult<Option<ProductPayPlanAllocation>> {
        let row = sqlx::query(
            r#"SELECT id, catalog_item_id, pay_plan_stack_id, points, active, created_at
               FROM product_payplan_allocations WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        row.map(row_to_allocation).transpose()
    }

    async fn list_for_products(&self, product_ids: &[CatalogItemId], conn: &mut PgConnection) -> AppResult<Vec<ProductPayPlanAllocation>> {
        if product_ids.is_empty() {
            return Ok(vec![]);
        }
        let rows = sqlx::query(
            r#"SELECT id, catalog_item_id, pay_plan_stack_id, points, active, created_at
               FROM product_payplan_allocations WHERE catalog_item_id = ANY($1)"#,
        )
        .bind(product_ids.iter().map(|id| id.0).collect::<Vec<_>>())
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        rows.into_iter().map(row_to_allocation).collect()
    }

    async fn list_all(&self, conn: &mut PgConnection) -> AppResult<Vec<ProductPayPlanAllocation>> {
        let rows = sqlx::query(
            r#"SELECT id, catalog_item_id, pay_plan_stack_id, points, active, created_at
               FROM product_payplan_allocations ORDER BY created_at DESC"#,
        )
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        rows.into_iter().map(row_to_allocation).collect()
    }

    async fn delete(&self, id: ProductPayPlanAllocationId, conn: &mut PgConnection) -> AppResult<()> {
        sqlx::query("DELETE FROM product_payplan_allocations WHERE id = $1")
            .bind(id)
            .execute(&mut *conn)
            .await
            .map_err(|e| AppError::Infra(e.to_string()))?;
        Ok(())
    }
}

fn row_to_allocation(row: sqlx::postgres::PgRow) -> AppResult<ProductPayPlanAllocation> {
    let id: uuid::Uuid = row.try_get("id").map_err(|e| AppError::Infra(e.to_string()))?;
    let catalog_item_id: uuid::Uuid = row.try_get("catalog_item_id").map_err(|e| AppError::Infra(e.to_string()))?;
    let pay_plan_stack_id: uuid::Uuid = row.try_get("pay_plan_stack_id").map_err(|e| AppError::Infra(e.to_string()))?;
    let points: i64 = row.try_get("points").map_err(|e| AppError::Infra(e.to_string()))?;
    let active: bool = row.try_get("active").map_err(|e| AppError::Infra(e.to_string()))?;
    let created_at: DateTime<Utc> = row.try_get("created_at").map_err(|e| AppError::Infra(e.to_string()))?;
    Ok(ProductPayPlanAllocation {
        id: ProductPayPlanAllocationId::from(id),
        catalog_item_id: CatalogItemId::from(catalog_item_id),
        pay_plan_stack_id: PayPlanStackId::from(pay_plan_stack_id),
        points,
        active,
        created_at,
    })
}
