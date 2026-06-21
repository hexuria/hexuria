use async_trait::async_trait;
use chrono::{DateTime, Utc};
use payplan_app::error::{AppError, AppResult};
use payplan_app::ports::{CompanyRepo, UserRepo};
use payplan_core::platform::company::{Company, CompanyStatus};
use payplan_core::platform::user::{User, UserRole};
use payplan_core::shared::ids::{CompanyId, UserId};
use serde_json::Value;
use sqlx::{PgConnection, Row};

#[derive(Default)]
pub struct PgCompanyRepo {}

impl PgCompanyRepo {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl CompanyRepo for PgCompanyRepo {
    async fn insert(&self, company: &Company, conn: &mut PgConnection) -> AppResult<()> {
        let status = match company.status {
            CompanyStatus::Active => "active",
            CompanyStatus::Suspended => "suspended",
            CompanyStatus::Archived => "archived",
        };
        sqlx::query(
            r#"INSERT INTO companies (id, name, slug, status, settings, created_at)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
        )
        .bind(company.id)
        .bind(&company.name)
        .bind(&company.slug)
        .bind(status)
        .bind(&company.settings)
        .bind(company.created_at)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        Ok(())
    }

    async fn get(&self, id: CompanyId, conn: &mut PgConnection) -> AppResult<Option<Company>> {
        let row = sqlx::query(
            r#"SELECT id, name, slug, status, settings, created_at FROM companies WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        row.map(row_to_company).transpose()
    }

    async fn list(&self, conn: &mut PgConnection) -> AppResult<Vec<Company>> {
        let rows = sqlx::query(
            r#"SELECT id, name, slug, status, settings, created_at FROM companies ORDER BY created_at DESC"#,
        )
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        rows.into_iter().map(row_to_company).collect()
    }
}

fn row_to_company(row: sqlx::postgres::PgRow) -> AppResult<Company> {
    let id: uuid::Uuid = row
        .try_get("id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let name: String = row
        .try_get("name")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let slug: String = row
        .try_get("slug")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let status: String = row
        .try_get("status")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let settings: Value = row
        .try_get("settings")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let created_at: DateTime<Utc> = row
        .try_get("created_at")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    Ok(Company {
        id: CompanyId::from(id),
        name,
        slug,
        status: parse_company_status(&status)?,
        settings,
        created_at,
    })
}

fn parse_company_status(s: &str) -> AppResult<CompanyStatus> {
    match s {
        "active" => Ok(CompanyStatus::Active),
        "suspended" => Ok(CompanyStatus::Suspended),
        "archived" => Ok(CompanyStatus::Archived),
        other => Err(AppError::Validation(format!(
            "unknown company status: {other}"
        ))),
    }
}

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
            UserRole::CompanyAdmin => "company_admin",
            UserRole::PlatformAdmin => "platform_admin",
        };
        sqlx::query(
            r#"INSERT INTO users (id, email, password_hash, email_verified, role, company_id, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
        )
        .bind(user.id)
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(user.email_verified)
        .bind(role)
        .bind(user.company_id)
        .bind(user.created_at)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        Ok(())
    }

    async fn get(&self, id: UserId, conn: &mut PgConnection) -> AppResult<Option<User>> {
        let row = sqlx::query(
            r#"SELECT id, email, password_hash, email_verified, role, company_id, created_at
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
            r#"SELECT id, email, password_hash, email_verified, role, company_id, created_at
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
    let company_id: Option<uuid::Uuid> = row
        .try_get("company_id")
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
        company_id: company_id.map(CompanyId::from),
        created_at,
    })
}

fn parse_user_role(s: &str) -> AppResult<UserRole> {
    match s {
        "user" => Ok(UserRole::User),
        "company_admin" => Ok(UserRole::CompanyAdmin),
        "platform_admin" => Ok(UserRole::PlatformAdmin),
        other => Err(AppError::Validation(format!("unknown user role: {other}"))),
    }
}
