use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher as _, PasswordVerifier, SaltString};
use argon2::Argon2;
use async_trait::async_trait;

use payplan_app::error::{AppError, AppResult};
use payplan_app::ports::PasswordPort;
#[derive(Default, Clone)]
pub struct PasswordService {
    argon: Argon2<'static>,
}

impl PasswordService {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Synchronous hash for non-async call sites (e.g. tests).
    pub fn hash_blocking(&self, password: &str) -> AppResult<String> {
        let salt = SaltString::generate(&mut OsRng);
        self.argon
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| AppError::Infra(format!("argon2 hash: {e}")))
    }

    pub fn verify_blocking(&self, password: &str, hash: &str) -> AppResult<bool> {
        let parsed =
            PasswordHash::new(hash).map_err(|e| AppError::Infra(format!("argon2 parse: {e}")))?;
        Ok(self
            .argon
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    }
}

#[async_trait]
impl PasswordPort for PasswordService {
    async fn hash(&self, plaintext: &str) -> AppResult<String> {
        self.hash_blocking(plaintext)
    }
    async fn verify(&self, plaintext: &str, hash: &str) -> AppResult<bool> {
        self.verify_blocking(plaintext, hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_round_trip() {
        let svc = PasswordService::new();
        let hash = svc
            .hash_blocking("correct horse battery staple")
            .expect("hash");
        assert!(svc
            .verify_blocking("correct horse battery staple", &hash)
            .unwrap());
        assert!(!svc.verify_blocking("wrong", &hash).unwrap());
    }

    #[test]
    fn hashes_are_unique_per_call() {
        let svc = PasswordService::new();
        let a = svc.hash_blocking("same").unwrap();
        let b = svc.hash_blocking("same").unwrap();
        assert_ne!(a, b, "different salts should yield different hashes");
    }
}

// ===========================================================================
// JWT service (HS256) — Track C
// ===========================================================================

use chrono::{DateTime, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use payplan_app::ports::{RevokedJtiStore, TokenClaims, TokenKind, TokenService};
use sqlx::{PgConnection, PgPool};

/// Access token lifetime: 15 minutes.
const ACCESS_TTL_SECS: i64 = 15 * 60;
/// Refresh token lifetime: 7 days.
const REFRESH_TTL_SECS: i64 = 7 * 24 * 60 * 60;

/// Issuer claim for all tokens issued by this service.
const JWT_ISSUER: &str = "payplan";
/// Audience claim for all tokens.
const JWT_AUDIENCE: &str = "payplan";

/// HS256 JWT signer/verifier. Stateless apart from the shared secret.
pub struct JwtService {
    encode_key: EncodingKey,
    decode_key: DecodingKey,
    header: Header,
    validation: Validation,
}

impl JwtService {
    #[must_use]
    pub fn new(secret: &str) -> Self {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_audience(&[JWT_AUDIENCE]);
        validation.set_issuer(&[JWT_ISSUER]);
        validation.set_required_spec_claims(&["exp", "iat", "sub", "aud", "iss"]);
        validation.leeway = 5;
        Self {
            encode_key: EncodingKey::from_secret(secret.as_bytes()),
            decode_key: DecodingKey::from_secret(secret.as_bytes()),
            header: Header::new(Algorithm::HS256),
            validation,
        }
    }

    fn build_claims(
        &self,
        sub: uuid::Uuid,
        company_id: Option<uuid::Uuid>,
        role: &str,
        kind: TokenKind,
        ttl_secs: i64,
    ) -> TokenClaims {
        let now = Utc::now().timestamp();
        TokenClaims {
            sub,
            company_id,
            role: role.to_string(),
            jti: uuid::Uuid::now_v7().to_string(),
            kind,
            iss: JWT_ISSUER.to_string(),
            aud: JWT_AUDIENCE.to_string(),
            exp: usize::try_from(now + ttl_secs).unwrap_or(usize::MAX),
            iat: usize::try_from(now).unwrap_or(0),
        }
    }
}

#[async_trait]
impl TokenService for JwtService {
    async fn issue_access(
        &self,
        sub: uuid::Uuid,
        company_id: Option<uuid::Uuid>,
        role: &str,
    ) -> AppResult<TokenClaims> {
        Ok(self.build_claims(sub, company_id, role, TokenKind::Access, ACCESS_TTL_SECS))
    }

    async fn issue_refresh(
        &self,
        sub: uuid::Uuid,
        company_id: Option<uuid::Uuid>,
        role: &str,
    ) -> AppResult<TokenClaims> {
        Ok(self.build_claims(sub, company_id, role, TokenKind::Refresh, REFRESH_TTL_SECS))
    }

    async fn encode(&self, claims: &TokenClaims) -> AppResult<String> {
        encode(&self.header, claims, &self.encode_key)
            .map_err(|e| AppError::Infra(format!("jwt encode: {e}")))
    }

    fn verify(&self, token: &str, expected_kind: TokenKind) -> AppResult<TokenClaims> {
        let data = decode::<TokenClaims>(token, &self.decode_key, &self.validation)
            .map_err(|e| AppError::Infra(format!("jwt verify: {e}")))?;
        if data.claims.kind != expected_kind {
            return Err(AppError::Infra(format!(
                "jwt kind mismatch: expected {:?}, got {:?}",
                expected_kind, data.claims.kind
            )));
        }
        Ok(data.claims)
    }
}

// ===========================================================================
// Postgres-backed revoked-JTI store — Track C
// ===========================================================================

/// Postgres-backed implementation of [`RevokedJtiStore`].
pub struct PgRevokedJtiStore {
    #[allow(dead_code)]
    pool: PgPool,
}

impl PgRevokedJtiStore {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RevokedJtiStore for PgRevokedJtiStore {
    async fn revoke(
        &self,
        jti: &str,
        user_id: uuid::Uuid,
        kind: TokenKind,
        expires_at: DateTime<Utc>,
        conn: &mut PgConnection,
    ) -> AppResult<bool> {
        let result = sqlx::query(
            r#"INSERT INTO revoked_jti (jti, user_id, token_type, expires_at)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (jti) DO NOTHING"#,
        )
        .bind(jti)
        .bind(user_id)
        .bind(kind.as_str())
        .bind(expires_at)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(format!("revoke jti: {e}")))?;
        Ok(result.rows_affected() == 1)
    }

    async fn is_revoked(&self, jti: &str, conn: &mut PgConnection) -> AppResult<bool> {
        let row: (bool,) =
            sqlx::query_as(r#"SELECT EXISTS(SELECT 1 FROM revoked_jti WHERE jti = $1)"#)
                .bind(jti)
                .fetch_one(conn)
                .await
                .map_err(|e| AppError::Infra(format!("is_revoked jti: {e}")))?;
        Ok(row.0)
    }
}

#[cfg(test)]
mod jwt_tests {
    use super::*;

    fn svc() -> JwtService {
        JwtService::new("test-secret-very-long-and-secure")
    }

    #[tokio::test]
    async fn access_token_round_trips() {
        let s = svc();
        let claims = s
            .issue_access(uuid::Uuid::now_v7(), None, "user")
            .await
            .unwrap();
        let token = s.encode(&claims).await.unwrap();
        let decoded = s.verify(&token, TokenKind::Access).unwrap();
        assert_eq!(decoded.sub, claims.sub);
        assert_eq!(decoded.kind, TokenKind::Access);
    }

    #[tokio::test]
    async fn wrong_kind_is_rejected() {
        let s = svc();
        let refresh = s
            .issue_refresh(uuid::Uuid::now_v7(), None, "user")
            .await
            .unwrap();
        let token = s.encode(&refresh).await.unwrap();
        // Using a refresh token where an access token is expected must fail.
        assert!(s.verify(&token, TokenKind::Access).is_err());
    }

    #[tokio::test]
    async fn expired_token_is_rejected() {
        let s = svc();
        let mut claims = s
            .issue_access(uuid::Uuid::now_v7(), None, "user")
            .await
            .unwrap();
        // Backdate expiry to the past.
        claims.exp = usize::try_from(Utc::now().timestamp() - 3600).unwrap();
        let token = s.encode(&claims).await.unwrap();
        assert!(s.verify(&token, TokenKind::Access).is_err());
    }

    #[tokio::test]
    async fn wrong_secret_is_rejected() {
        let signer = JwtService::new("secret-a");
        let verifier = JwtService::new("secret-b");
        let claims = signer
            .issue_access(uuid::Uuid::now_v7(), None, "user")
            .await
            .unwrap();
        let token = signer.encode(&claims).await.unwrap();
        assert!(verifier.verify(&token, TokenKind::Access).is_err());
    }
}
