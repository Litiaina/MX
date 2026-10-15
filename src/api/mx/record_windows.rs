//! Stateless, query-bound keyset windows. Cursors carry ordering values, not
//! permissions: every request rebuilds its authorized module/filter predicates.
use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize)]
pub struct WindowMetadata {
    pub cursors: Vec<String>,
    pub has_before: bool,
    pub has_after: bool,
    pub metadata_included: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct Cursor {
    scope: String,
    rowid: i64,
    value: Value,
}

fn invalid(message: &str) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(message.to_string())
}

fn scope(module: &str, query: &DynamicListQuery, attachments: bool) -> String {
    let bytes = serde_json::to_vec(&json!([
        module,
        query.q,
        query.match_mode,
        query.filters,
        query.attachments,
        query.sort_by,
        query.sort_dir,
        attachments
    ]))
    .unwrap();
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn decode(raw: &str, scope: &str) -> Result<Cursor, rusqlite::Error> {
    if raw.len() > 32768 {
        return Err(invalid("Record cursor is too large. Refresh this view."));
    }
    let cursor: Cursor = URL_SAFE_NO_PAD
        .decode(raw)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or_else(|| invalid("Invalid record cursor. Refresh this view."))?;
    if cursor.scope != scope
        || cursor.rowid < 1
        || !matches!(
            cursor.value,
            Value::Null | Value::String(_) | Value::Number(_)
        )
    {
        return Err(invalid(
            "Record cursor belongs to a different view. Refresh this view.",
        ));
    }
    Ok(cursor)
}

fn sql_value(value: &Value) -> SqlValue {
    match value {
        Value::String(value) => SqlValue::Text(value.clone()),
        Value::Number(value) => value
            .as_i64()
            .map(SqlValue::Integer)
            .unwrap_or_else(|| SqlValue::Real(value.as_f64().unwrap_or(0.0))),
        _ => SqlValue::Null,
    }
}

// A NULL marker makes ordering explicit in both directions. A rowid tie-break
// makes identical labels/numbers deterministic. No OFFSET is used.
fn boundary(expression: &str, asc: bool, before: bool) -> String {
    let cmp = if asc != before { ">" } else { "<" };
    let tie = if before { ">" } else { "<" };
    format!(
        "((({expression}) IS NOT NULL) {cmp} (? IS NOT NULL) OR \
        ((({expression}) IS NOT NULL) = (? IS NOT NULL) AND \
        (({expression}) {cmp} ? OR (({expression}) IS ? AND e.rowid {tie} ?))))"
    )
}

pub(super) fn list(
    connection: &rusqlite::Connection,
    module: &str,
    query: &DynamicListQuery,
    fields: &[FieldDefinition],
    attachments: bool,
) -> Result<DynamicPage, rusqlite::Error> {
    let direction = query.window.as_deref().unwrap_or("start");
    if !matches!(direction, "start" | "after" | "before" | "anchor") {
        return Err(invalid(
            "Record window must be start, after, before or anchor.",
        ));
    }
    let limit = query.limit.unwrap_or(50).clamp(1, MAX_PAGE_LIMIT);
    let field = query.sort_by.as_deref().and_then(|key| {
        fields
            .iter()
            .find(|field| field.key == key && field.sortable)
    });
    let signature = format!(
        "{}:{}",
        scope(module, query, attachments),
        field
            .map(|f| format!("{}:{}", f.uid, f.field_type))
            .unwrap_or_default()
    );
    let asc = field.is_some()
        && query
            .sort_dir
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("asc"));
    let expression = field.map(|field| {
        let column = match field.field_type.as_str() {
            "integer" | "auto_number" => "value_integer",
            "decimal" | "formula" => "value_real",
            "boolean" => "value_boolean",
            _ => "value_text COLLATE NOCASE",
        };
        let collate=if column.starts_with("value_text") { " COLLATE NOCASE" } else { "" };
        format!("(SELECT {column} FROM mx_record_values WHERE record_uid=e.uid AND field_uid='{}' LIMIT 1){collate}", quote_sql_literal(&field.uid))
    }).unwrap_or_else(|| "e.rowid".to_string());
    let (where_clause, params) = build_where_clause(module, query, fields, attachments)
        .map_err(rusqlite::Error::InvalidParameterName)?;
    let metadata = query.metadata.unwrap_or(true);
    let total: i64 = if metadata {
        connection.query_row(
            &format!("SELECT COUNT(*) FROM mx_records e {where_clause}"),
            params_from_iter(params.iter()),
            |row| row.get(0),
        )?
    } else {
        0
    };
    let summaries = if metadata {
        super::super::column_summaries::calculate(connection, fields, &where_clause, &params)?
    } else {
        BTreeMap::new()
    };
    let before = direction == "before";
    let mut cursor = if direction == "anchor" {
        None
    } else {
        query
            .cursor
            .as_deref()
            .map(|raw| decode(raw, &signature))
            .transpose()?
    };
    if direction == "anchor" {
        let uid = query
            .anchor_uid
            .as_deref()
            .ok_or_else(|| invalid("An anchor record is required."))?;
        let mut anchor_params = params.clone();
        anchor_params.push(SqlValue::Text(uid.to_string()));
        cursor = connection
            .query_row(
                &format!(
                    "SELECT e.rowid, {expression} FROM mx_records e {where_clause} AND e.uid=?"
                ),
                params_from_iter(anchor_params.iter()),
                |row| {
                    let value = row.get::<_, SqlValue>(1)?;
                    Ok(Cursor {
                        scope: signature.clone(),
                        rowid: row.get(0)?,
                        value: json_value(value),
                    })
                },
            )
            .optional()?;
        // A deleted/filtered anchor can still be positioned by its saved key.
        if cursor.is_none() {
            cursor = query
                .cursor
                .as_deref()
                .and_then(|raw| decode(raw, &signature).ok());
        }
    } else if direction != "start" && cursor.is_none() {
        return Err(invalid("A record cursor is required."));
    }
    let mut data_params = params.clone();
    let mut seek = String::new();
    if let Some(cursor) = &cursor {
        if field.is_none() {
            let comparison = if before {
                ">"
            } else if direction == "anchor" {
                "<="
            } else {
                "<"
            };
            seek = format!(" AND e.rowid {comparison} ?");
            data_params.push(SqlValue::Integer(cursor.rowid));
        } else {
            seek = format!(" AND ({})", boundary(&expression, asc, before));
            let value = sql_value(&cursor.value);
            data_params.extend([value.clone(), value.clone(), value.clone(), value]);
            data_params.push(SqlValue::Integer(cursor.rowid));
            if direction == "anchor" {
                seek = format!(
                    " AND (({}) OR e.rowid=?)",
                    boundary(&expression, asc, false)
                );
                data_params.push(SqlValue::Integer(cursor.rowid));
            }
        }
    }
    let sort = if asc != before { "ASC" } else { "DESC" };
    let tie = if before { "ASC" } else { "DESC" };
    data_params.push(SqlValue::Integer((limit + 1) as i64));
    let mut statement = connection.prepare(&format!("SELECT e.uid,e.rowid,{expression} FROM mx_records e {where_clause}{seek} ORDER BY {expression} {sort},e.rowid {tie} LIMIT ?"))?;
    let mut selected = statement
        .query_map(params_from_iter(data_params.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                Cursor {
                    scope: signature.clone(),
                    rowid: row.get(1)?,
                    value: json_value(row.get(2)?),
                },
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let more = selected.len() > limit;
    selected.truncate(limit);
    if before {
        selected.reverse();
    }
    let has_before = if before { more } else { cursor.is_some() };
    let has_after = if before { true } else { more };
    let uids = selected
        .iter()
        .map(|(uid, _)| uid.clone())
        .collect::<Vec<_>>();
    let mut values = load_record_values(connection, &uids)?;
    let mut revisions = load_record_revisions_initialized(connection, &uids)?;
    let mut files = if attachments {
        load_attachments_initialized(connection, &uids)?
    } else {
        HashMap::new()
    };
    for record_values in values.values_mut() {
        calculate_formula_json_values(fields, record_values)
            .map_err(rusqlite::Error::InvalidParameterName)?;
    }
    let cursors = selected
        .iter()
        .map(|(_, cursor)| URL_SAFE_NO_PAD.encode(serde_json::to_vec(cursor).unwrap()))
        .collect();
    let data = uids
        .into_iter()
        .map(|uid| DynamicRecord {
            values: values.remove(&uid).unwrap_or_default(),
            revision: revisions.remove(&uid).unwrap_or(1),
            attached_files: files.remove(&uid).unwrap_or_default(),
            uid,
        })
        .collect();
    let total = total.max(0) as usize;
    Ok(DynamicPage {
        data,
        page: 1,
        limit,
        has_next: has_after,
        total,
        total_pages: total.div_ceil(limit).max(1),
        summaries,
        window: Some(WindowMetadata {
            cursors,
            has_before,
            has_after,
            metadata_included: metadata,
        }),
    })
}

fn json_value(value: SqlValue) -> Value {
    match value {
        SqlValue::Integer(v) => json!(v),
        SqlValue::Real(v) => json!(v),
        SqlValue::Text(v) => json!(v),
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(kind: &str) -> (rusqlite::Connection, Vec<FieldDefinition>) {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE mx_records(uid TEXT PRIMARY KEY,module_uid TEXT,deleted_at INTEGER,revision INTEGER); \
          CREATE TABLE mx_fields(uid TEXT PRIMARY KEY,field_key TEXT,module_key TEXT,field_type TEXT,active INTEGER,position INTEGER,label TEXT,searchable INTEGER); \
          CREATE TABLE mx_record_values(record_uid TEXT,field_uid TEXT,value_text TEXT,value_integer INTEGER,value_real REAL,value_boolean INTEGER,PRIMARY KEY(record_uid,field_uid));").unwrap();
        let field:FieldDefinition=serde_json::from_value(json!({"uid":"field","key":"value","label":"Value","field_type":kind,"required":false,"unique_value":false,"searchable":true,"sortable":true,"table_visible":true,"table_priority":1,"position":0,"active":true,"config":{}})).unwrap();
        db.execute(
            "INSERT INTO mx_fields VALUES('field','value','value',?1,1,0,'Value',1)",
            [kind],
        )
        .unwrap();
        for i in 0..73 {
            let uid = format!("r{i}");
            db.execute(
                "INSERT INTO mx_records VALUES(?1,?2,NULL,1)",
                params![uid, if i == 72 { "other" } else { "module" }],
            )
            .unwrap();
            if i % 7 != 0 {
                db.execute(
                    "INSERT INTO mx_record_values VALUES(?1,'field',?2,?3,?4,?5)",
                    params![
                        uid,
                        ["alpha", "ALPHA", "beta", "zeta"][i % 4],
                        (i % 9) as i64,
                        (i % 9) as f64 / 3.0,
                        (i % 2) as i64
                    ],
                )
                .unwrap();
            }
        }
        (db, vec![field])
    }
    #[test]
    fn keyset_roundtrip_with_nulls_ties_and_every_scalar_sort() {
        for kind in ["text", "integer", "decimal", "boolean"] {
            for direction in ["asc", "desc"] {
                let (db, fields) = fixture(kind);
                let mut query = DynamicListQuery {
                    window: Some("start".into()),
                    limit: Some(11),
                    sort_by: Some("value".into()),
                    sort_dir: Some(direction.into()),
                    metadata: Some(false),
                    ..Default::default()
                };
                let mut seen = Vec::new();
                let mut batches = Vec::new();
                loop {
                    let page = list(&db, "module", &query, &fields, false).unwrap();
                    let meta = page.window.unwrap();
                    seen.extend(page.data.iter().map(|r| r.uid.clone()));
                    batches.push((
                        page.data.iter().map(|r| r.uid.clone()).collect::<Vec<_>>(),
                        meta.cursors.clone(),
                    ));
                    if !meta.has_after {
                        break;
                    }
                    query.window = Some("after".into());
                    query.cursor = meta.cursors.last().cloned();
                    assert!(batches.len() < 20);
                }
                assert_eq!(seen.len(), 72, "{kind} {direction}");
                assert_eq!(
                    seen.iter().collect::<std::collections::HashSet<_>>().len(),
                    72
                );
                for index in 1..batches.len() {
                    query.window = Some("before".into());
                    query.cursor = Some(batches[index].1[0].clone());
                    let back = list(&db, "module", &query, &fields, false).unwrap();
                    assert_eq!(
                        back.data.iter().map(|r| r.uid.clone()).collect::<Vec<_>>(),
                        batches[index - 1].0,
                        "backwards {kind} {direction}"
                    );
                }
            }
        }
    }
    #[test]
    fn deleted_anchor_is_resumable_without_resurrecting_it() {
        let (db, fields) = fixture("integer");
        let mut q = DynamicListQuery {
            window: Some("start".into()),
            limit: Some(12),
            metadata: Some(false),
            ..Default::default()
        };
        let first = list(&db, "module", &q, &fields, false).unwrap();
        q.cursor = Some(first.window.unwrap().cursors[5].clone());
        q.anchor_uid = Some(first.data[5].uid.clone());
        q.window = Some("anchor".into());
        let before = list(&db, "module", &q, &fields, false).unwrap();
        assert_eq!(before.data[0].uid, first.data[5].uid);
        db.execute(
            "UPDATE mx_records SET deleted_at=1 WHERE uid=?1",
            [&first.data[5].uid],
        )
        .unwrap();
        let after = list(&db, "module", &q, &fields, false).unwrap();
        assert_eq!(after.data[0].uid, first.data[6].uid);
    }
    #[test]
    fn cursors_cannot_cross_modules_filters_or_sort_orders() {
        let (db, fields) = fixture("integer");
        let mut q = DynamicListQuery {
            window: Some("start".into()),
            metadata: Some(false),
            ..Default::default()
        };
        let page = list(&db, "module", &q, &fields, false).unwrap();
        q.cursor = page.window.unwrap().cursors.last().cloned();
        q.window = Some("after".into());
        assert!(list(&db, "other", &q, &fields, false).is_err());
        q.sort_dir = Some("asc".into());
        assert!(list(&db, "module", &q, &fields, false).is_err());
        q.sort_dir = None;
        q.q = Some("alpha".into());
        assert!(list(&db, "module", &q, &fields, false).is_err());
        q.q = None;
        q.cursor = Some("invalid".into());
        assert!(list(&db, "module", &q, &fields, false).is_err());
    }
}
