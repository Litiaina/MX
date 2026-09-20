use axum::{Json, extract::Path, response::IntoResponse};
use reqwest::StatusCode;
use rusqlite::params;
use serde_json::json;
use uuid::Uuid;

use crate::{
    api::{
        admin::model::{
            AdminPasswordResetRequest, AdminSecurityResetRequest, CreateUserRequest,
            ExecuteQueryRequest, FilterRequest, ModifySuperUserRequest, UserSummary,
        },
        api_error::SqliteQueryError,
        query_handler::execute_sql_qeury,
    },
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::{ACCESS_ADMINISTRATOR, Claims, access_level_name, valid_access_level},
    util::{
        authentication::{CredentialStatus, verify_user_credentials},
        password::{hash_password, validate_new_password},
    },
};

/*
ACCESS LEVEL
0 = Administrator
1 = Manager
2 = Editor
3 = Viewer
*/

pub async fn create_super_user(
    Json(request): Json<CreateUserRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    if request.email.trim().is_empty()
        || request.password.is_empty()
        || request.name.trim().is_empty()
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "response": "name, email, and password are required"
            })),
        ));
    }

    if let Err(message) = validate_new_password(&request.password) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "response": message })),
        ));
    }

    let created_uid = Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().timestamp();

    let email = request.email.trim().to_string();
    let password_hash = hash_password(&request.password).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "response": "internal server error" })),
        )
    })?;
    let name = request.name.trim().to_string();

    /*
    Bootstrap is transactional.

    Only the first account may be created through /mx/v1/auth/create.
    The first account is ALWAYS access level 0 (Administrator).
    */
    let database_result =
        tokio::task::spawn_blocking(move || -> Result<bool, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                let transaction = connection.unchecked_transaction()?;

                let user_count: i64 =
                    transaction.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))?;

                if user_count > 0 {
                    return Ok(false);
                }

                transaction.execute(
                    r#"
                        INSERT INTO users (
                            uid,
                            email,
                            password,
                            password_hash,
                            name,
                            created_at,
                            access_level,
                            totp_secret
                        ) VALUES (
                            ?1, ?2, '', ?3, ?4, ?5, ?6, NULL
                        )
                        "#,
                    params![
                        created_uid,
                        email,
                        password_hash,
                        name,
                        created_at,
                        ACCESS_ADMINISTRATOR,
                    ],
                )?;

                transaction.commit()?;

                Ok(true)
            })
        })
        .await;

    match database_result {
        Ok(Ok(true)) => Ok((
            StatusCode::CREATED,
            Json(json!({
                "response": "administrator created successfully"
            })),
        )),

        Ok(Ok(false)) => Err((
            StatusCode::CONFLICT,
            Json(json!({
                "response": "MX is already initialized; create additional accounts from the Administrator Accounts panel"
            })),
        )),

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::SqliteFailure(error, _))))
            if error.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            Err((
                StatusCode::CONFLICT,
                Json(json!({
                    "response": "administrator account already exists"
                })),
            ))
        }

        Ok(Err(error)) => {
            crate::report_error!(
                format!("failed to create bootstrap administrator: {}", error),
                "function",
                "create_super_user()"
            );

            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "internal server error"
                })),
            ))
        }

        Err(error) => {
            crate::report_error!(
                format!("bootstrap administrator blocking task failed: {}", error),
                "function",
                "create_super_user()"
            );

            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "internal server error"
                })),
            ))
        }
    }
}

pub async fn get_user(
    claims: Claims,
    Json(request): Json<FilterRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    if !claims.can_manage_accounts() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "response": "access denied" })),
        ));
    }

    let filter = match request.filter.as_str() {
        "uid" => Some("uid"),
        "email" => Some("email"),
        "all" => None,

        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "response": format!(
                        "'{}' is an invalid filter. Use 'uid', 'email', or 'all'",
                        request.filter
                    )
                })),
            ));
        }
    };

    let value = request.value;

    let database_result =
        tokio::task::spawn_blocking(move || -> Result<Vec<UserSummary>, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                let mut users: Vec<UserSummary> = Vec::new();

                match filter {
                    Some(filter) => {
                        let query = format!(
                            "SELECT uid, email, name, access_level, totp_secret FROM users WHERE {} = ?1 LIMIT 1",
                            filter
                        );

                        let mut statement = connection.prepare(&query)?;

                        let rows = statement.query_map(params![value], |row| {
                            let uid: String = row.get("uid")?;
                            let email: String = row.get("email")?;
                            let name: String = row.get("name")?;
                            let access_level: i64 = row.get("access_level")?;
                            let totp_secret: Option<String> = row.get("totp_secret")?;

                            Ok(UserSummary {
                                uid,
                                email,
                                name,
                                access_level,
                                access_name: access_level_name(access_level).to_string(),
                                totp_enabled: totp_secret.is_some(),
                            })
                        })?;

                        for row in rows {
                            users.push(row?);
                        }
                    }

                    None => {
                        let mut statement = connection.prepare(
                            r#"
                        SELECT uid, email, name, access_level, totp_secret
                        FROM users
                        ORDER BY email ASC
                        "#,
                        )?;

                        let rows = statement.query_map([], |row| {
                            let uid: String = row.get("uid")?;
                            let email: String = row.get("email")?;
                            let name: String = row.get("name")?;
                            let access_level: i64 = row.get("access_level")?;
                            let totp_secret: Option<String> = row.get("totp_secret")?;

                            Ok(UserSummary {
                                uid,
                                email,
                                name,
                                access_level,
                                access_name: access_level_name(access_level).to_string(),
                                totp_enabled: totp_secret.is_some(),
                            })
                        })?;

                        for row in rows {
                            users.push(row?);
                        }
                    }
                }

                Ok(users)
            })
        })
        .await;

    match database_result {
        Ok(Ok(users)) => Ok((
            StatusCode::OK,
            Json(json!({
                "users": users
            })),
        )),

        Ok(Err(error)) => {
            crate::report_error!(
                format!("failed to retrieve users: {}", error),
                "function",
                "get_user()"
            );

            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "internal server error"
                })),
            ))
        }

        Err(error) => {
            crate::report_error!(
                format!("user retrieval blocking task failed: {}", error),
                "function",
                "get_user()"
            );

            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "internal server error"
                })),
            ))
        }
    }
}

pub async fn modify_super_user(
    claims: Claims,
    Json(request): Json<ModifySuperUserRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    if !claims.can_manage_accounts() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "response": "access denied" })),
        ));
    }

    if request.new_password.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "response": "use the dedicated administrator password-reset workflow"
            })),
        ));
    }

    if let Some(access_level) = request.access_level {
        if !valid_access_level(access_level) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "response": "invalid access level; allowed values are 0, 1, 2, and 3"
                })),
            ));
        }

        let changing_own_access = match request.filter.as_str() {
            "uid" => request.value == claims.uid,
            "email" => request.value == claims.email,
            _ => false,
        };

        if changing_own_access && access_level != claims.access_level {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "response": "an administrator cannot change their own access level from the account-management route"
                })),
            ));
        }
    }

    let filter = match request.filter.as_str() {
        "uid" => "uid",
        "email" => "email",
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "response": format!("'{}' is a invalid filter please use allowed filter such as 'uid' or 'email'", &request.filter)
                })),
            ));
        }
    };

    let security_changed = request.new_email.is_some() || request.access_level.is_some();

    let database_result =
        tokio::task::spawn_blocking(move || -> Result<usize, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                let query = format!(
                    r#"
            UPDATE users
                SET
                    email = COALESCE(?1, email),
                    name = COALESCE(?2, name),
                    access_level = COALESCE(?3, access_level),
                    auth_version = auth_version + CASE WHEN ?5 THEN 1 ELSE 0 END
                WHERE {filter} = ?4
            "#
                );

                let updated_rows = connection.execute(
                    &query,
                    params![
                        request.new_email,
                        request.new_name,
                        request.access_level,
                        request.value,
                        security_changed,
                    ],
                )?;

                Ok(updated_rows)
            })
        })
        .await;

    match database_result {
        Ok(Ok(updated_rows)) if updated_rows > 0 => Ok((
            StatusCode::OK,
            Json(json!({
                "response": "user modified successfully"
            })),
        )),

        Ok(Ok(_)) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "user not found"
            })),
        )),

        Ok(Err(error)) => {
            crate::report_error!(
                format!("failed to modify user: {}", error),
                "function",
                "modify_user()"
            );

            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "internal server error"
                })),
            ))
        }

        Err(error) => {
            crate::report_error!(
                format!("user modification blocking task failed: {}", error),
                "function",
                "modify_user()"
            );

            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "internal server error"
                })),
            ))
        }
    }
}

pub async fn admin_reset_user_password(
    claims: Claims,
    Path(target_uid): Path<String>,
    Json(request): Json<AdminPasswordResetRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    if !claims.can_manage_accounts() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "response": "access denied" })),
        ));
    }
    if let Err(message) = validate_new_password(&request.new_password) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "response": message })),
        ));
    }

    let password_hash = match hash_password(&request.new_password) {
        Ok(hash) => hash,
        Err(_) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "response": "internal server error" })),
            ));
        }
    };
    let admin_uid = claims.uid;
    let database_result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            let status = verify_user_credentials(
                &transaction,
                &admin_uid,
                &request.admin_password,
                request.admin_otp.as_deref(),
                request.admin_recovery_code.as_deref(),
            )?;
            if status != CredentialStatus::Valid {
                return Ok((0usize, status));
            }

            let updated = transaction.execute(
                r#"
                    UPDATE users
                    SET password_hash = ?1,
                        password = '',
                        auth_version = auth_version + 1
                    WHERE uid = ?2
                "#,
                params![password_hash, target_uid],
            )?;
            transaction.commit()?;
            Ok((updated, CredentialStatus::Valid))
        })
    })
    .await;

    match database_result {
        Ok(Ok((1, CredentialStatus::Valid))) => Ok((
            StatusCode::OK,
            Json(json!({
                "response": "password reset; all existing sessions were revoked"
            })),
        )),
        Ok(Ok((0, CredentialStatus::Valid))) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "response": "user not found" })),
        )),
        Ok(Ok((_, CredentialStatus::SecondFactorRequired))) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "response": "administrator authenticator or recovery code is required" })),
        )),
        Ok(Ok(_)) => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "response": "administrator re-authentication failed" })),
        )),
        Ok(Err(error)) => {
            crate::report_error!(
                format!("{error}"),
                "function",
                "admin_reset_user_password()"
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "response": "internal server error" })),
            ))
        }
        Err(error) => {
            crate::report_error!(
                format!("{error}"),
                "function",
                "admin_reset_user_password()"
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "response": "internal server error" })),
            ))
        }
    }
}

pub async fn admin_reset_user_security(
    claims: Claims,
    Path(target_uid): Path<String>,
    Json(request): Json<AdminSecurityResetRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    if !claims.can_manage_accounts() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "response": "access denied" })),
        ));
    }

    let admin_uid = claims.uid;
    let database_result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            let status = verify_user_credentials(
                &transaction,
                &admin_uid,
                &request.admin_password,
                request.admin_otp.as_deref(),
                request.admin_recovery_code.as_deref(),
            )?;
            if status != CredentialStatus::Valid {
                return Ok((0usize, status));
            }

            let updated = transaction.execute(
                r#"
                    UPDATE users
                    SET totp_secret = NULL,
                        totp_pending_secret = NULL,
                        totp_pending_created_at = NULL,
                        auth_version = auth_version + 1
                    WHERE uid = ?1
                "#,
                params![target_uid],
            )?;
            if updated == 1 {
                transaction.execute(
                    "DELETE FROM user_recovery_codes WHERE user_uid = ?1",
                    params![target_uid],
                )?;
            }
            transaction.commit()?;
            Ok((updated, CredentialStatus::Valid))
        })
    })
    .await;

    match database_result {
        Ok(Ok((1, CredentialStatus::Valid))) => Ok((
            StatusCode::OK,
            Json(json!({
                "response": "authenticator reset; all existing sessions were revoked"
            })),
        )),
        Ok(Ok((0, CredentialStatus::Valid))) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "response": "user not found" })),
        )),
        Ok(Ok((_, CredentialStatus::SecondFactorRequired))) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "response": "administrator authenticator or recovery code is required" })),
        )),
        Ok(Ok(_)) => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "response": "administrator re-authentication failed" })),
        )),
        Ok(Err(error)) => {
            crate::report_error!(
                format!("{error}"),
                "function",
                "admin_reset_user_security()"
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "response": "internal server error" })),
            ))
        }
        Err(error) => {
            crate::report_error!(
                format!("{error}"),
                "function",
                "admin_reset_user_security()"
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "response": "internal server error" })),
            ))
        }
    }
}

pub async fn delete_super_user(
    claims: Claims,
    Json(request): Json<FilterRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    if !claims.can_manage_accounts() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "response": "access denied" })),
        ));
    }

    let deleting_self = match request.filter.as_str() {
        "uid" => request.value == claims.uid,
        "email" => request.value == claims.email,
        _ => false,
    };

    if deleting_self {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "response": "use the self-service account delete route to delete your own account"
            })),
        ));
    }

    let filter = match request.filter.as_str() {
        "uid" => "uid",
        "email" => "email",
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "response": format!("'{}' is a invalid filter please use allowed filter such as 'uid' or 'email'", &request.filter)
                })),
            ));
        }
    };

    let database_result =
        tokio::task::spawn_blocking(move || -> Result<usize, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                let query = format!(r#"DELETE FROM users WHERE {filter} = ?1"#);

                let updated_rows = connection.execute(&query, params![request.value])?;

                Ok(updated_rows)
            })
        })
        .await;

    match database_result {
        Ok(Ok(updated_rows)) if updated_rows > 0 => Ok((
            StatusCode::OK,
            Json(json!({
                "response": "user deleted successfully"
            })),
        )),

        Ok(Ok(_)) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "user not found"
            })),
        )),

        Ok(Err(error)) => {
            crate::report_error!(
                format!("failed to modify user: {}", error),
                "function",
                "modify_user()"
            );

            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "internal server error"
                })),
            ))
        }

        Err(error) => {
            crate::report_error!(
                format!("user modification blocking task failed: {}", error),
                "function",
                "modify_user()"
            );

            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "internal server error"
                })),
            ))
        }
    }
}

pub async fn execute_query(
    claims: Claims,
    Json(request): Json<ExecuteQueryRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    if !claims.can_manage_accounts() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "response": "access denied" })),
        ));
    }

    let query = request.query;

    let command = query
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if !matches!(command.as_str(), "SELECT" | "EXPLAIN") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "response": "the administrator query endpoint is read-only"
            })),
        ));
    }

    let identifiers = query
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    if identifiers
        .iter()
        .any(|identifier| matches!(identifier.as_str(), "users" | "user_recovery_codes"))
    {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({
                "response": "authentication tables are unavailable through the query endpoint"
            })),
        ));
    }

    match execute_sql_qeury(query).await {
        SqliteQueryError::Result(result) => Ok((StatusCode::OK, Json(result))),
        SqliteQueryError::Error(err_value) => Err((StatusCode::BAD_REQUEST, Json(err_value))),
        SqliteQueryError::BadRequest(message) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "response": message.to_string() })),
        )),
        SqliteQueryError::InternalError => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "response": "internal server error" })),
        )),
    }
}
