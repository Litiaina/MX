use std::{io::Cursor, sync::Arc};

use axum::{Json, response::IntoResponse};
use image::ImageFormat;
use reqwest::StatusCode;
use rusqlite::params;
use serde_json::json;
use uuid::Uuid;

use crate::{
    api::{
        admin::model::{
            CreateUserRequest, ExecuteQueryRequest, FilterRequest, ModifySuperUserRequest,
            UserSummary,
        },
        api_error::SqliteQueryError,
        query_handler::execute_sql_qeury,
    },
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::{
        auth::{
            ACCESS_ADMINISTRATOR, AuthenticateRequest, Claims, access_level_name,
            valid_access_level,
        },
        totp::verify_totp,
    },
    util::{qr::generate_qr_image, randomizer::generate_random_base32},
};

/*
ACCESS LEVEL
0 = Administrator
1 = Manager
2 = Editor
3 = Viewer
*/

#[derive(Debug)]
enum TotpSecretUpdateError {
    UserNotFound,
    QrGeneration(String),
    Internal(String),
}

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

    let created_uid = Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().timestamp();

    let email = request.email.trim().to_string();
    let password = request.password;
    let name = request.name.trim().to_string();

    /*
    Bootstrap is transactional.

    Only the first account may be created through /aris/v1/auth/create.
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
                            name,
                            created_at,
                            access_level,
                            totp_secret
                        ) VALUES (
                            ?1, ?2, ?3, ?4, ?5, ?6, NULL
                        )
                        "#,
                    params![
                        created_uid,
                        email,
                        password,
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
                "response": "ARIS is already initialized; create additional accounts from the Administrator Accounts panel"
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

//TODO: to be modified for administrator use
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

    let database_result =
        tokio::task::spawn_blocking(move || -> Result<usize, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                let query = format!(
                    r#"
            UPDATE users
                SET
                    email = COALESCE(?1, email),
                    password = COALESCE(?2, password),
                    name = COALESCE(?3, name),
                    access_level = COALESCE(?4, access_level)
                WHERE {filter} = ?5
            "#
                );

                let updated_rows = connection.execute(
                    &query,
                    params![
                        request.new_email,
                        request.new_password,
                        request.new_name,
                        request.access_level,
                        request.value,
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

pub async fn enable_2fa_user(
    claims: Claims,
    Json(request): Json<AuthenticateRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    if !claims.can_manage_accounts() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "response": "access denied" })),
        ));
    }

    if request.email.is_empty() || request.password.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "response": "email and password are required" })),
        ));
    }

    let email = Arc::new(request.email);
    let email_clone = Arc::clone(&email);
    let database_result = tokio::task::spawn_blocking(
        move || -> Result<(String, Option<String>), SqliteDatabaseError> {
            with_sql_connection(|connection| {
                let query = format!(
                    r#"SELECT uid, totp_secret FROM users WHERE email = ?1 AND password = ?2"#
                );

                let mut statement = connection.prepare(&query)?;

                let row = statement.query_row(
                    params![Arc::clone(&email_clone), &request.password],
                    |row| {
                        let uid: String = row.get("uid")?;
                        let totp_secret: Option<String> = row.get("totp_secret")?;

                        Ok((uid, totp_secret))
                    },
                )?;

                Ok(row)
            })
        },
    )
    .await;

    match database_result {
        Ok(Ok((uid, totp_secret))) => match totp_secret {
            Some(value) => {
                if let Some(otp) = request.otp {
                    let valid = verify_totp(&value, otp.as_str(), 1);
                    if !valid {
                        Err((
                            StatusCode::UNAUTHORIZED,
                            Json(json!({ "response": "invalid credentials" })),
                        ))
                    } else {
                        match update_totp_secret_by_uid(email.as_ref().to_string(), uid).await {
                            Ok(image) => Ok((
                                StatusCode::OK,
                                ([(axum::http::header::CONTENT_TYPE, "image/png")], image),
                            )),
                            Err(err) => match err {
                                TotpSecretUpdateError::UserNotFound => Err((
                                    StatusCode::NOT_FOUND,
                                    Json(json!({ "response": "user not found" })),
                                )),
                                TotpSecretUpdateError::QrGeneration(error) => {
                                    crate::report_error!(
                                        format!("{error}"),
                                        "function",
                                        "enable_2fa_user()"
                                    );

                                    Err((
                                        StatusCode::INTERNAL_SERVER_ERROR,
                                        Json(json!({ "response": "internal server error" })),
                                    ))
                                }
                                TotpSecretUpdateError::Internal(error) => {
                                    crate::report_error!(
                                        format!("{error}"),
                                        "function",
                                        "enable_2fa_user()"
                                    );

                                    Err((
                                        StatusCode::INTERNAL_SERVER_ERROR,
                                        Json(json!({ "response": "internal server error" })),
                                    ))
                                }
                            },
                        }
                    }
                } else {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(json!({ "response": "otp is required" })),
                    ));
                }
            }
            None => match update_totp_secret_by_uid(email.as_ref().to_string(), uid).await {
                Ok(image) => Ok((
                    StatusCode::OK,
                    ([(axum::http::header::CONTENT_TYPE, "image/png")], image),
                )),
                Err(err) => match err {
                    TotpSecretUpdateError::UserNotFound => Err((
                        StatusCode::NOT_FOUND,
                        Json(json!({ "response": "user not found" })),
                    )),
                    TotpSecretUpdateError::QrGeneration(error) => {
                        crate::report_error!(format!("{error}"), "function", "enable_2fa_user()");

                        Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({ "response": "internal server error" })),
                        ))
                    }
                    TotpSecretUpdateError::Internal(error) => {
                        crate::report_error!(format!("{error}"), "function", "enable_2fa_user()");

                        Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({ "response": "internal server error" })),
                        ))
                    }
                },
            },
        },

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "response": "invalid credentials" })),
        )),

        Ok(Err(error)) => {
            crate::report_error!(format!("{error}"), "function", "enable_2fa_user");

            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "response": "internal server error" })),
            ))
        }

        Err(error) => {
            crate::report_error!(format!("{error}"), "function", "enable_2fa_user()");

            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "response": "internal server error" })),
            ))
        }
    }
}

pub async fn disable_2fa_user(
    claims: Claims,
    Json(request): Json<AuthenticateRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    if !claims.can_manage_accounts() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "response": "access denied" })),
        ));
    }

    if request.email.is_empty() || request.password.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "response": "email and password are required" })),
        ));
    }

    let email = Arc::new(request.email);
    let email_clone = Arc::clone(&email);
    let database_result = tokio::task::spawn_blocking(
        move || -> Result<(String, Option<String>), SqliteDatabaseError> {
            with_sql_connection(|connection| {
                let query = format!(
                    r#"SELECT uid, totp_secret FROM users WHERE email = ?1 AND password = ?2"#
                );

                let mut statement = connection.prepare(&query)?;

                let row = statement.query_row(
                    params![Arc::clone(&email_clone), &request.password],
                    |row| {
                        let uid: String = row.get("uid")?;
                        let totp_secret: Option<String> = row.get("totp_secret")?;

                        Ok((uid, totp_secret))
                    },
                )?;

                Ok(row)
            })
        },
    )
    .await;

    match database_result {
        Ok(Ok((uid, totp_secret))) => match totp_secret {
            Some(value) => {
                if let Some(otp) = request.otp {
                    let valid = verify_totp(&value, otp.as_str(), 1);
                    if !valid {
                        Err((
                            StatusCode::UNAUTHORIZED,
                            Json(json!({ "response": "invalid credentials" })),
                        ))
                    } else {
                        let database_result = tokio::task::spawn_blocking(
                            move || -> Result<usize, SqliteDatabaseError> {
                                with_sql_connection(|connection| {
                                    let query = format!(
                                        r#"
                            UPDATE users SET
                                totp_secret = COALESCE(?1, totp_secret)
                            WHERE uid = ?2
                        "#
                                    );
                                    let remove_totp: Option<String> = None;
                                    let updated_rows =
                                        connection.execute(&query, params![&remove_totp, &uid])?;

                                    Ok(updated_rows)
                                })
                            },
                        )
                        .await;

                        match database_result {
                            Ok(Ok(updated_rows)) if updated_rows > 0 => Ok((
                                StatusCode::OK,
                                Json(json!({ "response": "2fa disabled successfully" })),
                            )),
                            Ok(Ok(_)) => Err((
                                StatusCode::NOT_FOUND,
                                Json(json!({ "response": "user not found" })),
                            )),
                            Ok(Err(error)) => {
                                crate::report_error!(
                                    format!("{}", error),
                                    "function",
                                    "disable_2fa_user()"
                                );

                                Err((
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(json!({ "response": "internal server error" })),
                                ))
                            }
                            Err(error) => {
                                crate::report_error!(
                                    format!("{}", error),
                                    "function",
                                    "update_totp_secret_by_uid()"
                                );

                                Err((
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(json!({ "response": "internal server error" })),
                                ))
                            }
                        }
                    }
                } else {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(json!({ "response": "otp is required" })),
                    ));
                }
            }
            None => Ok((
                StatusCode::OK,
                Json(json!({ "response": "2fa already disabled" })),
            )),
        },

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "response": "invalid credentials" })),
        )),

        Ok(Err(error)) => {
            crate::report_error!(format!("{error}"), "function", "disable_2fa_user");

            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "response": "internal server error" })),
            ))
        }

        Err(error) => {
            crate::report_error!(format!("{error}"), "function", "disable_2fa_user()");

            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "response": "internal server error" })),
            ))
        }
    }
}

pub async fn check_2fa_status(
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
        tokio::task::spawn_blocking(move || -> Result<Option<String>, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                let query = format!(r#"SELECT totp_secret FROM users WHERE {filter} = ?1"#);

                let mut statement = connection.prepare(&query)?;

                let row = statement.query_row(params![request.value], |row| {
                    let totp_secret: Option<String> = row.get("totp_secret")?;

                    Ok(totp_secret)
                })?;

                Ok(row)
            })
        })
        .await;

    match database_result {
        Ok(Ok(totp_secret)) => match totp_secret {
            Some(_) => Ok((StatusCode::OK, Json(json!({ "response": "2fa available" })))),
            None => Ok((
                StatusCode::OK,
                Json(json!({ "response": "2fa not available" })),
            )),
        },

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "response": "invalid credentials" })),
        )),

        Ok(Err(error)) => {
            crate::report_error!(format!("{error}"), "function", "check_2fa_status()");

            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "response": "internal server error" })),
            ))
        }

        Err(error) => {
            crate::report_error!(format!("{error}"), "function", "check_2fa_status()");

            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "response": "internal server error" })),
            ))
        }
    }
}

async fn update_totp_secret_by_uid(
    email: String,
    uid: String,
) -> Result<Vec<u8>, TotpSecretUpdateError> {
    let secret = generate_random_base32(20);
    let label = format!("ARIS:{}", email);
    let issuer = "ARIS";
    let totp_uri = format!(
        "otpauth://totp/{}?secret={}&issuer={}",
        label, secret, issuer
    );
    let database_result =
        tokio::task::spawn_blocking(move || -> Result<usize, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                let query = format!(
                    r#"
                            UPDATE users SET
                                totp_secret = COALESCE(?1, totp_secret)
                            WHERE uid = ?2
                        "#
                );

                let updated_rows = connection.execute(&query, params![&secret, &uid])?;

                Ok(updated_rows)
            })
        })
        .await;

    match database_result {
        Ok(Ok(updated_rows)) if updated_rows > 0 => match generate_qr_image(totp_uri).await {
            Ok(image) => {
                let mut buffer = Cursor::new(Vec::new());
                if let Err(error) = image.write_to(&mut buffer, ImageFormat::Png) {
                    crate::report_error!(
                        format!("{}", error),
                        "function",
                        "update_totp_secret_by_uid()"
                    );
                }

                Ok(buffer.into_inner())
            }
            Err(error) => {
                crate::report_error!(
                    format!("{}", error),
                    "function",
                    "update_totp_secret_by_uid()"
                );

                Err(TotpSecretUpdateError::QrGeneration(format!("{error}")))
            }
        },
        Ok(Ok(_)) => Err(TotpSecretUpdateError::UserNotFound),
        Ok(Err(error)) => Err(TotpSecretUpdateError::Internal(format!("{error}"))),
        Err(error) => Err(TotpSecretUpdateError::Internal(format!("{error}"))),
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

    match execute_sql_qeury(query).await {
        SqliteQueryError::Result(result) => Ok((StatusCode::OK, Json(result))),
        SqliteQueryError::Error(err_value) => Err((StatusCode::BAD_REQUEST, Json(err_value))),
        SqliteQueryError::BadRequest(message) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "response":  format!("{message}")})),
        )),
        SqliteQueryError::InternalError => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "response": "internal server error" })),
        )),
    }
}
