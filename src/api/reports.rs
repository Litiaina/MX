use std::collections::HashMap;

use axum::{
    Json,
    body::Body,
    extract::Query,
    http::{
        HeaderValue, StatusCode,
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
    },
    response::{IntoResponse, Response},
};
use rusqlite::{Connection, params, params_from_iter, types::Value as SqlValue};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    api::live::publish_live_event,
    api::mx::{
        attachment_fields::{ensure_attachment_fields_schema, load_attachment_field_db},
        schema::{FieldDefinition, ensure_dynamic_schema, load_fields_db},
    },
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::Claims,
};

const MAX_EXPORT_ROWS: usize = 100_000;
const REPORT_ERROR_PREFIX: &str = "MX_REPORT:";

#[derive(Debug, Deserialize, Default, Clone)]
pub struct ReportQuery {
    pub group_field_uid: Option<String>,
    pub action_field_uid: Option<String>,
    pub action_mode: Option<String>,
    pub action_value: Option<String>,
    pub attachment_field_uid: Option<String>,
    pub date_field_uid: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub time_bucket: Option<String>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct UserPerformanceQuery {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ExportQuery {
    pub kind: String,
    pub group_field_uid: Option<String>,
    pub action_field_uid: Option<String>,
    pub action_mode: Option<String>,
    pub action_value: Option<String>,
    pub attachment_field_uid: Option<String>,
    pub date_field_uid: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub time_bucket: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SaveDashboardConfigRequest {
    pub config: Value,
}

#[derive(Debug, Serialize)]
pub struct ActionRateRow {
    pub group: String,
    pub total: u64,
    pub actioned: u64,
    pub pending: u64,
    pub rate: f64,
}

#[derive(Debug, Serialize, Clone)]
pub struct UserPerformanceRow {
    pub actor_uid: String,
    pub actor_name: String,
    pub actor_email: String,
    pub records_created: u64,
    pub records_updated: u64,
    pub attachments_uploaded: u64,
    pub records_deleted: u64,
    pub attachment_downloads: u64,
    pub successful_actions: u64,
    pub failed_actions: u64,
    pub unique_records_touched: u64,
    pub last_activity: i64,
}

fn api_json(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

fn report_error(message: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(format!("{REPORT_ERROR_PREFIX}{}", message.into()))
}

fn report_message(error: &SqliteDatabaseError) -> Option<String> {
    match error {
        SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(message))
            if message.starts_with(REPORT_ERROR_PREFIX) =>
        {
            Some(message.trim_start_matches(REPORT_ERROR_PREFIX).to_string())
        }
        _ => None,
    }
}

fn ensure_reporting_schema(connection: &Connection) -> rusqlite::Result<()> {
    ensure_dynamic_schema(connection)?;
    ensure_attachment_fields_schema(connection)?;

    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS mx_dashboard_config (
            id          INTEGER PRIMARY KEY NOT NULL CHECK(id = 1),
            revision    INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0),
            config_json TEXT NOT NULL DEFAULT '{"widgets":[],"show_user_performance":false}',
            updated_at  INTEGER NOT NULL DEFAULT 0
        );

        INSERT OR IGNORE INTO mx_dashboard_config (
            id, revision, config_json, updated_at
        ) VALUES (
            1, 0, '{"widgets":[],"show_user_performance":false}', 0
        );

        -- The previous presentation-feedback patch enabled user-performance
        -- by default. Convert only that untouched default row to the new blank
        -- dashboard. Never overwrite a dashboard the administrator edited.
        UPDATE mx_dashboard_config
        SET config_json = '{"widgets":[],"show_user_performance":false}'
        WHERE revision = 0
          AND config_json = '{"widgets":[],"show_user_performance":true}';
        "#,
    )
}

fn value_expression(alias: &str, field_type: &str) -> String {
    match field_type {
        "integer" | "auto_number" => {
            format!("CAST({alias}.value_integer AS TEXT)")
        }
        "decimal" => format!("CAST({alias}.value_real AS TEXT)"),
        "boolean" => {
            format!("CASE {alias}.value_boolean WHEN 1 THEN 'true' WHEN 0 THEN 'false' ELSE '' END")
        }
        _ => format!("COALESCE({alias}.value_text, '')"),
    }
}

fn lookup_field(connection: &Connection, uid: &str) -> rusqlite::Result<Option<FieldDefinition>> {
    Ok(load_fields_db(connection, false)?
        .into_iter()
        .find(|field| field.uid == uid && field.active))
}

fn optional_group_field(
    connection: &Connection,
    uid: Option<&str>,
) -> rusqlite::Result<Option<FieldDefinition>> {
    let Some(uid) = uid.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let field = lookup_field(connection, uid)?
        .ok_or_else(|| report_error("group_field_uid was not found or is archived"))?;

    if field.field_type == "attachments" {
        return Err(report_error(
            "group_field_uid must reference a non-attachment field",
        ));
    }

    Ok(Some(field))
}

fn report_grouping(
    connection: &Connection,
    query: &ReportQuery,
    bind: &mut Vec<SqlValue>,
) -> rusqlite::Result<(String, String, bool, bool)> {
    let time_bucket = query
        .time_bucket
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());

    if let Some(bucket) = time_bucket {
        if query
            .group_field_uid
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
        {
            return Err(report_error(
                "group_field_uid cannot be combined with time_bucket",
            ));
        }

        if !matches!(
            bucket.as_str(),
            "day" | "week" | "month" | "quarter" | "year"
        ) {
            return Err(report_error(
                "time_bucket must be day, week, month, quarter, or year",
            ));
        }

        let date_uid = query
            .date_field_uid
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| report_error("date_field_uid is required when time_bucket is used"))?;
        let date_field = lookup_field(connection, date_uid)?
            .ok_or_else(|| report_error("date_field_uid was not found or is archived"))?;
        if date_field.field_type != "date" {
            return Err(report_error("date_field_uid must reference a Date field"));
        }

        bind.push(SqlValue::Text(date_field.uid));
        let index = bind.len();
        let join = format!(
            " LEFT JOIN mx_record_values dv ON dv.record_uid = r.uid AND dv.field_uid = ?{index} "
        );
        let expression = match bucket.as_str() {
            "day" => "substr(COALESCE(dv.value_text, ''), 1, 10)".to_string(),
            "week" => "strftime('%Y-W%W', dv.value_text)".to_string(),
            "month" => "strftime('%Y-%m', dv.value_text)".to_string(),
            "quarter" => "printf('%04d-Q%d', CAST(strftime('%Y', dv.value_text) AS INTEGER), ((CAST(strftime('%m', dv.value_text) AS INTEGER) - 1) / 3) + 1)".to_string(),
            "year" => "strftime('%Y', dv.value_text)".to_string(),
            _ => unreachable!(),
        };

        return Ok((expression, join, true, true));
    }

    let group_field = optional_group_field(connection, query.group_field_uid.as_deref())?;
    match group_field.as_ref() {
        Some(field) => {
            bind.push(SqlValue::Text(field.uid.clone()));
            let index = bind.len();
            Ok((
                value_expression("gv", &field.field_type),
                format!(
                    " LEFT JOIN mx_record_values gv ON gv.record_uid = r.uid AND gv.field_uid = ?{index} "
                ),
                false,
                false,
            ))
        }
        None => Ok(("'All records'".to_string(), String::new(), false, false)),
    }
}

fn append_date_filter(
    connection: &Connection,
    query: &ReportQuery,
    bind: &mut Vec<SqlValue>,
    joins: &mut String,
    where_sql: &mut String,
    date_already_joined: bool,
) -> rusqlite::Result<()> {
    let Some(date_uid) = query
        .date_field_uid
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };

    if !date_already_joined {
        let date_field = lookup_field(connection, date_uid)?
            .ok_or_else(|| report_error("date_field_uid was not found or is archived"))?;

        if date_field.field_type != "date" {
            return Err(report_error("date_field_uid must reference a Date field"));
        }

        bind.push(SqlValue::Text(date_field.uid));
        let date_uid_index = bind.len();
        joins.push_str(&format!(
            " LEFT JOIN mx_record_values dv ON dv.record_uid = r.uid AND dv.field_uid = ?{date_uid_index} "
        ));
    }

    if query
        .time_bucket
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
    {
        where_sql.push_str(" AND date(dv.value_text) IS NOT NULL ");
    }

    if let Some(from) = query
        .date_from
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        bind.push(SqlValue::Text(from.to_string()));
        where_sql.push_str(&format!(
            " AND COALESCE(dv.value_text, '') >= ?{} ",
            bind.len()
        ));
    }

    if let Some(to) = query
        .date_to
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        bind.push(SqlValue::Text(to.to_string()));
        where_sql.push_str(&format!(
            " AND COALESCE(dv.value_text, '') <= ?{} ",
            bind.len()
        ));
    }

    Ok(())
}

fn attachment_presence_rows(
    connection: &Connection,
    query: &ReportQuery,
) -> rusqlite::Result<Vec<ActionRateRow>> {
    ensure_reporting_schema(connection)?;

    let attachment_field_uid = query
        .attachment_field_uid
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| report_error("attachment_field_uid is required"))?;

    let attachment_field = load_attachment_field_db(connection, attachment_field_uid, true)?
        .ok_or_else(|| report_error("attachment_field_uid was not found or is archived"))?;

    let mut bind: Vec<SqlValue> = Vec::new();
    let (group_expr, group_join, date_already_joined, chronological) =
        report_grouping(connection, query, &mut bind)?;

    bind.push(SqlValue::Text(attachment_field.uid));
    let attachment_index = bind.len();

    let mut joins = group_join;
    let mut where_sql = String::new();
    append_date_filter(
        connection,
        query,
        &mut bind,
        &mut joins,
        &mut where_sql,
        date_already_joined,
    )?;

    let order_sql = if chronological {
        "group_name COLLATE NOCASE ASC"
    } else {
        "total DESC, group_name COLLATE NOCASE ASC"
    };

    let sql = format!(
        r#"
        SELECT
            CASE
                WHEN TRIM({group_expr}) = '' THEN 'Unspecified'
                ELSE TRIM({group_expr})
            END AS group_name,
            COUNT(DISTINCT r.uid) AS total,
            COUNT(DISTINCT CASE
                WHEN EXISTS (
                    SELECT 1
                    FROM mx_attachments aa
                    WHERE aa.entry_uid = r.uid
                      AND aa.attachment_field_uid = ?{attachment_index}
                    LIMIT 1
                ) THEN r.uid
            END) AS actioned
        FROM mx_records r
        {joins}
        WHERE 1 = 1
        {where_sql}
        GROUP BY group_name
        ORDER BY {order_sql}
        "#
    );

    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(bind.iter()), |row| {
        let total = row.get::<_, i64>(1)?.max(0) as u64;
        let actioned = row.get::<_, i64>(2)?.max(0) as u64;
        Ok(ActionRateRow {
            group: row.get(0)?,
            total,
            actioned,
            pending: total.saturating_sub(actioned),
            rate: if total == 0 {
                0.0
            } else {
                actioned as f64 * 100.0 / total as f64
            },
        })
    })?;

    rows.collect()
}

fn action_rate_rows(
    connection: &Connection,
    query: &ReportQuery,
) -> rusqlite::Result<Vec<ActionRateRow>> {
    ensure_reporting_schema(connection)?;

    if query
        .attachment_field_uid
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
    {
        return attachment_presence_rows(connection, query);
    }

    let action_mode = query
        .action_mode
        .as_deref()
        .unwrap_or("equals")
        .trim()
        .to_ascii_lowercase();

    if !matches!(
        action_mode.as_str(),
        "all" | "equals" | "not_equals" | "nonempty" | "empty"
    ) {
        return Err(report_error(
            "action_mode must be 'all', 'equals', 'not_equals', 'nonempty', or 'empty'",
        ));
    }

    let mut bind: Vec<SqlValue> = Vec::new();
    let (group_expr, group_join, date_already_joined, chronological) =
        report_grouping(connection, query, &mut bind)?;

    let mut joins = group_join;

    let action_condition = if action_mode == "all" {
        "1 = 1".to_string()
    } else {
        let action_uid = query
            .action_field_uid
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| report_error("action_field_uid is required for this match mode"))?;
        let action_field = lookup_field(connection, action_uid)?
            .ok_or_else(|| report_error("action_field_uid was not found or is archived"))?;

        if action_field.field_type == "attachments" {
            return Err(report_error(
                "action_field_uid must reference a non-attachment field; use attachment_field_uid for File Attachment presence",
            ));
        }

        bind.push(SqlValue::Text(action_field.uid));
        let action_uid_index = bind.len();
        joins.push_str(&format!(
            " LEFT JOIN mx_record_values av ON av.record_uid = r.uid AND av.field_uid = ?{action_uid_index} "
        ));
        let action_expr = value_expression("av", &action_field.field_type);

        match action_mode.as_str() {
            "nonempty" => format!("TRIM({action_expr}) <> ''"),
            "empty" => format!("TRIM({action_expr}) = ''"),
            "equals" | "not_equals" => {
                let action_value = query
                    .action_value
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        report_error(format!(
                            "action_value is required when action_mode={action_mode}"
                        ))
                    })?
                    .to_string();
                bind.push(SqlValue::Text(action_value));
                let value_index = bind.len();
                let operator = if action_mode == "equals" { "=" } else { "<>" };
                format!("LOWER(TRIM({action_expr})) {operator} LOWER(TRIM(?{value_index}))")
            }
            _ => unreachable!(),
        }
    };

    let mut where_sql = String::new();
    append_date_filter(
        connection,
        query,
        &mut bind,
        &mut joins,
        &mut where_sql,
        date_already_joined,
    )?;

    let order_sql = if chronological {
        "group_name COLLATE NOCASE ASC"
    } else {
        "total DESC, group_name COLLATE NOCASE ASC"
    };

    let sql = format!(
        r#"
        SELECT
            CASE
                WHEN TRIM({group_expr}) = '' THEN 'Unspecified'
                ELSE TRIM({group_expr})
            END AS group_name,
            COUNT(DISTINCT r.uid) AS total,
            COUNT(DISTINCT CASE
                WHEN {action_condition}
                THEN r.uid
            END) AS actioned
        FROM mx_records r
        {joins}
        WHERE 1 = 1
        {where_sql}
        GROUP BY group_name
        ORDER BY {order_sql}
        "#
    );

    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(bind.iter()), |row| {
        let total = row.get::<_, i64>(1)?.max(0) as u64;
        let actioned = row.get::<_, i64>(2)?.max(0) as u64;

        Ok(ActionRateRow {
            group: row.get(0)?,
            total,
            actioned,
            pending: total.saturating_sub(actioned),
            rate: if total == 0 {
                0.0
            } else {
                actioned as f64 * 100.0 / total as f64
            },
        })
    })?;

    rows.collect()
}

fn parse_start_date(value: &str) -> rusqlite::Result<i64> {
    let date = chrono::NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map_err(|_| report_error("date_from must use YYYY-MM-DD"))?;
    Ok(date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| report_error("invalid date_from"))?
        .and_utc()
        .timestamp_millis())
}

fn parse_end_date(value: &str) -> rusqlite::Result<i64> {
    let date = chrono::NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map_err(|_| report_error("date_to must use YYYY-MM-DD"))?;
    Ok(date
        .and_hms_opt(23, 59, 59)
        .ok_or_else(|| report_error("invalid date_to"))?
        .and_utc()
        .timestamp_millis()
        + 999)
}

fn user_performance_rows(
    connection: &Connection,
    query: &UserPerformanceQuery,
) -> rusqlite::Result<Vec<UserPerformanceRow>> {
    ensure_reporting_schema(connection)?;

    let audit_exists = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='mx_audit_log')",
        [],
        |row| row.get::<_, i64>(0),
    )? != 0;

    if !audit_exists {
        return Ok(Vec::new());
    }

    let mut conditions = vec!["1 = 1".to_string()];
    let mut bind: Vec<SqlValue> = Vec::new();

    if let Some(from) = query
        .date_from
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        bind.push(SqlValue::Integer(parse_start_date(from)?));
        conditions.push(format!("created_at >= ?{}", bind.len()));
    }

    if let Some(to) = query
        .date_to
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        bind.push(SqlValue::Integer(parse_end_date(to)?));
        conditions.push(format!("created_at <= ?{}", bind.len()));
    }

    let sql = format!(
        r#"
        SELECT
            actor_uid,
            MAX(COALESCE(actor_name, '')),
            MAX(COALESCE(actor_email, '')),
            SUM(CASE WHEN success = 1 AND action = 'record.create' THEN 1 ELSE 0 END),
            SUM(CASE WHEN success = 1 AND action = 'record.update' THEN 1 ELSE 0 END),
            SUM(CASE WHEN success = 1 AND action = 'attachment.upload' THEN 1 ELSE 0 END),
            SUM(CASE WHEN success = 1 AND action = 'record.delete' THEN 1 ELSE 0 END),
            SUM(CASE WHEN success = 1 AND action = 'attachment.download' THEN 1 ELSE 0 END),
            SUM(CASE WHEN success = 1 THEN 1 ELSE 0 END),
            SUM(CASE WHEN success = 0 THEN 1 ELSE 0 END),
            COUNT(DISTINCT CASE WHEN target_type = 'record' THEN target_uid END),
            MAX(created_at)
        FROM mx_audit_log
        WHERE {}
        GROUP BY actor_uid
        ORDER BY SUM(CASE WHEN success = 1 THEN 1 ELSE 0 END) DESC,
                 actor_uid ASC
        "#,
        conditions.join(" AND ")
    );

    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(bind.iter()), |row| {
        Ok(UserPerformanceRow {
            actor_uid: row.get(0)?,
            actor_name: row.get(1)?,
            actor_email: row.get(2)?,
            records_created: row.get::<_, i64>(3)?.max(0) as u64,
            records_updated: row.get::<_, i64>(4)?.max(0) as u64,
            attachments_uploaded: row.get::<_, i64>(5)?.max(0) as u64,
            records_deleted: row.get::<_, i64>(6)?.max(0) as u64,
            attachment_downloads: row.get::<_, i64>(7)?.max(0) as u64,
            successful_actions: row.get::<_, i64>(8)?.max(0) as u64,
            failed_actions: row.get::<_, i64>(9)?.max(0) as u64,
            unique_records_touched: row.get::<_, i64>(10)?.max(0) as u64,
            last_activity: row.get::<_, i64>(11)?.max(0),
        })
    })?;

    rows.collect()
}

pub async fn get_dashboard_config(claims: Claims) -> Response {
    if !claims.can_read_records() {
        return api_json(StatusCode::FORBIDDEN, json!({"response":"access denied"}));
    }

    let result = tokio::task::spawn_blocking(
        move || -> Result<(i64, String, i64), SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_reporting_schema(connection)?;
                connection.query_row(
                    "SELECT revision, config_json, updated_at FROM mx_dashboard_config WHERE id = 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
            })
        },
    )
    .await;

    match result {
        Ok(Ok((revision, text, updated_at))) => api_json(
            StatusCode::OK,
            json!({
                "revision": revision,
                "config": serde_json::from_str::<Value>(&text)
                    .unwrap_or_else(|_| json!({"widgets":[]})),
                "updated_at": updated_at
            }),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to load dashboard configuration"}),
        ),
    }
}

pub async fn save_dashboard_config(
    claims: Claims,
    Json(request): Json<SaveDashboardConfigRequest>,
) -> Response {
    if !claims.can_manage_accounts() {
        return api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"administrator access is required"}),
        );
    }

    if !request.config.is_object() {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"config must be a JSON object"}),
        );
    }

    let config_text = match serde_json::to_string(&request.config) {
        Ok(value) => value,
        Err(_) => {
            return api_json(
                StatusCode::BAD_REQUEST,
                json!({"response":"invalid dashboard configuration"}),
            );
        }
    };

    let result = tokio::task::spawn_blocking(move || -> Result<i64, SqliteDatabaseError> {
        with_sql_connection(|connection| {
            ensure_reporting_schema(connection)?;
            connection.execute(
                r#"
                    UPDATE mx_dashboard_config
                    SET revision = revision + 1,
                        config_json = ?1,
                        updated_at = CAST(STRFTIME('%s','now') AS INTEGER) * 1000
                    WHERE id = 1
                    "#,
                params![config_text],
            )?;

            connection.query_row(
                "SELECT revision FROM mx_dashboard_config WHERE id = 1",
                [],
                |row| row.get(0),
            )
        })
    })
    .await;

    match result {
        Ok(Ok(revision)) => {
            publish_live_event(
                "dashboard.updated",
                Some(&claims.uid),
                json!({"revision": revision}),
            );
            api_json(
                StatusCode::OK,
                json!({"response":"dashboard configuration saved","revision":revision}),
            )
        }
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to save dashboard configuration"}),
        ),
    }
}

pub async fn get_action_rate_report(claims: Claims, Query(query): Query<ReportQuery>) -> Response {
    if !claims.can_read_records() {
        return api_json(StatusCode::FORBIDDEN, json!({"response":"access denied"}));
    }

    let result =
        tokio::task::spawn_blocking(move || -> Result<Vec<ActionRateRow>, SqliteDatabaseError> {
            with_sql_connection(|connection| action_rate_rows(connection, &query))
        })
        .await;

    match result {
        Ok(Ok(rows)) => api_json(StatusCode::OK, json!({"rows":rows})),
        Ok(Err(error)) if report_message(&error).is_some() => api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":report_message(&error).unwrap_or_default()}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to build action-rate report"}),
        ),
    }
}

pub async fn get_user_performance_report(
    claims: Claims,
    Query(query): Query<UserPerformanceQuery>,
) -> Response {
    if !claims.can_manage_accounts() {
        return api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"administrator access is required"}),
        );
    }

    let result = tokio::task::spawn_blocking(
        move || -> Result<Vec<UserPerformanceRow>, SqliteDatabaseError> {
            with_sql_connection(|connection| user_performance_rows(connection, &query))
        },
    )
    .await;

    match result {
        Ok(Ok(rows)) => api_json(StatusCode::OK, json!({"rows":rows})),
        Ok(Err(error)) if report_message(&error).is_some() => api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":report_message(&error).unwrap_or_default()}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to build user performance report"}),
        ),
    }
}

fn csv_cell(value: impl AsRef<str>) -> String {
    let value = value.as_ref();
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn csv_line(values: impl IntoIterator<Item = String>) -> String {
    values
        .into_iter()
        .map(csv_cell)
        .collect::<Vec<_>>()
        .join(",")
        + "\r\n"
}

fn detailed_records_csv(connection: &Connection) -> rusqlite::Result<String> {
    ensure_reporting_schema(connection)?;

    let fields = load_fields_db(connection, false)?
        .into_iter()
        .filter(|field| field.active)
        .collect::<Vec<_>>();
    let scalar_fields = fields
        .iter()
        .filter(|field| field.field_type != "attachments")
        .collect::<Vec<_>>();
    let attachment_fields = fields
        .iter()
        .filter(|field| field.field_type == "attachments")
        .collect::<Vec<_>>();

    let count: i64 =
        connection.query_row("SELECT COUNT(*) FROM mx_records", [], |row| row.get(0))?;

    if count.max(0) as usize > MAX_EXPORT_ROWS {
        return Err(report_error(format!(
            "Detailed export is limited to {MAX_EXPORT_ROWS} records per file"
        )));
    }

    let mut uid_statement = connection.prepare("SELECT uid FROM mx_records ORDER BY rowid ASC")?;
    let uids = uid_statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut values_by_record: HashMap<String, HashMap<String, String>> = HashMap::new();

    {
        let mut value_statement = connection.prepare(
            r#"
            SELECT
                rv.record_uid,
                rv.field_uid,
                f.field_type,
                rv.value_text,
                rv.value_integer,
                rv.value_real,
                rv.value_boolean
            FROM mx_record_values rv
            JOIN mx_fields f
              ON f.uid = rv.field_uid
             AND f.active = 1
             AND f.field_type <> 'attachments'
            JOIN mx_records r
              ON r.uid = rv.record_uid
            ORDER BY r.rowid ASC, f.position ASC, f.rowid ASC
            "#,
        )?;

        let mapped = value_statement.query_map([], |row| {
            let record_uid: String = row.get(0)?;
            let field_uid: String = row.get(1)?;
            let field_type: String = row.get(2)?;

            let value = match field_type.as_str() {
                "integer" | "auto_number" => row
                    .get::<_, Option<i64>>(4)?
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                "decimal" => row
                    .get::<_, Option<f64>>(5)?
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                "boolean" => row
                    .get::<_, Option<i64>>(6)?
                    .map(|value| if value != 0 { "true" } else { "false" }.to_string())
                    .unwrap_or_default(),
                _ => row.get::<_, Option<String>>(3)?.unwrap_or_default(),
            };

            Ok((record_uid, field_uid, value))
        })?;

        for mapped_value in mapped {
            let (record_uid, field_uid, value) = mapped_value?;
            values_by_record
                .entry(record_uid)
                .or_default()
                .insert(field_uid, value);
        }
    }

    let mut attachment_names_by_record: HashMap<String, HashMap<String, Vec<String>>> =
        HashMap::new();
    let mut attachment_totals_by_record: HashMap<String, (u64, u64)> = HashMap::new();

    {
        let mut attachment_statement = connection.prepare(
            r#"
            SELECT
                entry_uid,
                COALESCE(attachment_field_uid, ''),
                file_name,
                size
            FROM mx_attachments
            ORDER BY rowid ASC
            "#,
        )?;

        let mapped = attachment_statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?.max(0) as u64,
            ))
        })?;

        for attachment in mapped {
            let (record_uid, field_uid, file_name, size) = attachment?;
            attachment_names_by_record
                .entry(record_uid.clone())
                .or_default()
                .entry(field_uid)
                .or_default()
                .push(file_name);
            let totals = attachment_totals_by_record
                .entry(record_uid)
                .or_insert((0, 0));
            totals.0 = totals.0.saturating_add(1);
            totals.1 = totals.1.saturating_add(size);
        }
    }

    let mut output = String::new();
    let mut header = vec!["record_uid".to_string()];
    header.extend(scalar_fields.iter().map(|field| field.label.clone()));
    header.extend(
        attachment_fields
            .iter()
            .map(|field| format!("{} [Files]", field.label)),
    );
    header.extend(
        ["attachment_count", "attachment_bytes"]
            .into_iter()
            .map(str::to_string),
    );
    output.push_str(&csv_line(header));

    for uid in uids {
        let values = values_by_record.remove(&uid).unwrap_or_default();
        let attachment_values = attachment_names_by_record.remove(&uid).unwrap_or_default();
        let (attachment_count, attachment_bytes) =
            attachment_totals_by_record.remove(&uid).unwrap_or((0, 0));

        let mut row = vec![uid];
        row.extend(
            scalar_fields
                .iter()
                .map(|field| values.get(&field.uid).cloned().unwrap_or_default()),
        );
        row.extend(attachment_fields.iter().map(|field| {
            attachment_values
                .get(&field.uid)
                .map(|names| names.join(" | "))
                .unwrap_or_default()
        }));
        row.extend([attachment_count.to_string(), attachment_bytes.to_string()]);
        output.push_str(&csv_line(row));
    }

    Ok(output)
}

fn attachments_csv(connection: &Connection) -> rusqlite::Result<String> {
    ensure_reporting_schema(connection)?;

    let count: i64 =
        connection.query_row("SELECT COUNT(*) FROM mx_attachments", [], |row| row.get(0))?;

    if count.max(0) as usize > MAX_EXPORT_ROWS {
        return Err(report_error(format!(
            "Attachment export is limited to {MAX_EXPORT_ROWS} rows per file"
        )));
    }

    let mut output = csv_line(
        [
            "record_uid",
            "attachment_field_uid",
            "attachment_field",
            "file_name",
            "mime_type",
            "bytes",
            "object_key",
            "version_id",
            "created_at_ms",
        ]
        .into_iter()
        .map(str::to_string),
    );

    let mut statement = connection.prepare(
        r#"
        SELECT
            entry_uid,
            COALESCE(attachment_field_uid, ''),
            COALESCE(attachment_field_label, ''),
            file_name,
            mime_type,
            size,
            object_key,
            COALESCE(version_id, ''),
            created_at
        FROM mx_attachments
        ORDER BY rowid ASC
        "#,
    )?;

    let rows = statement.query_map([], |row| {
        Ok(vec![
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?.max(0).to_string(),
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, i64>(8)?.max(0).to_string(),
        ])
    })?;

    for row in rows {
        output.push_str(&csv_line(row?));
    }

    Ok(output)
}

fn user_performance_csv(
    connection: &Connection,
    query: &UserPerformanceQuery,
) -> rusqlite::Result<String> {
    let rows = user_performance_rows(connection, query)?;
    let mut output = csv_line(
        [
            "user_uid",
            "name",
            "email",
            "records_created",
            "records_updated",
            "attachments_uploaded",
            "records_deleted",
            "attachment_downloads",
            "successful_actions",
            "failed_actions",
            "unique_records_touched",
            "last_activity_ms",
        ]
        .into_iter()
        .map(str::to_string),
    );

    for row in rows {
        output.push_str(&csv_line(vec![
            row.actor_uid,
            row.actor_name,
            row.actor_email,
            row.records_created.to_string(),
            row.records_updated.to_string(),
            row.attachments_uploaded.to_string(),
            row.records_deleted.to_string(),
            row.attachment_downloads.to_string(),
            row.successful_actions.to_string(),
            row.failed_actions.to_string(),
            row.unique_records_touched.to_string(),
            row.last_activity.to_string(),
        ]));
    }

    Ok(output)
}

fn action_rate_csv(connection: &Connection, query: &ReportQuery) -> rusqlite::Result<String> {
    let rows = action_rate_rows(connection, query)?;
    let mut output = csv_line(
        [
            "group",
            "total_records",
            "matched_records",
            "not_matched",
            "match_rate_percent",
        ]
        .into_iter()
        .map(str::to_string),
    );

    for row in rows {
        output.push_str(&csv_line(vec![
            row.group,
            row.total.to_string(),
            row.actioned.to_string(),
            row.pending.to_string(),
            format!("{:.2}", row.rate),
        ]));
    }

    Ok(output)
}

fn csv_response(file_name: &str, csv: String) -> Response {
    let mut response = Response::new(Body::from(csv));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );

    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{file_name}\"")) {
        response.headers_mut().insert(CONTENT_DISPOSITION, value);
    }

    response
}

pub async fn export_report_csv(claims: Claims, Query(query): Query<ExportQuery>) -> Response {
    if !claims.can_read_records() {
        return api_json(StatusCode::FORBIDDEN, json!({"response":"access denied"}));
    }

    let kind = query.kind.trim().to_ascii_lowercase();
    if kind == "user_performance" && !claims.can_manage_accounts() {
        return api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"administrator access is required for user performance exports"}),
        );
    }

    let kind_for_task = kind.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<String, SqliteDatabaseError> {
        with_sql_connection(|connection| match kind_for_task.as_str() {
            "records" | "detailed_records" => detailed_records_csv(connection),
            "attachments" => attachments_csv(connection),
            "user_performance" => user_performance_csv(
                connection,
                &UserPerformanceQuery {
                    date_from: query.date_from.clone(),
                    date_to: query.date_to.clone(),
                },
            ),
            "action_rate" => action_rate_csv(
                connection,
                &ReportQuery {
                    group_field_uid: query.group_field_uid.clone(),
                    action_field_uid: query.action_field_uid.clone(),
                    action_mode: query.action_mode.clone(),
                    action_value: query.action_value.clone(),
                    attachment_field_uid: query.attachment_field_uid.clone(),
                    date_field_uid: query.date_field_uid.clone(),
                    date_from: query.date_from.clone(),
                    date_to: query.date_to.clone(),
                    time_bucket: query.time_bucket.clone(),
                },
            ),
            _ => Err(report_error(
                "kind must be records, attachments, user_performance, or action_rate",
            )),
        })
    })
    .await;

    match result {
        Ok(Ok(csv)) => csv_response(&format!("mx-{}.csv", kind.replace('_', "-")), csv),
        Ok(Err(error)) if report_message(&error).is_some() => api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":report_message(&error).unwrap_or_default()}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to create report export"}),
        ),
    }
}
