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
        schema::{FieldDefinition, load_fields_db, schema_revision_db},
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

#[derive(Debug, Clone, Serialize)]
pub struct NamedCount {
    pub name: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
pub struct DashboardFieldRef {
    pub uid: String,
    pub key: String,
    pub label: String,
    pub field_type: String,
}

#[derive(Debug, Serialize)]
pub struct DashboardMeta {
    pub primary_dimension: Option<DashboardFieldRef>,
    pub secondary_dimension: Option<DashboardFieldRef>,
    pub tertiary_dimension: Option<DashboardFieldRef>,
    pub date_field: Option<DashboardFieldRef>,
    pub auto_number_field: Option<DashboardFieldRef>,
    pub narrative_field: Option<DashboardFieldRef>,
    pub required_fields: Vec<DashboardFieldRef>,
}

#[derive(Debug, Serialize)]
pub struct RecentRecord {
    pub uid: String,
    pub control_value: String,
    pub date_value: String,
    pub primary_value: String,
    pub summary_title: String,
    pub summary_subtitle: String,
    pub secondary_value: String,
    pub attachment_count: u64,
}

#[derive(Debug, Serialize)]
pub struct DashboardSummary {
    pub revision: u64,
    pub schema_revision: u64,
    pub bucket: String,
    pub scope: DashboardScope,
    pub meta: DashboardMeta,
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
    primary_value: String,
    secondary_value: String,
}

#[derive(Clone)]
struct DashboardPlan {
    date_field: Option<FieldDefinition>,
    auto_number_field: Option<FieldDefinition>,
    primary_dimension: Option<FieldDefinition>,
    secondary_dimension: Option<FieldDefinition>,
    tertiary_dimension: Option<FieldDefinition>,
    narrative_field: Option<FieldDefinition>,
    required_fields: Vec<FieldDefinition>,
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

fn to_field_ref(field: &FieldDefinition) -> DashboardFieldRef {
    DashboardFieldRef {
        uid: field.uid.clone(),
        key: field.key.clone(),
        label: field.label.clone(),
        field_type: field.field_type.clone(),
    }
}

fn choose_dashboard_plan(fields: &[FieldDefinition]) -> DashboardPlan {
    let active: Vec<FieldDefinition> = fields
        .iter()
        .filter(|field| field.active)
        .cloned()
        .collect();

    let date_field = active
        .iter()
        .find(|field| field.field_type == "date")
        .cloned();
    let auto_number_field = active
        .iter()
        .find(|field| field.field_type == "auto_number")
        .cloned();
    let narrative_field = active
        .iter()
        .find(|field| field.field_type == "long_text")
        .cloned();

    let mut dimension_candidates: Vec<FieldDefinition> = active
        .iter()
        .filter(|field| {
            matches!(field.field_type.as_str(), "text" | "select" | "boolean")
                && !field.unique_value
        })
        .cloned()
        .collect();

    if dimension_candidates.is_empty() {
        dimension_candidates = active
            .iter()
            .filter(|field| {
                !matches!(
                    field.field_type.as_str(),
                    "date" | "auto_number" | "long_text"
                )
            })
            .cloned()
            .collect();
    }

    let primary_dimension = dimension_candidates.get(0).cloned();
    let secondary_dimension = dimension_candidates.get(1).cloned();
    let tertiary_dimension = dimension_candidates.get(2).cloned();
    let required_fields = active
        .iter()
        .filter(|field| field.required)
        .cloned()
        .collect();

    DashboardPlan {
        date_field,
        auto_number_field,
        primary_dimension,
        secondary_dimension,
        tertiary_dimension,
        narrative_field,
        required_fields,
    }
}

fn display_expr(alias: &str) -> String {
    format!(
        "COALESCE(NULLIF(TRIM({a}.value_text), ''), CASE WHEN {a}.value_integer IS NOT NULL THEN CAST({a}.value_integer AS TEXT) END, CASE WHEN {a}.value_real IS NOT NULL THEN CAST({a}.value_real AS TEXT) END, CASE WHEN {a}.value_boolean = 1 THEN 'Yes' WHEN {a}.value_boolean = 0 THEN 'No' END, '')",
        a = alias
    )
}

fn has_value_expr(value_alias: &str, field_alias: &str) -> String {
    format!(
        "(( {f}.field_type IN ('text', 'long_text', 'select', 'date') AND {v}.value_text IS NOT NULL AND TRIM({v}.value_text) <> '' ) OR ( {f}.field_type IN ('integer', 'auto_number') AND {v}.value_integer IS NOT NULL ) OR ( {f}.field_type = 'decimal' AND {v}.value_real IS NOT NULL ) OR ( {f}.field_type = 'boolean' AND {v}.value_boolean IS NOT NULL ))",
        v = value_alias,
        f = field_alias
    )
}

fn build_filter_sql() -> String {
    let primary_value = display_expr("pf");
    let secondary_value = display_expr("sf");
    format!(
        r#"
        (?5 = '' OR ?1 = '' OR EXISTS (
            SELECT 1
            FROM aris_record_values df
            WHERE df.record_uid = r.uid
              AND df.field_uid = ?5
              AND TRIM(COALESCE(df.value_text, '')) >= ?1
        ))
        AND (?5 = '' OR ?2 = '' OR EXISTS (
            SELECT 1
            FROM aris_record_values df
            WHERE df.record_uid = r.uid
              AND df.field_uid = ?5
              AND TRIM(COALESCE(df.value_text, '')) <= ?2
        ))
        AND (?6 = '' OR ?3 = '' OR EXISTS (
            SELECT 1
            FROM aris_record_values pf
            WHERE pf.record_uid = r.uid
              AND pf.field_uid = ?6
              AND LOWER(TRIM({primary_value})) = LOWER(TRIM(?3))
        ))
        AND (?7 = '' OR ?4 = '' OR EXISTS (
            SELECT 1
            FROM aris_record_values sf
            WHERE sf.record_uid = r.uid
              AND sf.field_uid = ?7
              AND LOWER(TRIM({secondary_value})) = LOWER(TRIM(?4))
        ))
        "#
    )
}

fn filter_params(filters: &FilterValues, plan: &DashboardPlan) -> [String; 7] {
    [
        filters.date_from.clone(),
        filters.date_to.clone(),
        filters.primary_value.clone(),
        filters.secondary_value.clone(),
        plan.date_field
            .as_ref()
            .map(|field| field.uid.clone())
            .unwrap_or_default(),
        plan.primary_dimension
            .as_ref()
            .map(|field| field.uid.clone())
            .unwrap_or_default(),
        plan.secondary_dimension
            .as_ref()
            .map(|field| field.uid.clone())
            .unwrap_or_default(),
    ]
}

fn filter_params_with_field(
    filters: &FilterValues,
    plan: &DashboardPlan,
    field_uid: &str,
) -> [String; 8] {
    let base = filter_params(filters, plan);
    [
        base[0].clone(),
        base[1].clone(),
        base[2].clone(),
        base[3].clone(),
        base[4].clone(),
        base[5].clone(),
        base[6].clone(),
        field_uid.to_string(),
    ]
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

fn all_dimensions(
    connection: &Connection,
    filters: &FilterValues,
    plan: &DashboardPlan,
    field: Option<&FieldDefinition>,
) -> Result<Vec<String>, rusqlite::Error> {
    let Some(field) = field else {
        return Ok(Vec::new());
    };

    let value_expr = display_expr("rv");
    let filter_sql = build_filter_sql();
    let sql = format!(
        r#"
        SELECT MIN(value_name) AS value_name
        FROM (
            SELECT {value_expr} AS value_name
            FROM aris_records r
            JOIN aris_record_values rv
              ON rv.record_uid = r.uid
             AND rv.field_uid = ?8
            WHERE {filter_sql}
              AND TRIM({value_expr}) <> ''
        ) dimension_values
        GROUP BY LOWER(TRIM(value_name))
        ORDER BY value_name COLLATE NOCASE ASC
        "#
    );

    let mut statement = connection.prepare(&sql)?;
    let params = filter_params_with_field(filters, plan, &field.uid);

    let rows = statement.query_map(rusqlite::params_from_iter(params), |row| {
        row.get::<_, String>(0)
    })?;
    rows.collect::<Result<Vec<_>, _>>()
}

fn named_counts(
    connection: &Connection,
    filters: &FilterValues,
    plan: &DashboardPlan,
    field: Option<&FieldDefinition>,
    limit: usize,
) -> Result<Vec<NamedCount>, rusqlite::Error> {
    let Some(field) = field else {
        return Ok(Vec::new());
    };

    let value_expr = display_expr("rv");
    let filter_sql = build_filter_sql();
    let sql = format!(
        r#"
        SELECT MIN(value_name) AS name, COUNT(*) AS count
        FROM (
            SELECT {value_expr} AS value_name
            FROM aris_records r
            JOIN aris_record_values rv
              ON rv.record_uid = r.uid
             AND rv.field_uid = ?8
            WHERE {filter_sql}
              AND TRIM({value_expr}) <> ''
        ) dimension_values
        GROUP BY LOWER(TRIM(value_name))
        ORDER BY count DESC, name COLLATE NOCASE ASC
        LIMIT {limit}
        "#
    );

    let mut statement = connection.prepare(&sql)?;
    let params = filter_params_with_field(filters, plan, &field.uid);

    let rows = statement.query_map(rusqlite::params_from_iter(params), |row| {
        let count: i64 = row.get(1)?;
        Ok(NamedCount {
            name: row.get(0)?,
            count: count.max(0) as u64,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>()
}

fn volume_points(
    connection: &Connection,
    filters: &FilterValues,
    plan: &DashboardPlan,
    bucket: &str,
) -> Result<Vec<VolumePoint>, rusqlite::Error> {
    let Some(date_field) = plan.date_field.as_ref() else {
        return Ok(Vec::new());
    };

    let period_expression = if bucket == "day" {
        "SUBSTR(rv.value_text, 1, 10)"
    } else {
        "SUBSTR(rv.value_text, 1, 7)"
    };

    let minimum_length = if bucket == "day" { 10 } else { 7 };
    let filter_sql = build_filter_sql();
    let sql = format!(
        r#"
        SELECT {period_expression} AS period, COUNT(*) AS count
        FROM aris_records r
        JOIN aris_record_values rv
          ON rv.record_uid = r.uid
         AND rv.field_uid = ?8
        WHERE {filter_sql}
          AND LENGTH(TRIM(COALESCE(rv.value_text, ''))) >= {minimum_length}
        GROUP BY period
        ORDER BY period ASC
        "#
    );

    let mut statement = connection.prepare(&sql)?;
    let params = filter_params_with_field(filters, plan, &date_field.uid);

    let rows = statement.query_map(rusqlite::params_from_iter(params), |row| {
        let count: i64 = row.get(1)?;
        Ok(VolumePoint {
            period: row.get(0)?,
            count: count.max(0) as u64,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>()
}

fn monthly_volume(
    connection: &Connection,
    plan: &DashboardPlan,
) -> Result<Vec<VolumePoint>, rusqlite::Error> {
    let Some(date_field) = plan.date_field.as_ref() else {
        return Ok(Vec::new());
    };

    let mut statement = connection.prepare(
        r#"
        SELECT SUBSTR(rv.value_text, 1, 7) AS period, COUNT(*) AS count
        FROM aris_record_values rv
        WHERE rv.field_uid = ?1
          AND LENGTH(TRIM(COALESCE(rv.value_text, ''))) >= 7
          AND rv.value_text >= DATE('now', 'localtime', 'start of month', '-11 months')
        GROUP BY period
        ORDER BY period ASC
        "#,
    )?;

    let rows = statement.query_map(params![&date_field.uid], |row| {
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
    plan: &DashboardPlan,
) -> Result<Vec<NamedCount>, rusqlite::Error> {
    let filter_sql = build_filter_sql();
    let sql = format!(
        r#"
        SELECT
            CASE
                WHEN LOWER(a.mime_type) LIKE 'image/%' THEN 'Images'
                WHEN LOWER(a.mime_type) = 'application/pdf' OR LOWER(a.file_name) LIKE '%.pdf' THEN 'PDF'
                WHEN LOWER(a.file_name) LIKE '%.doc' OR LOWER(a.file_name) LIKE '%.docx' OR LOWER(a.file_name) LIKE '%.docm' OR LOWER(a.file_name) LIKE '%.rtf' OR LOWER(a.file_name) LIKE '%.odt' THEN 'Word / Writer'
                WHEN LOWER(a.file_name) LIKE '%.xls' OR LOWER(a.file_name) LIKE '%.xlsx' OR LOWER(a.file_name) LIKE '%.xlsm' OR LOWER(a.file_name) LIKE '%.ods' OR LOWER(a.file_name) LIKE '%.csv' THEN 'Spreadsheet'
                WHEN LOWER(a.file_name) LIKE '%.ppt' OR LOWER(a.file_name) LIKE '%.pptx' OR LOWER(a.file_name) LIKE '%.pptm' OR LOWER(a.file_name) LIKE '%.odp' THEN 'Presentation'
                WHEN LOWER(a.mime_type) LIKE 'audio/%' THEN 'Audio'
                WHEN LOWER(a.mime_type) LIKE 'video/%' THEN 'Video'
                WHEN LOWER(a.mime_type) LIKE 'text/%' OR LOWER(a.file_name) LIKE '%.txt' OR LOWER(a.file_name) LIKE '%.md' THEN 'Text'
                ELSE 'Other'
            END AS name,
            COUNT(*) AS count
        FROM aris_records r
        INNER JOIN aris_attachments a
            ON a.entry_uid = r.uid
        WHERE {filter_sql}
        GROUP BY name
        ORDER BY count DESC, name ASC
        "#
    );

    let mut statement = connection.prepare(&sql)?;
    let params = filter_params(filters, plan);
    let rows = statement.query_map(rusqlite::params_from_iter(params), |row| {
        let count: i64 = row.get(1)?;
        Ok(NamedCount {
            name: row.get(0)?,
            count: count.max(0) as u64,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>()
}

fn recent_records(
    connection: &Connection,
    filters: &FilterValues,
    plan: &DashboardPlan,
) -> Result<Vec<RecentRecord>, rusqlite::Error> {
    let control_uid = plan
        .auto_number_field
        .as_ref()
        .map(|field| field.uid.as_str())
        .unwrap_or("");
    let date_uid = plan
        .date_field
        .as_ref()
        .map(|field| field.uid.as_str())
        .unwrap_or("");
    let primary_uid = plan
        .primary_dimension
        .as_ref()
        .map(|field| field.uid.as_str())
        .unwrap_or("");
    let secondary_uid = plan
        .secondary_dimension
        .as_ref()
        .map(|field| field.uid.as_str())
        .unwrap_or("");
    let tertiary_uid = plan
        .tertiary_dimension
        .as_ref()
        .map(|field| field.uid.as_str())
        .unwrap_or("");
    let narrative_uid = plan
        .narrative_field
        .as_ref()
        .map(|field| field.uid.as_str())
        .unwrap_or("");

    let control_expr = display_expr("cv");
    let date_expr = display_expr("dv");
    let primary_expr = display_expr("pv");
    let secondary_expr = display_expr("sv");
    let tertiary_expr = display_expr("tv");
    let narrative_expr = display_expr("nv");
    let filter_sql = build_filter_sql();

    let sql = format!(
        r#"
        SELECT
            r.uid,
            COALESCE((SELECT {control_expr} FROM aris_record_values cv WHERE cv.record_uid = r.uid AND cv.field_uid = ?8 LIMIT 1), '') AS control_value,
            COALESCE((SELECT {date_expr} FROM aris_record_values dv WHERE dv.record_uid = r.uid AND dv.field_uid = ?9 LIMIT 1), '') AS date_value,
            COALESCE((SELECT {primary_expr} FROM aris_record_values pv WHERE pv.record_uid = r.uid AND pv.field_uid = ?10 LIMIT 1), '') AS primary_value,
            COALESCE((SELECT {narrative_expr} FROM aris_record_values nv WHERE nv.record_uid = r.uid AND nv.field_uid = ?13 LIMIT 1), '') AS summary_title,
            COALESCE((SELECT {tertiary_expr} FROM aris_record_values tv WHERE tv.record_uid = r.uid AND tv.field_uid = ?12 LIMIT 1), '') AS summary_subtitle,
            COALESCE((SELECT {secondary_expr} FROM aris_record_values sv WHERE sv.record_uid = r.uid AND sv.field_uid = ?11 LIMIT 1), '') AS secondary_value,
            COUNT(a.uid) AS attachment_count
        FROM aris_records r
        LEFT JOIN aris_attachments a ON a.entry_uid = r.uid
        WHERE {filter_sql}
        GROUP BY r.uid
        ORDER BY date_value DESC, control_value DESC, r.uid DESC
        LIMIT 12
        "#
    );

    let mut statement = connection.prepare(&sql)?;
    let params = rusqlite::params![
        &filters.date_from,
        &filters.date_to,
        &filters.primary_value,
        &filters.secondary_value,
        &date_uid,
        &primary_uid,
        &secondary_uid,
        &control_uid,
        &date_uid,
        &primary_uid,
        &secondary_uid,
        &tertiary_uid,
        &narrative_uid,
    ];

    let rows = statement.query_map(params, |row| {
        let attachment_count: i64 = row.get(7)?;
        Ok(RecentRecord {
            uid: row.get(0)?,
            control_value: row.get(1)?,
            date_value: row.get(2)?,
            primary_value: row.get(3)?,
            summary_title: row.get(4)?,
            summary_subtitle: row.get(5)?,
            secondary_value: row.get(6)?,
            attachment_count: attachment_count.max(0) as u64,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>()
}

fn build_summary(
    connection: &Connection,
    query: DashboardQuery,
) -> Result<DashboardSummary, rusqlite::Error> {
    ensure_aris_record_schema(connection)?;
    let fields = load_fields_db(connection, false)?;
    let plan = choose_dashboard_plan(&fields);

    let filters = FilterValues {
        date_from: clean(query.date_from),
        date_to: clean(query.date_to),
        primary_value: clean(query.office),
        secondary_value: clean(query.routed_to_div),
    };

    let bucket = normalize_bucket(query.bucket);
    let revision = current_revision(connection)?;
    let schema_revision = schema_revision_db(connection)?;
    let filter_sql = build_filter_sql();

    let primary_uid = plan
        .primary_dimension
        .as_ref()
        .map(|field| field.uid.as_str())
        .unwrap_or("");
    let secondary_uid = plan
        .secondary_dimension
        .as_ref()
        .map(|field| field.uid.as_str())
        .unwrap_or("");
    let date_uid = plan
        .date_field
        .as_ref()
        .map(|field| field.uid.as_str())
        .unwrap_or("");
    let auto_uid = plan
        .auto_number_field
        .as_ref()
        .map(|field| field.uid.as_str())
        .unwrap_or("");
    let narrative_uid = plan
        .narrative_field
        .as_ref()
        .map(|field| field.uid.as_str())
        .unwrap_or("");

    let required_sql = format!(
        r#"
        SELECT COUNT(*)
        FROM aris_records r
        WHERE {filter_sql}
          AND NOT EXISTS (
                SELECT 1
                FROM aris_fields f
                WHERE f.active = 1
                  AND f.required = 1
                  AND NOT EXISTS (
                        SELECT 1
                        FROM aris_record_values rv
                        WHERE rv.record_uid = r.uid
                          AND rv.field_uid = f.uid
                          AND {has_value}
                  )
          )
        "#,
        has_value = has_value_expr("rv", "f")
    );

    let required_params = filter_params(&filters, &plan);
    let required_complete_i64: i64 = connection.query_row(
        &required_sql,
        rusqlite::params_from_iter(required_params),
        |row| row.get(0),
    )?;

    let records_i64: i64 = connection.query_row(
        &format!("SELECT COUNT(*) FROM aris_records r WHERE {filter_sql}"),
        rusqlite::params_from_iter(filter_params(&filters, &plan)),
        |row| row.get(0),
    )?;

    let total_records_i64: i64 =
        connection.query_row("SELECT COUNT(*) FROM aris_records", [], |row| row.get(0))?;

    let distinct_primary_i64: i64 = if primary_uid.is_empty() {
        0
    } else {
        let value_expr = display_expr("rv");
        let sql = format!(
            r#"
            SELECT COUNT(*)
            FROM (
                SELECT LOWER(TRIM({value_expr})) AS value_name
                FROM aris_records r
                JOIN aris_record_values rv
                  ON rv.record_uid = r.uid
                 AND rv.field_uid = ?8
                WHERE {filter_sql}
                  AND TRIM({value_expr}) <> ''
                GROUP BY LOWER(TRIM({value_expr}))
            ) valueset
            "#
        );
        connection.query_row(
            &sql,
            rusqlite::params![
                &filters.date_from,
                &filters.date_to,
                &filters.primary_value,
                &filters.secondary_value,
                &date_uid,
                &primary_uid,
                &secondary_uid,
                &primary_uid,
            ],
            |row| row.get(0),
        )?
    };

    let today_i64: i64 = if date_uid.is_empty() {
        0
    } else {
        connection.query_row(
            &format!(
                r#"
                SELECT COUNT(*)
                FROM aris_records r
                WHERE {filter_sql}
                  AND EXISTS (
                        SELECT 1
                        FROM aris_record_values rv
                        WHERE rv.record_uid = r.uid
                          AND rv.field_uid = ?8
                          AND rv.value_text = DATE('now', 'localtime')
                  )
                "#
            ),
            rusqlite::params![
                &filters.date_from,
                &filters.date_to,
                &filters.primary_value,
                &filters.secondary_value,
                &date_uid,
                &primary_uid,
                &secondary_uid,
                &date_uid,
            ],
            |row| row.get(0),
        )?
    };

    let month_i64: i64 = if date_uid.is_empty() {
        0
    } else {
        connection.query_row(
            &format!(
                r#"
                SELECT COUNT(*)
                FROM aris_records r
                WHERE {filter_sql}
                  AND EXISTS (
                        SELECT 1
                        FROM aris_record_values rv
                        WHERE rv.record_uid = r.uid
                          AND rv.field_uid = ?8
                          AND SUBSTR(rv.value_text, 1, 7) = STRFTIME('%Y-%m', 'now', 'localtime')
                  )
                "#
            ),
            rusqlite::params![
                &filters.date_from,
                &filters.date_to,
                &filters.primary_value,
                &filters.secondary_value,
                &date_uid,
                &primary_uid,
                &secondary_uid,
                &date_uid,
            ],
            |row| row.get(0),
        )?
    };

    let attachment_sql = format!(
        r#"
        SELECT COUNT(a.uid) AS attachments, COUNT(DISTINCT a.entry_uid) AS with_attachments
        FROM aris_records r
        LEFT JOIN aris_attachments a ON a.entry_uid = r.uid
        WHERE {filter_sql}
        "#
    );
    let attachment_params = filter_params(&filters, &plan);
    let (attachments_i64, with_attachments_i64): (i64, i64) = connection.query_row(
        &attachment_sql,
        rusqlite::params_from_iter(attachment_params),
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    let highest_i64: i64 = if auto_uid.is_empty() {
        0
    } else {
        connection.query_row(
            "SELECT COALESCE(MAX(value_integer), 0) FROM aris_record_values WHERE field_uid = ?1",
            params![&auto_uid],
            |row| row.get(0),
        )?
    };

    let duplicates_i64: i64 = if auto_uid.is_empty() {
        0
    } else {
        connection.query_row(
            &format!(
                r#"
                SELECT COALESCE(SUM(duplicate_count - 1), 0)
                FROM (
                    SELECT COUNT(*) AS duplicate_count
                    FROM aris_records r
                    JOIN aris_record_values rv
                      ON rv.record_uid = r.uid
                     AND rv.field_uid = ?8
                    WHERE {filter_sql}
                      AND rv.value_integer IS NOT NULL
                    GROUP BY rv.value_integer
                    HAVING COUNT(*) > 1
                ) duplicate_values
                "#
            ),
            rusqlite::params![
                &filters.date_from,
                &filters.date_to,
                &filters.primary_value,
                &filters.secondary_value,
                &date_uid,
                &primary_uid,
                &secondary_uid,
                &auto_uid,
            ],
            |row| row.get(0),
        )?
    };

    let remarks_i64: i64 = if narrative_uid.is_empty() {
        0
    } else {
        connection.query_row(
            &format!(
                r#"
                SELECT COUNT(*)
                FROM aris_records r
                WHERE {filter_sql}
                  AND EXISTS (
                        SELECT 1
                        FROM aris_record_values rv
                        WHERE rv.record_uid = r.uid
                          AND rv.field_uid = ?8
                          AND rv.value_text IS NOT NULL
                          AND TRIM(rv.value_text) <> ''
                  )
                "#
            ),
            rusqlite::params![
                &filters.date_from,
                &filters.date_to,
                &filters.primary_value,
                &filters.secondary_value,
                &date_uid,
                &primary_uid,
                &secondary_uid,
                &narrative_uid,
            ],
            |row| row.get(0),
        )?
    };

    let valid_dates_i64: i64 = if date_uid.is_empty() {
        0
    } else {
        connection.query_row(
            &format!(
                r#"
                SELECT COUNT(*)
                FROM aris_records r
                WHERE {filter_sql}
                  AND EXISTS (
                        SELECT 1
                        FROM aris_record_values rv
                        WHERE rv.record_uid = r.uid
                          AND rv.field_uid = ?8
                          AND LENGTH(TRIM(COALESCE(rv.value_text, ''))) = 10
                          AND DATE(rv.value_text) IS NOT NULL
                  )
                "#
            ),
            rusqlite::params![
                &filters.date_from,
                &filters.date_to,
                &filters.primary_value,
                &filters.secondary_value,
                &date_uid,
                &primary_uid,
                &secondary_uid,
                &date_uid,
            ],
            |row| row.get(0),
        )?
    };

    let records = records_i64.max(0) as u64;
    let attachments = attachments_i64.max(0) as u64;
    let with_attachments = with_attachments_i64.max(0) as u64;
    let attachment_coverage = if records == 0 {
        0.0
    } else {
        ((with_attachments as f64 / records as f64) * 1000.0).round() / 10.0
    };

    let volume = volume_points(connection, &filters, &plan, &bucket)?;

    Ok(DashboardSummary {
        revision,
        schema_revision,
        bucket,
        scope: DashboardScope {
            records,
            total_records: total_records_i64.max(0) as u64,
            today: today_i64.max(0) as u64,
            month: month_i64.max(0) as u64,
            attachments,
            with_attachments,
            offices: distinct_primary_i64.max(0) as u64,
            attachment_coverage,
            highest_control: highest_i64.max(0) as u64,
        },
        meta: DashboardMeta {
            primary_dimension: plan.primary_dimension.as_ref().map(to_field_ref),
            secondary_dimension: plan.secondary_dimension.as_ref().map(to_field_ref),
            tertiary_dimension: plan.tertiary_dimension.as_ref().map(to_field_ref),
            date_field: plan.date_field.as_ref().map(to_field_ref),
            auto_number_field: plan.auto_number_field.as_ref().map(to_field_ref),
            narrative_field: plan.narrative_field.as_ref().map(to_field_ref),
            required_fields: plan.required_fields.iter().map(to_field_ref).collect(),
        },
        top_offices: named_counts(
            connection,
            &filters,
            &plan,
            plan.primary_dimension.as_ref(),
            10,
        )?,
        top_routes: named_counts(
            connection,
            &filters,
            &plan,
            plan.secondary_dimension.as_ref(),
            8,
        )?,
        top_requestors: named_counts(
            connection,
            &filters,
            &plan,
            plan.tertiary_dimension.as_ref(),
            8,
        )?,
        volume,
        monthly_volume: monthly_volume(connection, &plan)?,
        attachment_types: attachment_types(connection, &filters, &plan)?,
        quality: DashboardQuality {
            required_complete: required_complete_i64.max(0) as u64,
            duplicate_controls: duplicates_i64.max(0) as u64,
            remarks: remarks_i64.max(0) as u64,
            valid_dates: valid_dates_i64.max(0) as u64,
        },
        dimensions: DashboardDimensions {
            offices: all_dimensions(connection, &filters, &plan, plan.primary_dimension.as_ref())?,
            routes: all_dimensions(
                connection,
                &filters,
                &plan,
                plan.secondary_dimension.as_ref(),
            )?,
        },
        recent_records: recent_records(connection, &filters, &plan)?,
    })
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
            "response": "The dashboard information could not be calculated."
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
        Ok(Err(error)) => return internal_error("get_dashboard_summary()", error),
        Err(error) => return internal_error("get_dashboard_summary()", error),
    };

    let etag = format!(
        "\"aris-dashboard-{}-{}\"",
        summary.revision, summary.schema_revision
    );

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
