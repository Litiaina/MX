use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::json;

use crate::{
    api::mx::{handler::ensure_mx_record_schema, schema::schema_revision_db},
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::Claims,
};

#[derive(Debug, Serialize)]
pub struct RevisionResponse {
    pub records_revision: u64,
    pub schema_revision: u64,
}

fn current_revision(connection: &Connection) -> Result<u64, rusqlite::Error> {
    let revision: i64 = connection.query_row(
        r#"
        SELECT records_revision
        FROM mx_revision
        WHERE id = 1
        "#,
        [],
        |row| row.get(0),
    )?;

    Ok(revision.max(0) as u64)
}

fn access_denied() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "response": "This account does not have record access."
        })),
    )
        .into_response()
}

fn internal_error(function_name: &str, error: impl std::fmt::Display) -> Response {
    crate::report_error!(format!("{error}"), "dashboard", function_name);

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "response": "The MX revision state could not be read."
        })),
    )
        .into_response()
}

/// Lightweight change token used by the browser workspaces.
///
/// Dashboard contents are intentionally not generated here. The dashboard is
/// assembled exclusively from administrator-defined widgets in `reports.rs`.
pub async fn get_records_revision(claims: Claims) -> Response {
    if !claims.can_read_records() {
        return access_denied();
    }

    let result = tokio::task::spawn_blocking(move || -> Result<(u64, u64), SqliteDatabaseError> {
        with_sql_connection(|connection| {
            ensure_mx_record_schema(connection)?;
            let records_revision = current_revision(connection)?;
            let schema_revision = schema_revision_db(connection)?;
            Ok((records_revision, schema_revision))
        })
    })
    .await;

    match result {
        Ok(Ok((records_revision, schema_revision))) => (
            StatusCode::OK,
            Json(RevisionResponse {
                records_revision,
                schema_revision,
            }),
        )
            .into_response(),
        Ok(Err(error)) => internal_error("get_records_revision()", error),
        Err(error) => internal_error("get_records_revision()", error),
    }
}
