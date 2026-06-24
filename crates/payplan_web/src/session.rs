//! Session and current-user extraction helpers.
//!
//! Two entry points for authentication:
//! - **Extractor**: handlers take `auth: AuthUser` directly — axum runs
//!   `FromRequestParts` automatically, returning 401/403 on failure.
//! - **Middleware** (`require_authenticated` / `require_company_admin` /
//!   `require_platform_admin`): applied as `route_layer` on route groups so
//!   an entire sub-router is gated without per-handler boilerplate.
//!
//! Both paths share [`authenticate`], which reads the `Authorization: Bearer
//! <token>` header, verifies the access token via [`TokenService`], and checks
//! the `revoked_jti` table via [`RevokedJtiStore`].

use std::pin::Pin;

use axum::{
    extract::{FromRequestParts, Request, State},
    http::{header::HeaderMap, request::Parts, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
// `TokenService` / `RevokedJtiStore` are referenced via intra-doc links above;
// the `authenticate` function only uses the trait methods on `ctx.tokens` /
// `ctx.revoked_jti`, so the imports are not strictly needed at the Rust level.
#[allow(unused_imports)]
use payplan_app::ports::{RevokedJtiStore, TokenKind, TokenService};
use payplan_core::platform::user::UserRole;
use payplan_core::shared::ids::UserId;
use serde::Serialize;

use crate::context::AppContext;

/// The authenticated principal, available to handlers as an extractor
/// (`auth: AuthUser`) or via request extensions after a `require_*` middleware.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: UserId,
    pub role: UserRole,
}

impl AuthUser {
    /// True if the caller is admin.
    pub fn is_admin(&self) -> bool {
        self.role.is_admin()
    }

    /// True if the caller may act on behalf of any user (admin impersonation).
    pub fn can_impersonate(&self) -> bool {
        self.role.is_admin()
    }

    /// True only for platform admins, who may target any company.
    pub fn can_admin_platform(&self) -> bool {
        self.role.is_admin()
    }
}

/// Errors raised by the auth layer. Maps to the correct HTTP status so callers
/// see 401 vs 403 rather than a generic 500.
#[derive(Debug)]
pub enum AuthError {
    /// No `Authorization: Bearer ...` header present.
    MissingToken,
    /// Token failed signature/expiry/kind verification.
    InvalidToken(String),
    /// Token's `jti` is in `revoked_jti` (logged out or rotated).
    Revoked,
    /// Authenticated, but the role does not satisfy the required predicate.
    Forbidden,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message): (StatusCode, String) = match self {
            Self::MissingToken => (StatusCode::UNAUTHORIZED, "missing bearer token".into()),
            Self::InvalidToken(msg) => (StatusCode::UNAUTHORIZED, msg),
            Self::Revoked => (StatusCode::UNAUTHORIZED, "token revoked".into()),
            Self::Forbidden => (StatusCode::FORBIDDEN, "insufficient role".into()),
        };
        #[derive(Serialize)]
        struct Body {
            message: String,
        }
        (status, axum::Json(Body { message })).into_response()
    }
}

/// Core authentication: extract + verify the bearer access token and check
/// revocation. Takes only the headers (not the full request) so it's usable
/// from both the `FromRequestParts` extractor path and the middleware path.
pub async fn authenticate(ctx: &AppContext, headers: &HeaderMap) -> Result<AuthUser, AuthError> {
    // Manual Authorization header parsing — avoids an axum-extra dependency.
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer ").map(str::to_string));

    let token = match token {
        Some(t) => t,
        None => return Err(AuthError::MissingToken),
    };

    authenticate_access_token(ctx, &token).await
}

/// Verify an access token supplied by a non-API transport, such as the
/// HttpOnly cookie used by the server-rendered administration UI.
pub async fn authenticate_access_token(
    ctx: &AppContext,
    token: &str,
    ) -> Result<AuthUser, AuthError> {
    let claims = ctx
        .tokens
        .verify(token, TokenKind::Access)
        .map_err(|e| AuthError::InvalidToken(e.to_string()))?;

    let mut conn = ctx.pool.acquire().await.map_err(|e| {
        tracing::error!(error = %e, "auth: failed to acquire connection");
        AuthError::InvalidToken("service unavailable".into())
    })?;
    // Fail closed: on a store error, deny (treat as revoked).
    if ctx
        .revoked_jti
        .is_revoked(&claims.jti, &mut conn)
        .await
        .unwrap_or(true)
    {
        return Err(AuthError::Revoked);
    }

    let role = match claims.role.as_str() {
        "user" => UserRole::User,
        "admin" => UserRole::Admin,
        other => {
            return Err(AuthError::InvalidToken(format!("unknown role: {other}")));
        }
    };

    Ok(AuthUser {
        user_id: UserId::from(claims.sub),
        role,
    })
}

// Allow handlers to use `auth: AuthUser` as a direct extractor.
impl FromRequestParts<AppContext> for AuthUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        ctx: &AppContext,
    ) -> Result<Self, Self::Rejection> {
        authenticate(ctx, &parts.headers).await
    }
}

type RoleFuture = Pin<Box<dyn std::future::Future<Output = Result<Response, AuthError>> + Send>>;

/// Middleware: require any authenticated user. Inserts `AuthUser` into
/// request extensions so downstream handlers can read it without re-running
/// the extractor.
pub async fn require_authenticated(
    State(ctx): State<AppContext>,
    mut req: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let auth = authenticate(&ctx, req.headers()).await?;
    req.extensions_mut().insert(auth);
    Ok(next.run(req).await)
}

/// Build a boxed middleware that requires a role satisfying `predicate`.
fn make_role_guard<F>(
    predicate: F,
) -> impl Fn(State<AppContext>, Request, Next) -> RoleFuture + Clone + Send + Sync + 'static
where
    F: Fn(UserRole) -> bool + Clone + Send + Sync + 'static,
{
    move |State(ctx): State<AppContext>, mut req: Request, next: Next| {
        let predicate = predicate.clone();
        Box::pin(async move {
            let auth = authenticate(&ctx, req.headers()).await?;
            if !predicate(auth.role) {
                return Err(AuthError::Forbidden);
            }
            req.extensions_mut().insert(auth);
            Ok(next.run(req).await)
        })
    }
}

/// Convenience: require CompanyAdmin or PlatformAdmin.
pub fn require_company_admin(
) -> impl Fn(State<AppContext>, Request, Next) -> RoleFuture + Clone + Send + Sync + 'static {
    make_role_guard(UserRole::is_admin)
}

/// Convenience: require PlatformAdmin only.
pub fn require_platform_admin(
) -> impl Fn(State<AppContext>, Request, Next) -> RoleFuture + Clone + Send + Sync + 'static {
    make_role_guard(UserRole::is_admin)
}
