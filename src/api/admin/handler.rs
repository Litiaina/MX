use std::io::Cursor;

use axum::{
    Json,
    http::{
        StatusCode,
        header::CONTENT_TYPE,
    },
    response::{
        IntoResponse,
        Response,
    },
};
use image::ImageFormat;
use rusqlite::params;
use serde_json::json;
use uuid::Uuid;

use crate::{
    api::{
        admin::model::{
            CreateUserRequest,
            ExecuteQueryRequest,
            FilterRequest,
            ModifySuperUserRequest,
            UserSummary,
        },
        api_error::SqliteQueryError,
        query_handler::execute_sql_qeury,
    },
    db::connector::{
        SqliteDatabaseError,
        with_sql_connection,
    },
    middleware::{
        auth::{
            ACCESS_ADMINISTRATOR,
            AuthenticateRequest,
            Claims,
            access_level_name,
            valid_access_level,
        },
        totp::verify_totp,
    },
    util::{
        qr::generate_qr_image,
        randomizer::generate_random_base32,
    },
};

fn json_response(
    status: StatusCode,
    body: serde_json::Value,
) -> Response {
    (
        status,
        Json(body),
    )
        .into_response()
}

fn require_admin(
    claims: &Claims,
) -> Result<(), Response> {
    if claims
        .can_manage_accounts()
    {
        Ok(())
    } else {
        Err(
            json_response(
                StatusCode::FORBIDDEN,

                json!({
                    "response":
                        "access denied"
                }),
            ),
        )
    }
}

/*
ACCESS LEVELS

0 = Administrator
1 = DGS Manager
2 = DGS Encoder
3 = Viewer
*/

pub async fn create_super_user(
    Json(request):
        Json<CreateUserRequest>,
) -> Response {
    if request
        .email
        .trim()
        .is_empty()
        || request
            .password
            .is_empty()
        || request
            .name
            .trim()
            .is_empty()
    {
        return json_response(
            StatusCode::BAD_REQUEST,

            json!({
                "response":
                    "name, email, and password are required"
            }),
        );
    }

    let created_uid =
        Uuid::new_v4()
            .to_string();

    let created_at =
        chrono::Utc::now()
            .timestamp();

    let email =
        request
            .email
            .trim()
            .to_string();

    let password =
        request.password;

    let name =
        request
            .name
            .trim()
            .to_string();

    /*
     * Bootstrap is transactional.
     *
     * Only the FIRST account may
     * be created through this route.
     *
     * access_level supplied by the
     * browser is ignored.
     *
     * First account is ALWAYS
     * Administrator.
     */
    let database_result =
        tokio::task::spawn_blocking(
            move ||
                -> Result<
                    bool,
                    SqliteDatabaseError,
                >
            {
                with_sql_connection(
                    |connection| {
                        let transaction =
                            connection
                                .unchecked_transaction()?;

                        let user_count:
                            i64 =
                            transaction
                                .query_row(
                                    "SELECT COUNT(*) FROM users",
                                    [],
                                    |row| {
                                        row.get(
                                            0
                                        )
                                    },
                                )?;

                        if user_count
                            > 0
                        {
                            return Ok(
                                false
                            );
                        }

                        transaction
                            .execute(
                                r#"
                                INSERT INTO users (
                                    uid,
                                    email,
                                    password,
                                    name,
                                    created_at,
                                    access_level,
                                    totp_secret
                                )
                                VALUES (
                                    ?1,
                                    ?2,
                                    ?3,
                                    ?4,
                                    ?5,
                                    ?6,
                                    NULL
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

                        transaction
                            .commit()?;

                        Ok(true)
                    },
                )
            },
        )
        .await;

    match database_result {
        Ok(Ok(true)) => {
            json_response(
                StatusCode::CREATED,

                json!({
                    "response":
                        "administrator created successfully"
                }),
            )
        }

        Ok(Ok(false)) => {
            json_response(
                StatusCode::CONFLICT,

                json!({
                    "response":
                        "Lux is already initialized"
                }),
            )
        }

        Ok(Err(
            SqliteDatabaseError::Sqlite(
                rusqlite::Error::SqliteFailure(
                    error,
                    _,
                ),
            ),
        ))
            if error.code
                == rusqlite::ErrorCode::ConstraintViolation =>
        {
            json_response(
                StatusCode::CONFLICT,

                json!({
                    "response":
                        "administrator account already exists"
                }),
            )
        }

        Ok(Err(error)) => {
            crate::report_error!(
                format!(
                    "failed to create bootstrap administrator: {}",
                    error
                ),
                "function",
                "create_super_user()"
            );

            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,

                json!({
                    "error":
                        "internal server error"
                }),
            )
        }

        Err(error) => {
            crate::report_error!(
                format!(
                    "bootstrap administrator blocking task failed: {}",
                    error
                ),
                "function",
                "create_super_user()"
            );

            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,

                json!({
                    "error":
                        "internal server error"
                }),
            )
        }
    }
}

pub async fn get_user(
    claims: Claims,
    Json(request):
        Json<FilterRequest>,
) -> Response {
    if let Err(response) =
        require_admin(
            &claims
        )
    {
        return response;
    }

    let filter =
        match request
            .filter
            .as_str()
        {
            "uid" => {
                Some("uid")
            }

            "email" => {
                Some("email")
            }

            "all" => {
                None
            }

            _ => {
                return json_response(
                    StatusCode::BAD_REQUEST,

                    json!({
                        "response":
                            format!(
                                "'{}' is an invalid filter. Use 'uid', 'email', or 'all'",
                                request.filter
                            )
                    }),
                );
            }
        };

    let value =
        request.value;

    let database_result =
        tokio::task::spawn_blocking(
            move ||
                -> Result<
                    Vec<UserSummary>,
                    SqliteDatabaseError,
                >
            {
                with_sql_connection(
                    |connection| {
                        let mut users =
                            Vec::new();

                        match filter {
                            Some(
                                filter
                            ) => {
                                let query =
                                    format!(
                                        r#"
                                        SELECT
                                            uid,
                                            email,
                                            name,
                                            access_level,
                                            totp_secret
                                        FROM users
                                        WHERE {} = ?1
                                        LIMIT 1
                                        "#,
                                        filter
                                    );

                                let mut statement =
                                    connection
                                        .prepare(
                                            &query
                                        )?;

                                let rows =
                                    statement
                                        .query_map(
                                            params![
                                                value
                                            ],
                                            |row| {
                                                let access_level:
                                                    i64 =
                                                    row.get(
                                                        "access_level"
                                                    )?;

                                                let totp_secret:
                                                    Option<String> =
                                                    row.get(
                                                        "totp_secret"
                                                    )?;

                                                Ok(
                                                    UserSummary {
                                                        uid:
                                                            row.get(
                                                                "uid"
                                                            )?,

                                                        email:
                                                            row.get(
                                                                "email"
                                                            )?,

                                                        name:
                                                            row.get(
                                                                "name"
                                                            )?,

                                                        access_level,

                                                        access_name:
                                                            access_level_name(
                                                                access_level
                                                            )
                                                            .to_string(),

                                                        totp_enabled:
                                                            totp_secret
                                                                .is_some(),
                                                    },
                                                )
                                            },
                                        )?;

                                for row
                                    in rows
                                {
                                    users.push(
                                        row?
                                    );
                                }
                            }

                            None => {
                                let mut statement =
                                    connection
                                        .prepare(
                                            r#"
                                            SELECT
                                                uid,
                                                email,
                                                name,
                                                access_level,
                                                totp_secret
                                            FROM users
                                            ORDER BY email ASC
                                            "#,
                                        )?;

                                let rows =
                                    statement
                                        .query_map(
                                            [],
                                            |row| {
                                                let access_level:
                                                    i64 =
                                                    row.get(
                                                        "access_level"
                                                    )?;

                                                let totp_secret:
                                                    Option<String> =
                                                    row.get(
                                                        "totp_secret"
                                                    )?;

                                                Ok(
                                                    UserSummary {
                                                        uid:
                                                            row.get(
                                                                "uid"
                                                            )?,

                                                        email:
                                                            row.get(
                                                                "email"
                                                            )?,

                                                        name:
                                                            row.get(
                                                                "name"
                                                            )?,

                                                        access_level,

                                                        access_name:
                                                            access_level_name(
                                                                access_level
                                                            )
                                                            .to_string(),

                                                        totp_enabled:
                                                            totp_secret
                                                                .is_some(),
                                                    },
                                                )
                                            },
                                        )?;

                                for row
                                    in rows
                                {
                                    users.push(
                                        row?
                                    );
                                }
                            }
                        }

                        Ok(users)
                    },
                )
            },
        )
        .await;

    match database_result {
        Ok(Ok(users)) => {
            json_response(
                StatusCode::OK,

                json!({
                    "users":
                        users
                }),
            )
        }

        Ok(Err(error)) => {
            crate::report_error!(
                format!(
                    "failed to retrieve users: {}",
                    error
                ),
                "function",
                "get_user()"
            );

            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,

                json!({
                    "error":
                        "internal server error"
                }),
            )
        }

        Err(error) => {
            crate::report_error!(
                format!(
                    "user retrieval blocking task failed: {}",
                    error
                ),
                "function",
                "get_user()"
            );

            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,

                json!({
                    "error":
                        "internal server error"
                }),
            )
        }
    }
}

pub async fn modify_super_user(
    claims: Claims,
    Json(request):
        Json<ModifySuperUserRequest>,
) -> Response {
    if let Err(response) =
        require_admin(
            &claims
        )
    {
        return response;
    }

    if let Some(
        access_level
    ) = request.access_level
    {
        if !valid_access_level(
            access_level
        ) {
            return json_response(
                StatusCode::BAD_REQUEST,

                json!({
                    "response":
                        "invalid access level; allowed values are 0, 1, 2, and 3"
                }),
            );
        }

        let changing_self =
            match request
                .filter
                .as_str()
            {
                "uid" => {
                    request.value
                        == claims.uid
                }

                "email" => {
                    request.value
                        == claims.email
                }

                _ => false,
            };

        if changing_self
            && access_level
                != claims.access_level
        {
            return json_response(
                StatusCode::BAD_REQUEST,

                json!({
                    "response":
                        "administrator cannot change their own access level"
                }),
            );
        }
    }

    let filter =
        match request
            .filter
            .as_str()
        {
            "uid" => "uid",
            "email" => "email",

            _ => {
                return json_response(
                    StatusCode::BAD_REQUEST,

                    json!({
                        "response":
                            format!(
                                "'{}' is an invalid filter. Use 'uid' or 'email'",
                                request.filter
                            )
                    }),
                );
            }
        };

    let database_result =
        tokio::task::spawn_blocking(
            move ||
                -> Result<
                    usize,
                    SqliteDatabaseError,
                >
            {
                with_sql_connection(
                    |connection| {
                        let query =
                            format!(
                                r#"
                                UPDATE users
                                SET
                                    email =
                                        COALESCE(
                                            ?1,
                                            email
                                        ),

                                    password =
                                        COALESCE(
                                            ?2,
                                            password
                                        ),

                                    name =
                                        COALESCE(
                                            ?3,
                                            name
                                        ),

                                    access_level =
                                        COALESCE(
                                            ?4,
                                            access_level
                                        )

                                WHERE {filter} = ?5
                                "#
                            );

                        connection.execute(
                            &query,

                            params![
                                request.new_email,
                                request.new_password,
                                request.new_name,
                                request.access_level,
                                request.value,
                            ],
                        )
                    },
                )
            },
        )
        .await;

    match database_result {
        Ok(Ok(
            updated_rows
        ))
            if updated_rows
                > 0 =>
        {
            json_response(
                StatusCode::OK,

                json!({
                    "response":
                        "user modified successfully"
                }),
            )
        }

        Ok(Ok(_)) => {
            json_response(
                StatusCode::NOT_FOUND,

                json!({
                    "error":
                        "user not found"
                }),
            )
        }

        Ok(Err(
            SqliteDatabaseError::Sqlite(
                rusqlite::Error::SqliteFailure(
                    error,
                    _,
                ),
            ),
        ))
            if error.code
                == rusqlite::ErrorCode::ConstraintViolation =>
        {
            json_response(
                StatusCode::CONFLICT,

                json!({
                    "response":
                        "email already exists"
                }),
            )
        }

        Ok(Err(error)) => {
            crate::report_error!(
                format!(
                    "failed to modify user: {}",
                    error
                ),
                "function",
                "modify_super_user()"
            );

            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,

                json!({
                    "error":
                        "internal server error"
                }),
            )
        }

        Err(error) => {
            crate::report_error!(
                format!(
                    "user modification blocking task failed: {}",
                    error
                ),
                "function",
                "modify_super_user()"
            );

            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,

                json!({
                    "error":
                        "internal server error"
                }),
            )
        }
    }
}

pub async fn delete_super_user(
    claims: Claims,
    Json(request):
        Json<FilterRequest>,
) -> Response {
    if let Err(response) =
        require_admin(
            &claims
        )
    {
        return response;
    }

    let deleting_self =
        match request
            .filter
            .as_str()
        {
            "uid" => {
                request.value
                    == claims.uid
            }

            "email" => {
                request.value
                    == claims.email
            }

            _ => false,
        };

    if deleting_self {
        return json_response(
            StatusCode::BAD_REQUEST,

            json!({
                "response":
                    "administrator cannot delete their own account from account management"
            }),
        );
    }

    let filter =
        match request
            .filter
            .as_str()
        {
            "uid" => "uid",
            "email" => "email",

            _ => {
                return json_response(
                    StatusCode::BAD_REQUEST,

                    json!({
                        "response":
                            format!(
                                "'{}' is an invalid filter. Use 'uid' or 'email'",
                                request.filter
                            )
                    }),
                );
            }
        };

    let database_result =
        tokio::task::spawn_blocking(
            move ||
                -> Result<
                    usize,
                    SqliteDatabaseError,
                >
            {
                with_sql_connection(
                    |connection| {
                        let query =
                            format!(
                                "DELETE FROM users WHERE {filter} = ?1"
                            );

                        connection.execute(
                            &query,
                            params![
                                request.value
                            ],
                        )
                    },
                )
            },
        )
        .await;

    match database_result {
        Ok(Ok(
            updated_rows
        ))
            if updated_rows
                > 0 =>
        {
            json_response(
                StatusCode::OK,

                json!({
                    "response":
                        "user deleted successfully"
                }),
            )
        }

        Ok(Ok(_)) => {
            json_response(
                StatusCode::NOT_FOUND,

                json!({
                    "error":
                        "user not found"
                }),
            )
        }

        Ok(Err(error)) => {
            crate::report_error!(
                format!(
                    "failed to delete user: {}",
                    error
                ),
                "function",
                "delete_super_user()"
            );

            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,

                json!({
                    "error":
                        "internal server error"
                }),
            )
        }

        Err(error) => {
            crate::report_error!(
                format!(
                    "user deletion blocking task failed: {}",
                    error
                ),
                "function",
                "delete_super_user()"
            );

            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,

                json!({
                    "error":
                        "internal server error"
                }),
            )
        }
    }
}

pub async fn enable_2fa_user(
    claims: Claims,
    Json(request):
        Json<AuthenticateRequest>,
) -> Response {
    if let Err(response) =
        require_admin(
            &claims
        )
    {
        return response;
    }

    if request
        .email
        .trim()
        .is_empty()
        || request
            .password
            .is_empty()
    {
        return json_response(
            StatusCode::BAD_REQUEST,

            json!({
                "response":
                    "email and password are required"
            }),
        );
    }

    let email =
        request
            .email
            .trim()
            .to_string();

    let password =
        request.password;

    let otp =
        request.otp;

    let lookup_email =
        email.clone();

    let database_result =
        tokio::task::spawn_blocking(
            move ||
                -> Result<
                    (
                        String,
                        Option<String>,
                    ),
                    SqliteDatabaseError,
                >
            {
                with_sql_connection(
                    |connection| {
                        connection
                            .query_row(
                                r#"
                                SELECT
                                    uid,
                                    totp_secret
                                FROM users
                                WHERE
                                    email = ?1
                                    AND password = ?2
                                LIMIT 1
                                "#,
                                params![
                                    lookup_email,
                                    password
                                ],
                                |row| {
                                    Ok((
                                        row.get(
                                            "uid"
                                        )?,

                                        row.get(
                                            "totp_secret"
                                        )?,
                                    ))
                                },
                            )
                    },
                )
            },
        )
        .await;

    let (
        uid,
        current_secret,
    ) =
        match database_result {
            Ok(Ok(value)) => {
                value
            }

            Ok(Err(
                SqliteDatabaseError::Sqlite(
                    rusqlite::Error::QueryReturnedNoRows,
                ),
            )) => {
                return json_response(
                    StatusCode::UNAUTHORIZED,

                    json!({
                        "response":
                            "invalid credentials"
                    }),
                );
            }

            Ok(Err(error)) => {
                crate::report_error!(
                    format!(
                        "failed to find user for 2FA enable: {}",
                        error
                    ),
                    "function",
                    "enable_2fa_user()"
                );

                return json_response(
                    StatusCode::INTERNAL_SERVER_ERROR,

                    json!({
                        "response":
                            "internal server error"
                    }),
                );
            }

            Err(error) => {
                crate::report_error!(
                    format!(
                        "2FA enable lookup task failed: {}",
                        error
                    ),
                    "function",
                    "enable_2fa_user()"
                );

                return json_response(
                    StatusCode::INTERNAL_SERVER_ERROR,

                    json!({
                        "response":
                            "internal server error"
                    }),
                );
            }
        };

    /*
     * If 2FA already exists,
     * verify the old OTP before
     * rotating the secret.
     */
    if let Some(secret) =
        current_secret
    {
        let Some(otp) =
            otp
        else {
            return json_response(
                StatusCode::BAD_REQUEST,

                json!({
                    "response":
                        "otp is required"
                }),
            );
        };

        if !verify_totp(
            &secret,
            &otp,
            1,
        ) {
            return json_response(
                StatusCode::UNAUTHORIZED,

                json!({
                    "response":
                        "invalid credentials"
                }),
            );
        }
    }

    match update_totp_secret_by_uid(
        email,
        uid,
    )
    .await
    {
        Ok(image) => {
            (
                StatusCode::OK,

                [
                    (
                        CONTENT_TYPE,
                        "image/png",
                    ),
                ],

                image,
            )
                .into_response()
        }

        Err(error) => {
            crate::report_error!(
                error,
                "function",
                "enable_2fa_user()"
            );

            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,

                json!({
                    "response":
                        "internal server error"
                }),
            )
        }
    }
}

pub async fn disable_2fa_user(
    claims: Claims,
    Json(request):
        Json<AuthenticateRequest>,
) -> Response {
    if let Err(response) =
        require_admin(
            &claims
        )
    {
        return response;
    }

    if request
        .email
        .trim()
        .is_empty()
        || request
            .password
            .is_empty()
    {
        return json_response(
            StatusCode::BAD_REQUEST,

            json!({
                "response":
                    "email and password are required"
            }),
        );
    }

    let email =
        request
            .email
            .trim()
            .to_string();

    let password =
        request.password;

    let otp =
        request.otp;

    let database_result =
        tokio::task::spawn_blocking(
            move ||
                -> Result<
                    (
                        String,
                        Option<String>,
                    ),
                    SqliteDatabaseError,
                >
            {
                with_sql_connection(
                    |connection| {
                        connection
                            .query_row(
                                r#"
                                SELECT
                                    uid,
                                    totp_secret
                                FROM users
                                WHERE
                                    email = ?1
                                    AND password = ?2
                                LIMIT 1
                                "#,
                                params![
                                    email,
                                    password
                                ],
                                |row| {
                                    Ok((
                                        row.get(
                                            "uid"
                                        )?,

                                        row.get(
                                            "totp_secret"
                                        )?,
                                    ))
                                },
                            )
                    },
                )
            },
        )
        .await;

    let (
        uid,
        current_secret,
    ) =
        match database_result {
            Ok(Ok(value)) => {
                value
            }

            Ok(Err(
                SqliteDatabaseError::Sqlite(
                    rusqlite::Error::QueryReturnedNoRows,
                ),
            )) => {
                return json_response(
                    StatusCode::UNAUTHORIZED,

                    json!({
                        "response":
                            "invalid credentials"
                    }),
                );
            }

            Ok(Err(error)) => {
                crate::report_error!(
                    format!(
                        "failed to find user for 2FA disable: {}",
                        error
                    ),
                    "function",
                    "disable_2fa_user()"
                );

                return json_response(
                    StatusCode::INTERNAL_SERVER_ERROR,

                    json!({
                        "response":
                            "internal server error"
                    }),
                );
            }

            Err(error) => {
                crate::report_error!(
                    format!(
                        "2FA disable lookup task failed: {}",
                        error
                    ),
                    "function",
                    "disable_2fa_user()"
                );

                return json_response(
                    StatusCode::INTERNAL_SERVER_ERROR,

                    json!({
                        "response":
                            "internal server error"
                    }),
                );
            }
        };

    let Some(secret) =
        current_secret
    else {
        return json_response(
            StatusCode::OK,

            json!({
                "response":
                    "2fa already disabled"
            }),
        );
    };

    let Some(otp) =
        otp
    else {
        return json_response(
            StatusCode::BAD_REQUEST,

            json!({
                "response":
                    "otp is required"
            }),
        );
    };

    if !verify_totp(
        &secret,
        &otp,
        1,
    ) {
        return json_response(
            StatusCode::UNAUTHORIZED,

            json!({
                "response":
                    "invalid credentials"
            }),
        );
    }

    let database_result =
        tokio::task::spawn_blocking(
            move ||
                -> Result<
                    usize,
                    SqliteDatabaseError,
                >
            {
                with_sql_connection(
                    |connection| {
                        connection.execute(
                            "UPDATE users SET totp_secret = NULL WHERE uid = ?1",

                            params![
                                uid
                            ],
                        )
                    },
                )
            },
        )
        .await;

    match database_result {
        Ok(Ok(
            updated_rows
        ))
            if updated_rows
                > 0 =>
        {
            json_response(
                StatusCode::OK,

                json!({
                    "response":
                        "2fa disabled successfully"
                }),
            )
        }

        Ok(Ok(_)) => {
            json_response(
                StatusCode::NOT_FOUND,

                json!({
                    "response":
                        "user not found"
                }),
            )
        }

        Ok(Err(error)) => {
            crate::report_error!(
                format!(
                    "failed to disable 2FA: {}",
                    error
                ),
                "function",
                "disable_2fa_user()"
            );

            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,

                json!({
                    "response":
                        "internal server error"
                }),
            )
        }

        Err(error) => {
            crate::report_error!(
                format!(
                    "2FA disable update task failed: {}",
                    error
                ),
                "function",
                "disable_2fa_user()"
            );

            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,

                json!({
                    "response":
                        "internal server error"
                }),
            )
        }
    }
}

pub async fn check_2fa_status(
    claims: Claims,
    Json(request):
        Json<FilterRequest>,
) -> Response {
    if let Err(response) =
        require_admin(
            &claims
        )
    {
        return response;
    }

    let filter =
        match request
            .filter
            .as_str()
        {
            "uid" => "uid",
            "email" => "email",

            _ => {
                return json_response(
                    StatusCode::BAD_REQUEST,

                    json!({
                        "response":
                            format!(
                                "'{}' is an invalid filter. Use 'uid' or 'email'",
                                request.filter
                            )
                    }),
                );
            }
        };

    let value =
        request.value;

    let database_result =
        tokio::task::spawn_blocking(
            move ||
                -> Result<
                    Option<String>,
                    SqliteDatabaseError,
                >
            {
                with_sql_connection(
                    |connection| {
                        let query =
                            format!(
                                "SELECT totp_secret FROM users WHERE {filter} = ?1 LIMIT 1"
                            );

                        connection.query_row(
                            &query,

                            params![
                                value
                            ],

                            |row| {
                                row.get(
                                    "totp_secret"
                                )
                            },
                        )
                    },
                )
            },
        )
        .await;

    match database_result {
        Ok(Ok(
            Some(_)
        )) => {
            json_response(
                StatusCode::OK,

                json!({
                    "response":
                        "2fa available"
                }),
            )
        }

        Ok(Ok(None)) => {
            json_response(
                StatusCode::OK,

                json!({
                    "response":
                        "2fa not available"
                }),
            )
        }

        Ok(Err(
            SqliteDatabaseError::Sqlite(
                rusqlite::Error::QueryReturnedNoRows,
            ),
        )) => {
            json_response(
                StatusCode::NOT_FOUND,

                json!({
                    "response":
                        "user not found"
                }),
            )
        }

        Ok(Err(error)) => {
            crate::report_error!(
                format!(
                    "failed to check 2FA status: {}",
                    error
                ),
                "function",
                "check_2fa_status()"
            );

            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,

                json!({
                    "response":
                        "internal server error"
                }),
            )
        }

        Err(error) => {
            crate::report_error!(
                format!(
                    "2FA status task failed: {}",
                    error
                ),
                "function",
                "check_2fa_status()"
            );

            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,

                json!({
                    "response":
                        "internal server error"
                }),
            )
        }
    }
}

async fn update_totp_secret_by_uid(
    email: String,
    uid: String,
) -> Result<
    Vec<u8>,
    String,
> {
    let secret =
        generate_random_base32(
            20
        );

    let label =
        format!(
            "Litiaina:{}",
            email
        );

    let issuer =
        "Litiaina";

    let totp_uri =
        format!(
            "otpauth://totp/{}?secret={}&issuer={}",
            label,
            secret,
            issuer
        );

    let database_result =
        tokio::task::spawn_blocking(
            move ||
                -> Result<
                    usize,
                    SqliteDatabaseError,
                >
            {
                with_sql_connection(
                    |connection| {
                        connection.execute(
                            "UPDATE users SET totp_secret = ?1 WHERE uid = ?2",

                            params![
                                secret,
                                uid
                            ],
                        )
                    },
                )
            },
        )
        .await;

    match database_result {
        Ok(Ok(
            updated_rows
        ))
            if updated_rows
                > 0 =>
        {
            let image =
                generate_qr_image(
                    totp_uri
                )
                .await
                .map_err(
                    |error| {
                        format!(
                            "QR generation failed: {}",
                            error
                        )
                    },
                )?;

            let mut buffer =
                Cursor::new(
                    Vec::new()
                );

            image
                .write_to(
                    &mut buffer,
                    ImageFormat::Png,
                )
                .map_err(
                    |error| {
                        format!(
                            "failed to encode QR image: {}",
                            error
                        )
                    },
                )?;

            Ok(
                buffer
                    .into_inner()
            )
        }

        Ok(Ok(_)) => {
            Err(
                "user not found"
                    .to_string()
            )
        }

        Ok(Err(error)) => {
            Err(
                format!(
                    "failed to save TOTP secret: {}",
                    error
                ),
            )
        }

        Err(error) => {
            Err(
                format!(
                    "TOTP update blocking task failed: {}",
                    error
                ),
            )
        }
    }
}

pub async fn execute_query(
    claims: Claims,
    Json(request):
        Json<ExecuteQueryRequest>,
) -> Response {
    if let Err(response) =
        require_admin(
            &claims
        )
    {
        return response;
    }

    match execute_sql_qeury(
        request.query
    )
    .await
    {
        SqliteQueryError::Result(
            result
        ) => {
            (
                StatusCode::OK,
                Json(result),
            )
                .into_response()
        }

        SqliteQueryError::Error(
            error
        ) => {
            (
                StatusCode::BAD_REQUEST,
                Json(error),
            )
                .into_response()
        }

        SqliteQueryError::BadRequest(
            message
        ) => {
            json_response(
                StatusCode::BAD_REQUEST,

                json!({
                    "response":
                        message
                }),
            )
        }

        SqliteQueryError::InternalError => {
            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,

                json!({
                    "response":
                        "internal server error"
                }),
            )
        }
    }
}
