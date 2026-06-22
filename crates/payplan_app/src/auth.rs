use chrono::{DateTime, Utc};
use payplan_core::{
    platform::user::{User, UserRole},
    shared::ids::{CompanyId, UserId},
};
use sqlx::PgPool;

use crate::{
    error::{AppError, AppResult},
    ports::{PasswordPort, RevokedJtiStore, TokenKind, TokenService, UserRepo},
};

const DUMMY_ARGON2_HASH: &str =
    "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$JEjzR8KNbB7+kyBJPzLYa6Ek1T/lnM39FhOvkMB6HKs";

pub struct AuthDeps<'a> {
    pub pool: &'a PgPool,
    pub users: &'a dyn UserRepo,
    pub passwords: &'a dyn PasswordPort,
    pub tokens: &'a dyn TokenService,
    pub revoked_jti: &'a dyn RevokedJtiStore,
}

#[derive(Debug, Clone)]
pub struct LoginInput {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone)]
pub struct IssuedTokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: UserId,
    pub company_id: Option<CompanyId>,
    pub role: UserRole,
}

pub async fn login(deps: &AuthDeps<'_>, input: LoginInput) -> AppResult<IssuedTokenPair> {
    let mut conn = deps
        .pool
        .acquire()
        .await
        .map_err(|error| AppError::Infra(error.to_string()))?;
    let user = deps.users.find_by_email(&input.email, &mut conn).await?;
    let (user, valid) = match user {
        Some(user) => {
            let valid = deps
                .passwords
                .verify(&input.password, &user.password_hash)
                .await?;
            (Some(user), valid)
        }
        None => {
            let _ = deps
                .passwords
                .verify(&input.password, DUMMY_ARGON2_HASH)
                .await;
            (None, false)
        }
    };
    if !valid {
        return Err(AppError::Forbidden("invalid credentials".into()));
    }

    issue_pair(
        deps,
        user.expect("valid credentials require an existing user"),
    )
    .await
}

pub async fn refresh_tokens(
    deps: &AuthDeps<'_>,
    refresh_token: &str,
) -> AppResult<IssuedTokenPair> {
    let claims = deps.tokens.verify(refresh_token, TokenKind::Refresh)?;
    let mut conn = deps
        .pool
        .acquire()
        .await
        .map_err(|error| AppError::Infra(error.to_string()))?;
    let expires_at = DateTime::from_timestamp(claims.exp as i64, 0).unwrap_or_else(Utc::now);
    let inserted = deps
        .revoked_jti
        .revoke(
            &claims.jti,
            claims.sub,
            TokenKind::Refresh,
            expires_at,
            &mut conn,
        )
        .await?;
    if !inserted {
        return Err(AppError::Forbidden("token revoked".into()));
    }
    let user = deps
        .users
        .get(UserId::from(claims.sub), &mut conn)
        .await?
        .ok_or_else(|| AppError::NotFound("user no longer exists".into()))?;
    drop(conn);
    issue_pair(deps, user).await
}

pub async fn revoke_tokens(
    deps: &AuthDeps<'_>,
    access_token: &str,
    refresh_token: Option<&str>,
) -> AppResult<()> {
    let mut conn = deps
        .pool
        .acquire()
        .await
        .map_err(|error| AppError::Infra(error.to_string()))?;
    for (token, kind) in std::iter::once((access_token, TokenKind::Access))
        .chain(refresh_token.map(|token| (token, TokenKind::Refresh)))
    {
        if let Ok(claims) = deps.tokens.verify(token, kind) {
            let expires_at =
                DateTime::from_timestamp(claims.exp as i64, 0).unwrap_or_else(Utc::now);
            let _ = deps
                .revoked_jti
                .revoke(&claims.jti, claims.sub, kind, expires_at, &mut conn)
                .await?;
        }
    }
    Ok(())
}

async fn issue_pair(deps: &AuthDeps<'_>, user: User) -> AppResult<IssuedTokenPair> {
    let role = role_str(user.role);
    let company_id = user.company_id.map(|company_id| company_id.0);
    let access = deps
        .tokens
        .issue_access(user.id.0, company_id, role)
        .await?;
    let refresh = deps
        .tokens
        .issue_refresh(user.id.0, company_id, role)
        .await?;

    Ok(IssuedTokenPair {
        access_token: deps.tokens.encode(&access).await?,
        refresh_token: deps.tokens.encode(&refresh).await?,
        user_id: user.id,
        company_id: user.company_id,
        role: user.role,
    })
}

#[must_use]
pub fn role_str(role: UserRole) -> &'static str {
    match role {
        UserRole::User => "user",
        UserRole::CompanyAdmin => "company_admin",
        UserRole::PlatformAdmin => "platform_admin",
    }
}
