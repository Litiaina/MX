use axum::{
    Json,
    extract::Query,
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{ETAG, IF_NONE_MATCH},
    },
    response::{IntoResponse, Response},
};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    api::aris::{
        handler::ensure_aris_record_schema,
        schema::{ensure_dynamic_schema, schema_revision_db},
    },
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::Claims,
};

#[derive(Debug, Deserialize, Default)]
pub struct DashboardQuery {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub office: Option<String>,
    pub routed_to_div: Option<String>,
    pub bucket: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RevisionResponse {
    pub records_revision: u64,
    pub schema_revision: u64,
}

#[derive(Debug, Serialize)]
pub struct NamedCount {
    pub name: String,
    pub count: u64,
}

#[derive(Debug, Serialize)]
pub struct VolumePoint {
    pub period: String,
    pub count: u64,
}

#[derive(Debug, Serialize)]
pub struct DashboardScope {
    pub records: u64,
    pub total_records: u64,
    pub today: u64,
    pub month: u64,
    pub attachments: u64,
    pub with_attachments: u64,
    pub offices: u64,
    pub attachment_coverage: f64,
    pub highest_control: u64,
}

#[derive(Debug, Serialize)]
pub struct DashboardQuality {
    pub required_complete: u64,
    pub duplicate_controls: u64,
    pub remarks: u64,
    pub valid_dates: u64,
}

#[derive(Debug, Serialize)]
pub struct DashboardDimensions {
    pub offices: Vec<String>,
    pub routes: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct RecentRecord {
    pub uid: String,
    pub control_no: u64,
    pub date: String,
    pub office: String,
    pub requestor: String,
    pub subject: String,
    pub routed_to_div: String,
    pub remarks: String,
    pub attachment_count: u64,
}

#[derive(Debug, Serialize)]
pub struct DashboardSummary {
    pub revision: u64,
    pub bucket: String,
    pub scope: DashboardScope,
    pub top_offices: Vec<NamedCount>,
    pub top_routes: Vec<NamedCount>,
    pub top_requestors: Vec<NamedCount>,
    pub volume: Vec<VolumePoint>,
    pub monthly_volume: Vec<VolumePoint>,
    pub attachment_types: Vec<NamedCount>,
    pub quality: DashboardQuality,
    pub dimensions: DashboardDimensions,
    pub recent_records: Vec<RecentRecord>,
}

#[derive(Clone)]
struct FilterValues {
    date_from: String,
    date_to: String,
    office: String,
    route: String,
}

fn clean(value: Option<String>) -> String {
    value.unwrap_or_default().trim().to_string()
}

fn normalize_bucket(value: Option<String>) -> String {
    match value
        .as_deref()
        .unwrap_or("month")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "day" => "day".to_string(),
        _ => "month".to_string(),
    }
}

const FILTER_SQL: &str = r#"
    (?1 = '' OR e.date >= ?1)
    AND (?2 = '' OR e.date <= ?2)
    AND (?3 = '' OR e.office = ?3 COLLATE NOCASE)
    AND (?4 = '' OR e.routed_to_div = ?4 COLLATE NOCASE)
"#;

fn ensure_dashboard_indexes(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS idx_aris_records_office_date
            ON aris_records(office COLLATE NOCASE, date DESC, control_no DESC);

        CREATE INDEX IF NOT EXISTS idx_aris_records_route_date
            ON aris_records(routed_to_div COLLATE NOCASE, date DESC, control_no DESC);

        CREATE INDEX IF NOT EXISTS idx_aris_records_requestor_date
            ON aris_records(requestor COLLATE NOCASE, date DESC, control_no DESC);

        CREATE INDEX IF NOT EXISTS idx_aris_attachments_mime
            ON aris_attachments(mime_type COLLATE NOCASE);
        "#,
    )
}

fn current_revision(connection: &Connection) -> Result<u64, rusqlite::Error> {
    let revision: i64 = connection.query_row(
        r#"
        SELECT records_revision
        FROM aris_revision
        WHERE id = 1
        "#,
        [],
        |row| row.get(0),
    )?;

    Ok(revision.max(0) as u64)
}

fn all_dimensions(connection: &Connection, column: &str) -> Result<Vec<String>, rusqlite::Error> {
    let sql = format!(
        r#"
        SELECT MIN(TRIM({column})) AS value
        FROM aris_records
        WHERE TRIM({column}) <> ''
        GROUP BY LOWER(TRIM({column}))
        ORDER BY value COLLATE NOCASE ASC
        "#
    );

    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;

    rows.collect::<Result<Vec<_>, _>>()
}

fn named_counts(
    connection: &Connection,
    filters: &FilterValues,
    column: &str,
    limit: usize,
) -> Result<Vec<NamedCount>, rusqlite::Error> {
    let sql = format!(
        r#"
        SELECT
            MIN(TRIM({column})) AS name,
            COUNT(*) AS count
        FROM aris_records e
        WHERE {FILTER_SQL}
          AND TRIM({column}) <> ''
        GROUP BY LOWER(TRIM({column}))
        ORDER BY count DESC, name COLLATE NOCASE ASC
        LIMIT {limit}
        "#
    );

    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        params![
            &filters.date_from,
            &filters.date_to,
            &filters.office,
            &filters.route,
        ],
        |row| {
            let count: i64 = row.get(1)?;

            Ok(NamedCount {
                name: row.get(0)?,
                count: count.max(0) as u64,
            })
        },
    )?;

    rows.collect::<Result<Vec<_>, _>>()
}

fn volume_points(
    connection: &Connection,
    filters: &FilterValues,
    bucket: &str,
) -> Result<Vec<VolumePoint>, rusqlite::Error> {
    let period_expression = if bucket == "day" {
        "SUBSTR(e.date, 1, 10)"
    } else {
        "SUBSTR(e.date, 1, 7)"
    };

    let minimum_length = if bucket == "day" { 10 } else { 7 };

    let sql = format!(
        r#"
        SELECT
            {period_expression} AS period,
            COUNT(*) AS count
        FROM aris_records e
        WHERE {FILTER_SQL}
          AND LENGTH(e.date) >= {minimum_length}
        GROUP BY period
        ORDER BY period ASC
        "#
    );

    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        params![
            &filters.date_from,
            &filters.date_to,
            &filters.office,
            &filters.route,
        ],
        |row| {
            let count: i64 = row.get(1)?;

            Ok(VolumePoint {
                period: row.get(0)?,
                count: count.max(0) as u64,
            })
        },
    )?;

    rows.collect::<Result<Vec<_>, _>>()
}

fn monthly_volume(connection: &Connection) -> Result<Vec<VolumePoint>, rusqlite::Error> {
    let mut statement = connection.prepare(
        r#"
        SELECT
            SUBSTR(e.date, 1, 7) AS period,
            COUNT(*) AS count
        FROM aris_records e
        WHERE LENGTH(e.date) >= 7
          AND e.date >= DATE('now', 'localtime', 'start of month', '-11 months')
        GROUP BY period
        ORDER BY period ASC
        "#,
    )?;

    let rows = statement.query_map([], |row| {
        let count: i64 = row.get(1)?;

        Ok(VolumePoint {
            period: row.get(0)?,
            count: count.max(0) as u64,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>()
}

fn attachment_types(
    connection: &Connection,
    filters: &FilterValues,
) -> Result<Vec<NamedCount>, rusqlite::Error> {
    let sql = format!(
        r#"
        SELECT
            CASE
                WHEN LOWER(a.mime_type) LIKE 'image/%'
                    THEN 'Images'
                WHEN LOWER(a.mime_type) = 'application/pdf'
                    OR LOWER(a.file_name) LIKE '%.pdf'
                    THEN 'PDF'
                WHEN LOWER(a.file_name) LIKE '%.doc'
                    OR LOWER(a.file_name) LIKE '%.docx'
                    OR LOWER(a.file_name) LIKE '%.docm'
                    OR LOWER(a.file_name) LIKE '%.rtf'
                    OR LOWER(a.file_name) LIKE '%.odt'
                    THEN 'Word / Writer'
                WHEN LOWER(a.file_name) LIKE '%.xls'
                    OR LOWER(a.file_name) LIKE '%.xlsx'
                    OR LOWER(a.file_name) LIKE '%.xlsm'
                    OR LOWER(a.file_name) LIKE '%.ods'
                    OR LOWER(a.file_name) LIKE '%.csv'
                    THEN 'Spreadsheet'
                WHEN LOWER(a.file_name) LIKE '%.ppt'
                    OR LOWER(a.file_name) LIKE '%.pptx'
                    OR LOWER(a.file_name) LIKE '%.pptm'
                    OR LOWER(a.file_name) LIKE '%.odp'
                    THEN 'Presentation'
                WHEN LOWER(a.mime_type) LIKE 'audio/%'
                    THEN 'Audio'
                WHEN LOWER(a.mime_type) LIKE 'video/%'
                    THEN 'Video'
                WHEN LOWER(a.mime_type) LIKE 'text/%'
                    OR LOWER(a.file_name) LIKE '%.txt'
                    OR LOWER(a.file_name) LIKE '%.md'
                    THEN 'Text'
                ELSE 'Other'
            END AS name,
            COUNT(*) AS count
        FROM aris_records e
        INNER JOIN aris_attachments a
            ON a.entry_uid = e.uid
        WHERE {FILTER_SQL}
        GROUP BY name
        ORDER BY count DESC, name ASC
        "#
    );

    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        params![
            &filters.date_from,
            &filters.date_to,
            &filters.office,
            &filters.route,
        ],
        |row| {
            let count: i64 = row.get(1)?;

            Ok(NamedCount {
                name: row.get(0)?,
                count: count.max(0) as u64,
            })
        },
    )?;

    rows.collect::<Result<Vec<_>, _>>()
}

fn recent_records(
    connection: &Connection,
    filters: &FilterValues,
) -> Result<Vec<RecentRecord>, rusqlite::Error> {
    let sql = format!(
        r#"
        SELECT
            e.uid,
            e.control_no,
            e.date,
            e.office,
            e.requestor,
            e.subject,
            e.routed_to_div,
            e.remarks,
            COUNT(a.uid) AS attachment_count
        FROM aris_records e
        LEFT JOIN aris_attachments a
            ON a.entry_uid = e.uid
        WHERE {FILTER_SQL}
        GROUP BY
            e.uid,
            e.control_no,
            e.date,
            e.office,
            e.requestor,
            e.subject,
            e.routed_to_div,
            e.remarks
        ORDER BY
            e.date DESC,
            e.control_no DESC,
            e.uid DESC
        LIMIT 12
        "#
    );

    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        params![
            &filters.date_from,
            &filters.date_to,
            &filters.office,
            &filters.route,
        ],
        |row| {
            let control_no: i64 = row.get(1)?;
            let attachment_count: i64 = row.get(8)?;

            Ok(RecentRecord {
                uid: row.get(0)?,
                control_no: control_no.max(0) as u64,
                date: row.get(2)?,
                office: row.get(3)?,
                requestor: row.get(4)?,
                subject: row.get(5)?,
                routed_to_div: row.get(6)?,
                remarks: row.get(7)?,
                attachment_count: attachment_count.max(0) as u64,
            })
        },
    )?;

    rows.collect::<Result<Vec<_>, _>>()
}

fn build_summary(
    connection: &Connection,
    query: DashboardQuery,
) -> Result<DashboardSummary, rusqlite::Error> {
    ensure_aris_record_schema(connection)?;
    ensure_dashboard_indexes(connection)?;

    let filters = FilterValues {
        date_from: clean(query.date_from),
        date_to: clean(query.date_to),
        office: clean(query.office),
        route: clean(query.routed_to_div),
    };

    let bucket = normalize_bucket(query.bucket);
    let revision = current_revision(connection)?;

    let metrics_sql = format!(
        r#"
        SELECT
            COUNT(*) AS records,
            COUNT(
                DISTINCT CASE
                    WHEN TRIM(e.office) <> ''
                    THEN LOWER(TRIM(e.office))
                END
            ) AS offices,
            COALESCE(
                SUM(
                    CASE
                        WHEN e.date = DATE('now', 'localtime')
                        THEN 1 ELSE 0
                    END
                ),
                0
            ) AS today_count,
            COALESCE(
                SUM(
                    CASE
                        WHEN SUBSTR(e.date, 1, 7) = STRFTIME('%Y-%m', 'now', 'localtime')
                        THEN 1 ELSE 0
                    END
                ),
                0
            ) AS month_count,
            COALESCE(
                SUM(
                    CASE
                        WHEN TRIM(e.date) <> ''
                         AND TRIM(e.office) <> ''
                         AND TRIM(e.requestor) <> ''
                         AND TRIM(e.subject) <> ''
                         AND TRIM(e.routed_to_div) <> ''
                        THEN 1 ELSE 0
                    END
                ),
                0
            ) AS required_complete,
            COALESCE(
                SUM(
                    CASE
                        WHEN DATE(e.date) IS NOT NULL
                        THEN 1 ELSE 0
                    END
                ),
                0
            ) AS valid_dates,
            COALESCE(
                SUM(
                    CASE
                        WHEN TRIM(e.remarks) <> ''
                        THEN 1 ELSE 0
                    END
                ),
                0
            ) AS remarks
        FROM aris_records e
        WHERE {FILTER_SQL}
        "#
    );

    let (
        records_i64,
        offices_i64,
        today_i64,
        month_i64,
        required_i64,
        valid_dates_i64,
        remarks_i64,
    ): (i64, i64, i64, i64, i64, i64, i64) = connection.query_row(
        &metrics_sql,
        params![
            &filters.date_from,
            &filters.date_to,
            &filters.office,
            &filters.route,
        ],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        },
    )?;

    let total_records_i64: i64 =
        connection.query_row("SELECT COUNT(*) FROM aris_records", [], |row| row.get(0))?;

    let attachment_sql = format!(
        r#"
        SELECT
            COUNT(a.uid) AS attachments,
            COUNT(DISTINCT a.entry_uid) AS with_attachments
        FROM aris_records e
        LEFT JOIN aris_attachments a
            ON a.entry_uid = e.uid
        WHERE {FILTER_SQL}
        "#
    );

    let (attachments_i64, with_attachments_i64): (i64, i64) = connection.query_row(
        &attachment_sql,
        params![
            &filters.date_from,
            &filters.date_to,
            &filters.office,
            &filters.route,
        ],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    let current_year: i32 = connection.query_row(
        r#"
        SELECT CAST(
            STRFTIME('%Y', 'now', 'localtime')
            AS INTEGER
        )
        "#,
        [],
        |row| row.get(0),
    )?;

    let highest_i64: i64 = connection
        .query_row(
            r#"
            SELECT last_value
            FROM aris_control_sequence
            WHERE year = ?1
            "#,
            params![current_year],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let duplicate_sql = format!(
        r#"
        SELECT COALESCE(SUM(duplicate_count - 1), 0)
        FROM (
            SELECT COUNT(*) AS duplicate_count
            FROM aris_records e
            WHERE {FILTER_SQL}
            GROUP BY
                e.control_year,
                e.control_no
            HAVING COUNT(*) > 1
        )
        "#
    );

    let duplicates_i64: i64 = connection.query_row(
        &duplicate_sql,
        params![
            &filters.date_from,
            &filters.date_to,
            &filters.office,
            &filters.route,
        ],
        |row| row.get(0),
    )?;

    let records = records_i64.max(0) as u64;
    let attachments = attachments_i64.max(0) as u64;
    let with_attachments = with_attachments_i64.max(0) as u64;

    let attachment_coverage = if records == 0 {
        0.0
    } else {
        ((with_attachments as f64 / records as f64) * 1000.0).round() / 10.0
    };

    let volume = volume_points(connection, &filters, &bucket)?;

    Ok(DashboardSummary {
        revision,
        bucket,
        scope: DashboardScope {
            records,
            total_records: total_records_i64.max(0) as u64,
            today: today_i64.max(0) as u64,
            month: month_i64.max(0) as u64,
            attachments,
            with_attachments,
            offices: offices_i64.max(0) as u64,
            attachment_coverage,
            highest_control: highest_i64.max(0) as u64,
        },
        top_offices: named_counts(connection, &filters, "e.office", 10)?,
        top_routes: named_counts(connection, &filters, "e.routed_to_div", 8)?,
        top_requestors: named_counts(connection, &filters, "e.requestor", 8)?,
        volume,
        monthly_volume: monthly_volume(connection)?,
        attachment_types: attachment_types(connection, &filters)?,
        quality: DashboardQuality {
            required_complete: required_i64.max(0) as u64,
            duplicate_controls: duplicates_i64.max(0) as u64,
            remarks: remarks_i64.max(0) as u64,
            valid_dates: valid_dates_i64.max(0) as u64,
        },
        dimensions: DashboardDimensions {
            offices: all_dimensions(connection, "office")?,
            routes: all_dimensions(connection, "routed_to_div")?,
        },
        recent_records: recent_records(connection, &filters)?,
    })
}

fn access_denied() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "response":
                "This account does not have ARIS record access."
        })),
    )
        .into_response()
}

fn internal_error(function_name: &str, error: impl std::fmt::Display) -> Response {
    crate::report_error!(format!("{error}"), "dashboard", function_name);

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "response":
                "ARIS could not calculate dashboard information."
        })),
    )
        .into_response()
}

pub async fn get_records_revision(claims: Claims) -> Response {
    if !claims.can_read_records() {
        return access_denied();
    }

    let result = tokio::task::spawn_blocking(move || -> Result<(u64, u64), SqliteDatabaseError> {
        with_sql_connection(|connection| {
            ensure_aris_record_schema(connection)?;
            ensure_dynamic_schema(connection)?;

            Ok((
                current_revision(connection)?,
                schema_revision_db(connection)?,
            ))
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

pub async fn get_dashboard_summary(
    claims: Claims,
    Query(query): Query<DashboardQuery>,
    headers: HeaderMap,
) -> Response {
    if !claims.can_read_records() {
        return access_denied();
    }

    let result =
        tokio::task::spawn_blocking(move || -> Result<DashboardSummary, SqliteDatabaseError> {
            with_sql_connection(|connection| build_summary(connection, query))
        })
        .await;

    let summary = match result {
        Ok(Ok(summary)) => summary,

        Ok(Err(error)) => {
            return internal_error("get_dashboard_summary()", error);
        }

        Err(error) => {
            return internal_error("get_dashboard_summary()", error);
        }
    };

    let etag = format!("\"aris-dashboard-{}\"", summary.revision);

    let not_modified = headers
        .get(IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(',').any(|candidate| candidate.trim() == etag))
        .unwrap_or(false);

    if not_modified {
        let mut response = StatusCode::NOT_MODIFIED.into_response();

        if let Ok(value) = HeaderValue::from_str(&etag) {
            response.headers_mut().insert(ETAG, value);
        }

        return response;
    }

    let mut response = (StatusCode::OK, Json(summary)).into_response();

    if let Ok(value) = HeaderValue::from_str(&etag) {
        response.headers_mut().insert(ETAG, value);
    }

    response
}
