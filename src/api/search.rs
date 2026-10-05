use axum::{
    Json,
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    api::lifecycle::ensure_record_lifecycle_schema,
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::Claims,
};

#[derive(Debug, Deserialize)]
pub struct GlobalSearchQuery {
    pub q: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct GlobalSearchResult {
    uid: String,
    module_uid: String,
    module_name: String,
    singular_name: String,
    icon: String,
    color: String,
    label: String,
    context: String,
}

fn api_json(status: StatusCode, body: serde_json::Value) -> Response {
    (status, Json(body)).into_response()
}

fn like_pattern(value: &str) -> String {
    format!(
        "%{}%",
        value
            .replace('!', "!!")
            .replace('%', "!%")
            .replace('_', "!_")
    )
}

fn search_records(
    connection: &rusqlite::Connection,
    user_uid: &str,
    access_level: i64,
    query: &str,
    limit: usize,
) -> rusqlite::Result<Vec<GlobalSearchResult>> {
    ensure_record_lifecycle_schema(connection)?;
    let pattern = like_pattern(query);
    let mut statement = connection.prepare(
        r#"
        SELECT
            record.uid,
            record.module_uid,
            module.name,
            module.singular_name,
            module.icon,
            module.color,
            COALESCE((
                SELECT CASE field.field_type
                    WHEN 'integer' THEN CAST(value.value_integer AS TEXT)
                    WHEN 'auto_number' THEN CAST(value.value_integer AS TEXT)
                    WHEN 'decimal' THEN CAST(value.value_real AS TEXT)
                    WHEN 'formula' THEN CAST(value.value_real AS TEXT)
                    WHEN 'boolean' THEN CASE value.value_boolean WHEN 1 THEN 'Yes' ELSE 'No' END
                    ELSE NULLIF(TRIM(value.value_text), '')
                END
                FROM mx_record_values value
                JOIN mx_fields field ON field.uid = value.field_uid
                WHERE value.record_uid = record.uid
                  AND field.active = 1
                  AND field.field_type <> 'attachments'
                ORDER BY field.position, field.rowid
                LIMIT 1
            ), record.uid),
            COALESCE((
                SELECT field.label || ': ' || CASE field.field_type
                    WHEN 'integer' THEN CAST(value.value_integer AS TEXT)
                    WHEN 'auto_number' THEN CAST(value.value_integer AS TEXT)
                    WHEN 'decimal' THEN CAST(value.value_real AS TEXT)
                    WHEN 'formula' THEN CAST(value.value_real AS TEXT)
                    WHEN 'boolean' THEN CASE value.value_boolean WHEN 1 THEN 'Yes' ELSE 'No' END
                    ELSE COALESCE(value.value_text, '')
                END
                FROM mx_record_values value
                JOIN mx_fields field ON field.uid = value.field_uid
                WHERE value.record_uid = record.uid
                  AND field.active = 1
                  AND field.searchable = 1
                  AND LOWER(CASE field.field_type
                    WHEN 'integer' THEN CAST(value.value_integer AS TEXT)
                    WHEN 'auto_number' THEN CAST(value.value_integer AS TEXT)
                    WHEN 'decimal' THEN CAST(value.value_real AS TEXT)
                    WHEN 'formula' THEN CAST(value.value_real AS TEXT)
                    WHEN 'boolean' THEN CASE value.value_boolean WHEN 1 THEN 'true' ELSE 'false' END
                    ELSE COALESCE(value.value_text, '')
                  END) LIKE LOWER(?3) ESCAPE '!'
                ORDER BY field.position, field.rowid
                LIMIT 1
            ), (
                SELECT 'File: ' || attachment.file_name
                FROM mx_attachments attachment
                WHERE attachment.entry_uid = record.uid
                  AND permission.can_attachments=1
                  AND (?1=0 OR account.can_attachments=1)
                  AND LOWER(attachment.file_name) LIKE LOWER(?3) ESCAPE '!'
                ORDER BY attachment.rowid
                LIMIT 1
            ), 'Matched record')
        FROM mx_records record
        JOIN mx_modules module ON module.uid = record.module_uid
        JOIN mx_module_permissions permission
          ON permission.module_uid = record.module_uid
         AND permission.access_level = ?1
         AND permission.can_read = 1
        LEFT JOIN mx_module_account_permissions account
          ON account.module_uid=record.module_uid
         AND account.user_uid=?2
        WHERE module.active = 1
          AND record.deleted_at IS NULL
          AND (?1=0 OR account.can_read=1)
          AND (
              EXISTS (
                  SELECT 1
                  FROM mx_record_values value
                  JOIN mx_fields field ON field.uid = value.field_uid
                  WHERE value.record_uid = record.uid
                    AND field.active = 1
                    AND field.searchable = 1
                    AND LOWER(CASE field.field_type
                      WHEN 'integer' THEN CAST(value.value_integer AS TEXT)
                      WHEN 'auto_number' THEN CAST(value.value_integer AS TEXT)
                      WHEN 'decimal' THEN CAST(value.value_real AS TEXT)
                      WHEN 'formula' THEN CAST(value.value_real AS TEXT)
                      WHEN 'boolean' THEN CASE value.value_boolean WHEN 1 THEN 'true' ELSE 'false' END
                      ELSE COALESCE(value.value_text, '')
                    END) LIKE LOWER(?3) ESCAPE '!'
              )
              OR EXISTS (
                  SELECT 1
                  FROM mx_attachments attachment
                  WHERE attachment.entry_uid = record.uid
                    AND permission.can_attachments=1
                    AND (?1=0 OR account.can_attachments=1)
                    AND LOWER(attachment.file_name) LIKE LOWER(?3) ESCAPE '!'
              )
          )
        ORDER BY record.rowid DESC
        LIMIT ?4
        "#,
    )?;

    statement
        .query_map(
            params![access_level, user_uid, pattern, limit as i64],
            |row| {
                Ok(GlobalSearchResult {
                    uid: row.get(0)?,
                    module_uid: row.get(1)?,
                    module_name: row.get(2)?,
                    singular_name: row.get(3)?,
                    icon: row.get(4)?,
                    color: row.get(5)?,
                    label: row.get(6)?,
                    context: row.get(7)?,
                })
            },
        )?
        .collect()
}

pub async fn global_search(claims: Claims, Query(query): Query<GlobalSearchQuery>) -> Response {
    let value = query.q.trim().chars().take(200).collect::<String>();
    if value.chars().count() < 2 {
        return api_json(StatusCode::OK, json!({"results":[]}));
    }
    let access_level = claims.access_level;
    let user_uid = claims.uid;
    let limit = query.limit.unwrap_or(30).clamp(1, 50);
    let result = tokio::task::spawn_blocking(
        move || -> Result<Vec<GlobalSearchResult>, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                search_records(connection, &user_uid, access_level, &value, limit)
            })
        },
    )
    .await;

    match result {
        Ok(Ok(results)) => api_json(StatusCode::OK, json!({"results":results})),
        Ok(Err(error)) => {
            crate::report_error!(format!("{error}"), "search", "global_search()");
            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"global search failed"}),
            )
        }
        Err(error) => {
            crate::report_error!(format!("{error}"), "search", "global_search()");
            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"global search task failed"}),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::modules::DEFAULT_MODULE_UID;

    #[test]
    fn global_search_respects_module_read_permission() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        ensure_record_lifecycle_schema(&connection).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS users(uid TEXT PRIMARY KEY, access_level INTEGER NOT NULL);\n\
                 INSERT INTO users(uid,access_level) VALUES('viewer',3);",
            )
            .unwrap();
        connection
            .execute(
                r#"
                INSERT INTO mx_fields(
                    uid, field_key, label, field_type, required, unique_value,
                    searchable, sortable, table_visible, table_priority,
                    position, active, config_json, module_uid, module_key
                ) VALUES ('name-field', 'name_field', 'Name', 'text', 0, 0, 1, 1, 1, 50, 0, 1, '{}', ?1, 'name')
                "#,
                params![DEFAULT_MODULE_UID],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO mx_records(uid,control_year,control_no,date,office,requestor,subject,routed_to_div,remarks,module_uid) VALUES ('record-1',0,1,'','','','','','',?1)",
                params![DEFAULT_MODULE_UID],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO mx_record_values(record_uid,field_uid,value_text) VALUES ('record-1','name-field','William Luther')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO mx_module_account_permissions(module_uid,user_uid,can_read,can_create,can_update,can_delete,can_configure,can_report,can_attachments,updated_at) VALUES(?1,'viewer',1,0,0,0,0,1,1,0)",
                params![DEFAULT_MODULE_UID],
            )
            .unwrap();

        assert_eq!(
            search_records(&connection, "viewer", 3, "William", 10)
                .unwrap()
                .len(),
            1
        );
        connection
            .execute(
                "UPDATE mx_module_permissions SET can_read=0 WHERE module_uid=?1 AND access_level=3",
                params![DEFAULT_MODULE_UID],
            )
            .unwrap();
        assert!(
            search_records(&connection, "viewer", 3, "William", 10)
                .unwrap()
                .is_empty()
        );
    }
}
