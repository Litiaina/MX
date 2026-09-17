use std::sync::Arc;

use data_encoding::BASE64;
use rusqlite::{params, types::ValueRef};
use serde_json::{Value, json};

use crate::{
    api::{
        admin::model::User,
        api_error::{AuthError, SqliteError, SqliteQueryError},
        user::model::{NewUserData, QueryFilter},
    },
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::totp::verify_totp,
};

pub async fn execute_create_user(
    user: User,
    content_type: &str,
    function_name: &str,
) -> Result<(), SqliteError> {
    let new_user = user;

    let query = format!(
        r#"INSERT INTO users (
        uid,
        email,
        password,
        name,
        created_at,
        access_level,
        totp_secret
        ) VALUES (
        ?1,
        ?2,
        ?3,
        ?4,
        ?5,
        ?6,
        ?7)"#
    );

    let database_result =
        tokio::task::spawn_blocking(move || -> Result<(), SqliteDatabaseError> {
            with_sql_connection(|connection| {
                connection.execute(
                    &query,
                    params![
                        new_user.uid,
                        new_user.email,
                        new_user.password,
                        new_user.name,
                        new_user.created_at,
                        new_user.access_level,
                        new_user.totp_secret
                    ],
                )?;

                Ok(())
            })
        })
        .await;

    match database_result {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::SqliteFailure(error, _))))
            if error.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE =>
        {
            Err(SqliteError::Conflict)
        }
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => {
            Err(SqliteError::NotFound)
        }
        Ok(Err(error)) => {
            crate::report_error!(format!("{error}"), content_type, function_name);

            Err(SqliteError::SqliteDatabaseError)
        }

        Err(error) => {
            crate::report_error!(format!("{error}"), content_type, function_name);

            Err(SqliteError::JoinError)
        }
    }
}

pub async fn execute_modify_user(
    query_filter: QueryFilter,
    user: NewUserData,
    content_type: &str,
    function_name: &str,
) -> Result<(), SqliteError> {
    let filter = match query_filter.filter.as_str() {
        "uid" => "uid",
        "email" => "email",
        _ => {
            return Err(SqliteError::InvalidFilter);
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
                        user.new_email,
                        user.new_password,
                        user.new_name,
                        user.access_level,
                        query_filter.value,
                    ],
                )?;

                Ok(updated_rows)
            })
        })
        .await;

    match database_result {
        Ok(Ok(updated_rows)) if updated_rows > 0 => Ok(()),
        Ok(Ok(_)) => Err(SqliteError::NotFound),
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::SqliteFailure(error, _))))
            if error.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE =>
        {
            Err(SqliteError::Conflict)
        }
        Ok(Err(error)) => {
            crate::report_error!(
                format!("failed to modify user: {}", error),
                content_type,
                function_name
            );

            Err(SqliteError::SqliteDatabaseError)
        }
        Err(error) => {
            crate::report_error!(
                format!("user modification blocking task failed: {}", error),
                content_type,
                function_name
            );

            Err(SqliteError::JoinError)
        }
    }
}

pub async fn execute_sql_qeury(query: String) -> SqliteQueryError {
    let database_result =
        tokio::task::spawn_blocking(move || -> Result<Value, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                let mut statement = connection.prepare(&query)?;

                let column_count = statement.column_count();

                if column_count > 0 {
                    let columns = statement
                        .column_names()
                        .into_iter()
                        .map(ToString::to_string)
                        .collect::<Vec<String>>();

                    let mut query_rows = statement.query([])?;
                    let mut rows = Vec::new();

                    while let Some(row) = query_rows.next()? {
                        let mut values = Vec::with_capacity(column_count);

                        for index in 0..column_count {
                            let value = row.get_ref(index)?;
                            values.push(sqlite_value_to_json(value));
                        }

                        rows.push(values);
                    }

                    Ok(json!({
                        "columns": columns,
                        "row_count": rows.len(),
                        "rows": rows
                    }))
                } else {
                    let rows_affected = statement.execute([])?;

                    Ok(json!({
                        "rows_affected": rows_affected
                    }))
                }
            })
        })
        .await;

    match database_result {
        Ok(Ok(result)) => SqliteQueryError::Result(result),
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::SqliteFailure(error, message)))) => {
            SqliteQueryError::Error(json!({
                "error": {
                    "message": message,
                    "code": error.extended_code & 0xFF,
                    "extended_code": error.extended_code
                }
            }))
        }
        Ok(Err(error)) => SqliteQueryError::BadRequest(error.to_string()),
        Err(error) => {
            crate::report_error!(
                format!("query execution blocking task failed: {}", error),
                "function",
                "execute_query()"
            );
            SqliteQueryError::InternalError
        }
    }
}

pub async fn execute_delete_user(
    uid: String,
    content_type: &str,
    function_name: &str,
) -> Result<(), SqliteError> {
    let database_result =
        tokio::task::spawn_blocking(move || -> Result<usize, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                let query = format!(r#"DELETE FROM users WHERE uid = ?1"#);

                let updated_rows = connection.execute(&query, params![uid])?;

                Ok(updated_rows)
            })
        })
        .await;

    match database_result {
        Ok(Ok(updated_rows)) if updated_rows > 0 => Ok(()),
        Ok(Ok(_)) => Err(SqliteError::NotFound),
        Ok(Err(error)) => {
            crate::report_error!(format!("{error}"), content_type, function_name);
            Err(SqliteError::SqliteDatabaseError)
        }

        Err(error) => {
            crate::report_error!(
                format!("user modification blocking task failed: {}", error),
                content_type,
                function_name
            );
            Err(SqliteError::JoinError)
        }
    }
}

pub async fn verify_authentication(
    uid: Arc<String>,
    email: String,
    password: String,
    otp: Option<String>,
    content_type: &str,
    function_name: &str,
) -> AuthError {
    let database_result =
        tokio::task::spawn_blocking(move || -> Result<Option<String>, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                let mut statement = connection.prepare(
                    "SELECT totp_secret FROM users WHERE uid = ?1 AND email = ?2 AND password = ?3",
                )?;

                let row = statement.query_row(params![uid, email, password], |row| {
                    let totp_secret: Option<String> = row.get("totp_secret")?;

                    Ok(totp_secret)
                })?;

                Ok(row)
            })
        })
        .await;

    match database_result {
        Ok(Ok(totp_secret)) => match totp_secret {
            Some(value) => {
                if let Some(otp) = otp {
                    let valid = verify_totp(&value, otp.as_str(), 1);
                    if !valid {
                        AuthError::Unauthorized
                    } else {
                        AuthError::Ok
                    }
                } else {
                    return AuthError::MissingTotp;
                }
            }
            None => AuthError::Ok,
        },

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => {
            AuthError::Unauthorized
        }

        Ok(Err(error)) => {
            crate::report_error!(format!("{error}"), content_type, function_name);
            AuthError::Error
        }

        Err(error) => {
            crate::report_error!(format!("{error}"), content_type, function_name);
            AuthError::Error
        }
    }
}

fn sqlite_value_to_json(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => {
            json!({
                "type": "null",
                "value": null
            })
        }

        ValueRef::Integer(value) => {
            json!({
                "type": "integer",
                "value": value.to_string()
            })
        }

        ValueRef::Real(value) => {
            json!({
                "type": "real",
                "value": value.to_string()
            })
        }

        ValueRef::Text(value) => match std::str::from_utf8(value) {
            Ok(text) => {
                json!({
                    "type": "text",
                    "value": text
                })
            }

            Err(_) => {
                json!({
                    "type": "text",
                    "encoding": "base64",
                    "value": BASE64.encode(value)
                })
            }
        },

        ValueRef::Blob(value) => {
            json!({
                "type": "blob",
                "encoding": "base64",
                "value": BASE64.encode(value)
            })
        }
    }
}
