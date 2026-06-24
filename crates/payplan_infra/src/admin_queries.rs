use async_trait::async_trait;
use payplan_app::{
    error::{AppError, AppResult},
    queries::{
        AdminQueryService, BillingRow, CatalogRow, DashboardView, PackageRow, Page,
        PageRequest, PurchaseRow, UserRow,
    },
};
use payplan_core::shared::ids::UserId;
use sqlx::{PgPool, Row};

#[derive(Clone)]
pub struct PgAdminQueryService {
    pool: PgPool,
}

impl PgAdminQueryService {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AdminQueryService for PgAdminQueryService {
    async fn dashboard(&self) -> AppResult<DashboardView> {
        let counts = sqlx::query(
            r#"SELECT
                (SELECT COUNT(*) FROM users) AS users,
                (SELECT COUNT(*) FROM packages) AS packages,
                (SELECT COUNT(*) FROM purchases) AS purchases"#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(infra)?;
        let recent = self
            .purchases(
                PageRequest {
                    page: 1,
                    page_size: 8,
                    query: None,
                },
            )
            .await?;
        Ok(DashboardView {
            user_count: to_u64(counts.try_get::<i64, _>("users").map_err(infra)?),
            package_count: to_u64(counts.try_get::<i64, _>("packages").map_err(infra)?),
            purchase_count: to_u64(counts.try_get::<i64, _>("purchases").map_err(infra)?),
            recent_purchases: recent.items,
        })
    }

    async fn users(&self, request: PageRequest) -> AppResult<Page<UserRow>> {
        let request = request.normalized();
        let rows = sqlx::query(
            r#"SELECT id, email, role, email_verified, created_at,
                      COUNT(*) OVER() AS total
               FROM users
               WHERE $1::text IS NULL OR email ILIKE $1
               ORDER BY created_at DESC, id
               LIMIT $2 OFFSET $3"#,
        )
        .bind(search_pattern(&request))
        .bind(i64::from(request.page_size))
        .bind(request.offset())
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        page_from_rows(request, rows, |row| {
            Ok(UserRow {
                id: UserId::from(row.try_get::<uuid::Uuid, _>("id").map_err(infra)?),
                email: row.try_get("email").map_err(infra)?,
                role: row.try_get("role").map_err(infra)?,
                email_verified: row.try_get("email_verified").map_err(infra)?,
                created_at: row.try_get("created_at").map_err(infra)?,
            })
        })
    }

    async fn catalog(
        &self,
        request: PageRequest,
    ) -> AppResult<Page<CatalogRow>> {
        let request = request.normalized();
        let rows = sqlx::query(
            r#"SELECT id, name, item_type, sku, status, created_at,
                      COUNT(*) OVER() AS total
               FROM catalog_items
               WHERE $1::text IS NULL OR name ILIKE $1 OR sku ILIKE $1
               ORDER BY created_at DESC, id
               LIMIT $2 OFFSET $3"#,
        )
        .bind(search_pattern(&request))
        .bind(i64::from(request.page_size))
        .bind(request.offset())
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        page_from_rows(request, rows, |row| {
            Ok(CatalogRow {
                id: row.try_get("id").map_err(infra)?,
                name: row.try_get("name").map_err(infra)?,
                item_type: row.try_get("item_type").map_err(infra)?,
                sku: row.try_get("sku").map_err(infra)?,
                status: row.try_get("status").map_err(infra)?,
                created_at: row.try_get("created_at").map_err(infra)?,
            })
        })
    }

    async fn billing(
        &self,
        request: PageRequest,
    ) -> AppResult<Page<BillingRow>> {
        let request = request.normalized();
        let rows = sqlx::query(
            r#"SELECT bp.id, ci.name AS catalog_item_name,
                      bp.billing_type, bp.price_amount::text AS price, bp.currency,
                      bp.active, bp.created_at, COUNT(*) OVER() AS total
               FROM billing_plans bp
               JOIN catalog_items ci ON ci.id = bp.catalog_item_id
               WHERE $1::text IS NULL OR ci.name ILIKE $1
               ORDER BY bp.created_at DESC, bp.id
               LIMIT $2 OFFSET $3"#,
        )
        .bind(search_pattern(&request))
        .bind(i64::from(request.page_size))
        .bind(request.offset())
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        page_from_rows(request, rows, |row| {
            Ok(BillingRow {
                id: row.try_get("id").map_err(infra)?,
                catalog_item_name: row.try_get("catalog_item_name").map_err(infra)?,
                billing_type: row.try_get("billing_type").map_err(infra)?,
                price: row.try_get("price").map_err(infra)?,
                currency: row.try_get("currency").map_err(infra)?,
                active: row.try_get("active").map_err(infra)?,
                created_at: row.try_get("created_at").map_err(infra)?,
            })
        })
    }

    async fn packages(
        &self,
        request: PageRequest,
    ) -> AppResult<Page<PackageRow>> {
        let request = request.normalized();
        let rows = sqlx::query(
            r#"SELECT p.id, p.name, p.status, p.created_at,
                      COUNT(pi.id)::bigint AS item_count,
                      COUNT(*) OVER() AS total
               FROM packages p
               LEFT JOIN package_items pi ON pi.package_id = p.id
               WHERE $1::text IS NULL OR p.name ILIKE $1
               GROUP BY p.id
               ORDER BY p.created_at DESC, p.id
               LIMIT $2 OFFSET $3"#,
        )
        .bind(search_pattern(&request))
        .bind(i64::from(request.page_size))
        .bind(request.offset())
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        page_from_rows(request, rows, |row| {
            Ok(PackageRow {
                id: row.try_get("id").map_err(infra)?,
                name: row.try_get("name").map_err(infra)?,
                status: row.try_get("status").map_err(infra)?,
                item_count: to_u64(row.try_get("item_count").map_err(infra)?),
                created_at: row.try_get("created_at").map_err(infra)?,
            })
        })
    }

    async fn purchases(
        &self,
        request: PageRequest,
    ) -> AppResult<Page<PurchaseRow>> {
        let request = request.normalized();
        let rows = sqlx::query(
            r#"SELECT p.id, p.user_id, pkg.name AS package_name,
                      p.gross_amount::text AS amount, p.currency, p.status,
                      p.purchased_at, COUNT(*) OVER() AS total
               FROM purchases p
               JOIN packages pkg ON pkg.id = p.package_id
               JOIN users u ON u.id = p.user_id
               WHERE $1::text IS NULL OR pkg.name ILIKE $1 OR u.email ILIKE $1
               ORDER BY p.purchased_at DESC, p.id
               LIMIT $2 OFFSET $3"#,
        )
        .bind(search_pattern(&request))
        .bind(i64::from(request.page_size))
        .bind(request.offset())
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        page_from_rows(request, rows, |row| {
            Ok(PurchaseRow {
                id: row.try_get("id").map_err(infra)?,
                user_id: UserId::from(row.try_get::<uuid::Uuid, _>("user_id").map_err(infra)?),
                package_name: row.try_get("package_name").map_err(infra)?,
                amount: row.try_get("amount").map_err(infra)?,
                currency: row.try_get("currency").map_err(infra)?,
                status: row.try_get("status").map_err(infra)?,
                purchased_at: row.try_get("purchased_at").map_err(infra)?,
            })
        })
    }
}

fn search_pattern(request: &PageRequest) -> Option<String> {
    request.query.as_ref().map(|query| format!("%{query}%"))
}

fn page_from_rows<T>(
    request: PageRequest,
    rows: Vec<sqlx::postgres::PgRow>,
    convert: impl Fn(sqlx::postgres::PgRow) -> AppResult<T>,
) -> AppResult<Page<T>> {
    let total_items = rows
        .first()
        .map(|row| row.try_get::<i64, _>("total").map(to_u64))
        .transpose()
        .map_err(infra)?
        .unwrap_or(0);
    Ok(Page {
        items: rows
            .into_iter()
            .map(convert)
            .collect::<AppResult<Vec<_>>>()?,
        page: request.page,
        page_size: request.page_size,
        total_items,
    })
}

fn to_u64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn infra(error: impl std::fmt::Display) -> AppError {
    AppError::Infra(error.to_string())
}
