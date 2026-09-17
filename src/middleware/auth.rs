use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::{Json, RequestPartsExt, extract::Request};
use axum_extra::TypedHeader;
use headers::Authorization;
use headers::authorization::Bearer;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::env;
use std::fmt::Display;
use std::sync::LazyLock;
use std::time::SystemTime;

use crate::config::load_config::CONFIG;
use crate::db::connector::{SqliteDatabaseError, with_sql_connection};
use crate::middleware::totp::verify_totp;

const ACCESS_TOKEN_KIND: &str = "access";
const REFRESH_TOKEN_KIND: &str = "refresh";

pub const ACCESS_ADMINISTRATOR: i64 = 0;
pub const ACCESS_MANAGER: i64 = 1;
pub const ACCESS_EDITOR: i64 = 2;
pub const ACCESS_VIEWER: i64 = 3;

pub fn valid_access_level(access_level: i64) -> bool {
    matches!(
        access_level,
        ACCESS_ADMINISTRATOR | ACCESS_MANAGER | ACCESS_EDITOR | ACCESS_VIEWER
    )
}

pub fn access_level_name(access_level: i64) -> &'static str {
    match access_level {
        ACCESS_ADMINISTRATOR => "Administrator",
        ACCESS_MANAGER => "Manager",
        ACCESS_EDITOR => "Editor",
        ACCESS_VIEWER => "Viewer",
        _ => "Unknown",
    }
}

#[derive(Debug)]
pub enum AuthError {
    WrongCredentials,
    MissingCredentials,
    TokenCreation,
    InvalidToken,
    InternalError,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub uid: String,
    pub email: String,
    pub access_level: i64,
    pub token_kind: String,
    pub exp: usize,
}

impl Claims {
    pub fn can_manage_accounts(&self) -> bool {
        self.access_level == ACCESS_ADMINISTRATOR
    }

    pub fn can_read_records(&self) -> bool {
        valid_access_level(self.access_level)
    }

    pub fn can_write_records(&self) -> bool {
        matches!(
            self.access_level,
            ACCESS_ADMINISTRATOR | ACCESS_MANAGER | ACCESS_EDITOR
        )
    }

    pub fn can_delete_records(&self) -> bool {
        matches!(self.access_level, ACCESS_ADMINISTRATOR | ACCESS_MANAGER)
    }
}

#[derive(Debug, Serialize)]
pub struct AuthBody {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,

    pub expires_in: u64,

    pub uid: String,
    pub email: String,

    pub access_level: i64,
    pub access_name: String,
}

#[derive(Debug, Deserialize)]
pub struct AuthenticateRequest {
    pub email: String,
    pub password: String,
    pub otp: Option<String>,
}

pub struct RetrievedAuthData {
    pub uid: String,
    pub email: String,
    pub access_level: i64,
    pub totp_secret: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize)]
pub struct BootstrapStatus {
    pub setup_required: bool,
}

#[derive(Debug, Serialize)]
pub struct SessionBody {
    pub uid: String,
    pub email: String,
    pub name: String,
    pub access_level: i64,
    pub access_name: String,
    pub totp_enabled: bool,
}

static JWT_KEYS: LazyLock<Keys> = LazyLock::new(|| {
    let secret = env::var("JWT_SECRET").unwrap_or_else(|error| {
        crate::fatal_error!(
            format!(
                "failed to load environment variable 'JWT_SECRET': {}",
                error
            ),
            "static",
            "JWT_KEYS"
        )
    });

    Keys::new(secret.as_bytes())
});

static VALID_AUTH_KEYS: LazyLock<HashSet<String>> = LazyLock::new(|| {
    let raw = env::var("AUTH_KEYS").unwrap_or_else(|error| {
        crate::fatal_error!(
            format!("failed to load environment variable 'AUTH_KEYS': {}", error),
            "static",
            "VALID_AUTH_KEYS"
        )
    });

    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(String::from)
        .collect()
});

pub async fn signup_auth(request: Request, next: Next) -> Response {
    let method = request.method();

    if matches!(method, &Method::OPTIONS | &Method::HEAD) {
        return next.run(request).await;
    }

    let auth_key = request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|authorization| {
            let authorization = authorization.trim();

            authorization
                .get(0..7)
                .filter(|prefix| prefix.eq_ignore_ascii_case("Bearer "))
                .map(|_| authorization[7..].trim().to_string())
        });

    let auth_key = match auth_key {
        Some(key) if !key.is_empty() => key,

        Some(_) | None => {
            return unauthorized_response();
        }
    };

    if VALID_AUTH_KEYS.contains(&auth_key) {
        next.run(request).await
    } else {
        unauthorized_response()
    }
}

pub async fn auth(request: Request, next: Next) -> Result<Response, AuthError> {
    let authorization_header = request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok());

    let token = authorization_header
        .and_then(|authorization| authorization.strip_prefix("Bearer "))
        .ok_or(AuthError::InvalidToken)?;

    let validation = Validation::new(Algorithm::HS256);

    let token_data = decode::<Claims>(token, &JWT_KEYS.decoding, &validation)
        .map_err(|_| AuthError::InvalidToken)?;

    if token_data.claims.token_kind != ACCESS_TOKEN_KIND {
        return Err(AuthError::InvalidToken);
    }

    if !valid_access_level(token_data.claims.access_level) {
        return Err(AuthError::InvalidToken);
    }

    Ok(next.run(request).await)
}

pub async fn bootstrap_status() -> Result<Json<BootstrapStatus>, AuthError> {
    let database_result =
        tokio::task::spawn_blocking(move || -> Result<i64, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                let user_count: i64 =
                    connection.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))?;

                Ok(user_count)
            })
        })
        .await;

    match database_result {
        Ok(Ok(user_count)) => Ok(Json(BootstrapStatus {
            setup_required: user_count == 0,
        })),

        Ok(Err(error)) => {
            crate::report_error!(
                format!("failed to check bootstrap status: {}", error),
                "function",
                "bootstrap_status()"
            );

            Err(AuthError::InternalError)
        }

        Err(error) => {
            crate::report_error!(
                format!("bootstrap status blocking task failed: {}", error),
                "function",
                "bootstrap_status()"
            );

            Err(AuthError::InternalError)
        }
    }
}

pub async fn authorize(
    Json(payload): Json<AuthenticateRequest>,
) -> Result<Json<AuthBody>, AuthError> {
    if payload.email.trim().is_empty() || payload.password.is_empty() {
        return Err(AuthError::MissingCredentials);
    }

    let email = payload.email.trim().to_string();

    let password = payload.password;

    let otp = payload.otp;

    let database_result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            connection.query_row(
                r#"
                                SELECT
                                    uid,
                                    email,
                                    access_level,
                                    totp_secret
                                FROM users
                                WHERE
                                    email = ?1
                                    AND password = ?2
                                LIMIT 1
                                "#,
                params![email, password],
                |row| {
                    Ok(RetrievedAuthData {
                        uid: row.get("uid")?,
                        email: row.get("email")?,
                        access_level: row.get("access_level")?,
                        totp_secret: row.get("totp_secret")?,
                    })
                },
            )
        })
    })
    .await;

    match database_result {
        Ok(Ok(data)) => {
            if !valid_access_level(data.access_level) {
                return Err(AuthError::InvalidToken);
            }

            if let Some(totp_secret) = data.totp_secret {
                let Some(otp) = otp else {
                    return Err(AuthError::MissingCredentials);
                };

                if !verify_totp(&totp_secret, &otp, 1) {
                    return Err(AuthError::WrongCredentials);
                }
            }

            issue_auth_body(data.uid, data.email, data.access_level)
        }

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => {
            Err(AuthError::WrongCredentials)
        }

        Ok(Err(error)) => {
            crate::report_error!(
                format!("failed to authorize user: {}", error),
                "function",
                "authorize()"
            );

            Err(AuthError::InternalError)
        }

        Err(error) => {
            crate::report_error!(
                format!("authorization blocking task failed: {}", error),
                "function",
                "authorize()"
            );

            Err(AuthError::InternalError)
        }
    }
}

pub async fn refresh_access_token(
    Json(payload): Json<RefreshTokenRequest>,
) -> Result<Json<AuthBody>, AuthError> {
    if payload.refresh_token.is_empty() {
        return Err(AuthError::MissingCredentials);
    }

    let validation = Validation::new(Algorithm::HS256);

    let token_data = decode::<Claims>(&payload.refresh_token, &JWT_KEYS.decoding, &validation)
        .map_err(|error| {
            crate::report_error!(
                format!("failed to validate refresh token: {}", error),
                "function",
                "refresh_access_token()"
            );

            AuthError::InvalidToken
        })?;

    let refresh_claims = token_data.claims;

    if refresh_claims.token_kind != REFRESH_TOKEN_KIND {
        return Err(AuthError::InvalidToken);
    }

    let uid = refresh_claims.uid;

    /*
     * Reload the account from SQLite.
     *
     * This is deliberate:
     * if an Administrator changes a
     * user's access level, the next
     * refresh token exchange receives
     * the CURRENT access level.
     */
    let database_result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            connection.query_row(
                r#"
                                SELECT
                                    uid,
                                    email,
                                    access_level,
                                    totp_secret
                                FROM users
                                WHERE uid = ?1
                                LIMIT 1
                                "#,
                params![uid],
                |row| {
                    Ok(RetrievedAuthData {
                        uid: row.get("uid")?,
                        email: row.get("email")?,
                        access_level: row.get("access_level")?,
                        totp_secret: row.get("totp_secret")?,
                    })
                },
            )
        })
    })
    .await;

    match database_result {
        Ok(Ok(data)) => {
            if !valid_access_level(data.access_level) {
                return Err(AuthError::InvalidToken);
            }

            issue_auth_body(data.uid, data.email, data.access_level)
        }

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => {
            Err(AuthError::InvalidToken)
        }

        Ok(Err(error)) => {
            crate::report_error!(
                format!("failed to reload user during token refresh: {}", error),
                "function",
                "refresh_access_token()"
            );

            Err(AuthError::InternalError)
        }

        Err(error) => {
            crate::report_error!(
                format!("refresh-token database task failed: {}", error),
                "function",
                "refresh_access_token()"
            );

            Err(AuthError::InternalError)
        }
    }
}

pub async fn session_info(claims: Claims) -> Result<Json<SessionBody>, AuthError> {
    let uid = claims.uid;

    let database_result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            connection.query_row(
                r#"
                                SELECT
                                    uid,
                                    email,
                                    name,
                                    access_level,
                                    totp_secret
                                FROM users
                                WHERE uid = ?1
                                LIMIT 1
                                "#,
                params![uid],
                |row| {
                    let uid: String = row.get("uid")?;

                    let email: String = row.get("email")?;

                    let name: String = row.get("name")?;

                    let access_level: i64 = row.get("access_level")?;

                    let totp_secret: Option<String> = row.get("totp_secret")?;

                    Ok(SessionBody {
                        uid,
                        email,
                        name,
                        access_level,
                        access_name: access_level_name(access_level).to_string(),
                        totp_enabled: totp_secret.is_some(),
                    })
                },
            )
        })
    })
    .await;

    match database_result {
        Ok(Ok(session)) => Ok(Json(session)),

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => {
            Err(AuthError::InvalidToken)
        }

        Ok(Err(error)) => {
            crate::report_error!(
                format!("failed to retrieve current session: {}", error),
                "function",
                "session_info()"
            );

            Err(AuthError::InternalError)
        }

        Err(error) => {
            crate::report_error!(
                format!("session lookup blocking task failed: {}", error),
                "function",
                "session_info()"
            );

            Err(AuthError::InternalError)
        }
    }
}

fn issue_auth_body(
    uid: String,
    email: String,
    access_level: i64,
) -> Result<Json<AuthBody>, AuthError> {
    let access_token = create_token(
        uid.clone(),
        email.clone(),
        access_level,
        ACCESS_TOKEN_KIND,
        CONFIG.jwt_token_config.login_token_expiration,
    )?;

    let refresh_token = create_token(
        uid.clone(),
        email.clone(),
        access_level,
        REFRESH_TOKEN_KIND,
        CONFIG.jwt_token_config.refresh_token_expiration,
    )?;

    Ok(Json(AuthBody::new(
        access_token,
        refresh_token,
        uid,
        email,
        access_level,
    )))
}

fn create_token(
    uid: String,
    email: String,
    access_level: i64,
    token_kind: &str,
    expiration: u64,
) -> Result<String, AuthError> {
    let current_time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|error| {
            crate::report_error!(
                format!("failed to calculate JWT expiration: {}", error),
                "function",
                "create_token()"
            );

            AuthError::InternalError
        })?
        .as_secs();

    let claims = Claims {
        uid,
        email,
        access_level,
        token_kind: token_kind.to_string(),
        exp: (current_time + expiration) as usize,
    };

    encode(&Header::default(), &claims, &JWT_KEYS.encoding).map_err(|error| {
        crate::report_error!(
            format!("failed to create JWT: {}", error),
            "function",
            "create_token()"
        );

        AuthError::TokenCreation
    })
}

impl Display for Claims {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "uid: {}, email: {}, access_level: {}",
            self.uid, self.email, self.access_level
        )
    }
}

impl AuthBody {
    fn new(
        access_token: String,
        refresh_token: String,
        uid: String,
        email: String,
        access_level: i64,
    ) -> Self {
        Self {
            access_token,
            refresh_token,

            token_type: "Bearer".to_string(),

            expires_in: CONFIG.jwt_token_config.login_token_expiration,

            uid,
            email,

            access_level,

            access_name: access_level_name(access_level).to_string(),
        }
    }
}

impl<S> FromRequestParts<S> for Claims
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| AuthError::InvalidToken)?;

        let validation = Validation::new(Algorithm::HS256);

        let token_data = decode::<Claims>(bearer.token(), &JWT_KEYS.decoding, &validation)
            .map_err(|error| {
                /*
                 * Access-token expiration is expected during a normal
                 * session. The client receives 401, exchanges the refresh
                 * token, and retries the request.
                 *
                 * Do not report ExpiredSignature as an application ERROR.
                 * Other JWT failures remain error-level because they can
                 * indicate malformed, invalid, or incorrectly signed
                 * tokens.
                 */
                if !matches!(
                    error.kind(),
                    jsonwebtoken::errors::ErrorKind::ExpiredSignature
                ) {
                    crate::report_error!(
                        format!("failed to extract JWT claims: {}", error),
                        "function",
                        "Claims::from_request_parts()"
                    );
                }

                AuthError::InvalidToken
            })?;

        if token_data.claims.token_kind != ACCESS_TOKEN_KIND {
            return Err(AuthError::InvalidToken);
        }

        if !valid_access_level(token_data.claims.access_level) {
            return Err(AuthError::InvalidToken);
        }

        Ok(token_data.claims)
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AuthError::WrongCredentials => (StatusCode::UNAUTHORIZED, "wrong credentials"),

            AuthError::MissingCredentials => (StatusCode::BAD_REQUEST, "missing credentials"),

            AuthError::TokenCreation => (StatusCode::INTERNAL_SERVER_ERROR, "token creation error"),

            AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "invalid or expired token"),

            AuthError::InternalError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "An internal error occurred",
            ),
        };

        let body = Json(json!({
            "error":
                error_message,
        }));

        (status, body).into_response()
    }
}

struct Keys {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

impl Keys {
    fn new(secret: &[u8]) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret),

            decoding: DecodingKey::from_secret(secret),
        }
    }
}

fn unauthorized_response() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "error":
                "access denied"
        })),
    )
        .into_response()
}
