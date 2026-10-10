//! Persistent record links, not edit sessions. Values participate in normal
//! record revisions; the indexed edges enforce referential integrity.
use std::collections::BTreeMap;

use super::schema::{FieldDefinition, load_module_fields_initialized_db};
use crate::{
    api::modules::module_can_for_user_initialized,
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::{Claims, auth},
};
use axum::{
    Json, Router,
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

fn invalid(message: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(message.into())
}
pub(crate) fn computed(kind: &str) -> bool {
    matches!(kind, "lookup" | "rollup")
}

pub(crate) fn config(kind: &str, value: &Value) -> Result<Value, String> {
    let required = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| format!("Configure {key} for this {kind} field."))
    };
    if kind == "relationship" {
        let module = required("target_module_uid")?;
        let label = required("label_field")?;
        Ok(
            json!({"target_module_uid":module,"label_field":label,"multiple":value.get("multiple").and_then(Value::as_bool).unwrap_or(false),"on_delete":"restrict"}),
        )
    } else {
        let relationship = required("relationship_field")?;
        let target = value
            .get("target_field")
            .and_then(Value::as_str)
            .unwrap_or("");
        let function = value
            .get("function")
            .and_then(Value::as_str)
            .unwrap_or("sum");
        if kind == "lookup" && target.is_empty() {
            return Err("Choose the field to look up.".into());
        }
        if kind == "rollup"
            && (!matches!(function, "count" | "sum" | "average" | "min" | "max")
                || (function != "count" && target.is_empty()))
        {
            return Err("Choose count, sum, average, min, or max and a numeric field.".into());
        }
        Ok(json!({"relationship_field":relationship,"target_field":target,"function":function}))
    }
}

pub(crate) fn validate_config(
    c: &Connection,
    module: &str,
    kind: &str,
    config: &Value,
) -> rusqlite::Result<()> {
    if kind == "relationship" {
        let target = config["target_module_uid"].as_str().unwrap_or("");
        let valid: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM mx_modules WHERE uid=?1 AND active=1)",
            [target],
            |r| r.get(0),
        )?;
        if !valid {
            return Err(invalid("MX_CONFIG:Choose an active target module."));
        }
        let label = config["label_field"].as_str().unwrap_or("");
        let fields = load_module_fields_initialized_db(c, target, false)?;
        if !fields.iter().any(|f| {
            f.key == label
                && !matches!(
                    f.field_type.as_str(),
                    "attachments" | "relationship" | "lookup" | "rollup"
                )
        }) {
            return Err(invalid(
                "MX_CONFIG:Choose a regular display field in the target module.",
            ));
        }
    } else if computed(kind) {
        let fields = load_module_fields_initialized_db(c, module, false)?;
        let relationship = fields
            .iter()
            .find(|f| {
                f.key == config["relationship_field"].as_str().unwrap_or("")
                    && f.field_type == "relationship"
            })
            .ok_or_else(|| {
                invalid("MX_CONFIG:Choose an active relationship field in this module.")
            })?;
        let target = relationship.config["target_module_uid"]
            .as_str()
            .unwrap_or("");
        let targets = load_module_fields_initialized_db(c, target, false)?;
        let key = config["target_field"].as_str().unwrap_or("");
        if kind == "rollup" && config["function"] == "count" {
            return Ok(());
        }
        let field = targets
            .iter()
            .find(|f| f.key == key)
            .ok_or_else(|| invalid("MX_CONFIG:Choose an active field in the linked module."))?;
        if matches!(
            field.field_type.as_str(),
            "attachments" | "relationship" | "lookup" | "rollup"
        ) || (kind == "rollup"
            && !matches!(
                field.field_type.as_str(),
                "integer" | "decimal" | "auto_number" | "formula"
            ))
        {
            return Err(invalid(
                "MX_CONFIG:Unsupported lookup or nonnumeric rollup target.",
            ));
        }
    }
    Ok(())
}

pub(crate) fn normalize(field: &FieldDefinition, value: &Value) -> Result<Option<String>, String> {
    let multiple = field.config["multiple"].as_bool().unwrap_or(false);
    let mut ids = if value.is_null() || value.as_str() == Some("") {
        vec![]
    } else if multiple {
        value
            .as_array()
            .ok_or_else(|| format!("{} must contain record selections.", field.label))?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "Select valid records.".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![
            value
                .as_str()
                .ok_or_else(|| "Select a record.".to_string())?
                .to_owned(),
        ]
    };
    if ids.len() > 100 {
        return Err("Select at most 100 linked records per field.".into());
    }
    for id in &ids {
        if Uuid::parse_str(id).is_err() {
            return Err("Select a valid record, not a typed label.".into());
        }
    }
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        Ok(None)
    } else if multiple {
        Ok(Some(json!(ids).to_string()))
    } else {
        Ok(ids.pop())
    }
}
pub(crate) fn ids(value: &Value) -> Vec<String> {
    if let Some(items) = value.as_array() {
        items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    } else {
        value
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| vec![s.to_owned()])
            .unwrap_or_default()
    }
}

pub(crate) fn ensure(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch(r#"
      CREATE TABLE IF NOT EXISTS mx_record_links (
        record_uid TEXT NOT NULL REFERENCES mx_records(uid) ON DELETE CASCADE,
        field_uid TEXT NOT NULL REFERENCES mx_fields(uid) ON DELETE CASCADE,
        target_uid TEXT NOT NULL REFERENCES mx_records(uid) ON DELETE RESTRICT,
        PRIMARY KEY(record_uid,field_uid,target_uid));
      CREATE INDEX IF NOT EXISTS mx_record_links_target ON mx_record_links(target_uid,field_uid,record_uid);
      CREATE TRIGGER IF NOT EXISTS mx_link_insert AFTER INSERT ON mx_record_values
      WHEN (SELECT field_type FROM mx_fields WHERE uid=NEW.field_uid)='relationship' BEGIN
        SELECT CASE WHEN EXISTS(SELECT 1 FROM json_each(CASE WHEN json_valid(NEW.value_text) THEN NEW.value_text ELSE json_array(NEW.value_text) END) j
          WHERE NOT EXISTS(SELECT 1 FROM mx_records r JOIN mx_fields f ON f.uid=NEW.field_uid WHERE r.uid=j.value AND r.deleted_at IS NULL AND r.module_uid=json_extract(f.config_json,'$.target_module_uid')))
          THEN RAISE(ABORT,'Linked record is unavailable or belongs to another module.') END;
        INSERT INTO mx_record_links SELECT NEW.record_uid,NEW.field_uid,value FROM json_each(CASE WHEN json_valid(NEW.value_text) THEN NEW.value_text ELSE json_array(NEW.value_text) END);
      END;
      CREATE TRIGGER IF NOT EXISTS mx_link_remove AFTER DELETE ON mx_record_values BEGIN
        DELETE FROM mx_record_links WHERE record_uid=OLD.record_uid AND field_uid=OLD.field_uid;
      END;
      CREATE TRIGGER IF NOT EXISTS mx_link_update AFTER UPDATE ON mx_record_values
      WHEN (SELECT field_type FROM mx_fields WHERE uid=NEW.field_uid)='relationship' BEGIN
        DELETE FROM mx_record_links WHERE record_uid=OLD.record_uid AND field_uid=OLD.field_uid;
        SELECT CASE WHEN EXISTS(SELECT 1 FROM json_each(CASE WHEN json_valid(NEW.value_text) THEN NEW.value_text ELSE json_array(NEW.value_text) END) j
          WHERE NOT EXISTS(SELECT 1 FROM mx_records r JOIN mx_fields f ON f.uid=NEW.field_uid WHERE r.uid=j.value AND r.deleted_at IS NULL AND r.module_uid=json_extract(f.config_json,'$.target_module_uid')))
          THEN RAISE(ABORT,'Linked record is unavailable or belongs to another module.') END;
        INSERT INTO mx_record_links SELECT NEW.record_uid,NEW.field_uid,value FROM json_each(CASE WHEN json_valid(NEW.value_text) THEN NEW.value_text ELSE json_array(NEW.value_text) END);
      END;
      CREATE TRIGGER IF NOT EXISTS mx_link_restrict_trash BEFORE UPDATE OF deleted_at ON mx_records
      WHEN OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL BEGIN
        SELECT CASE WHEN EXISTS(SELECT 1 FROM mx_record_links l JOIN mx_records r ON r.uid=l.record_uid WHERE l.target_uid=OLD.uid AND r.deleted_at IS NULL AND r.uid!=OLD.uid)
          THEN RAISE(ABORT,'Remove active record relationships before deleting this record.') END;
      END;
      CREATE TRIGGER IF NOT EXISTS mx_link_restrict_module BEFORE DELETE ON mx_modules BEGIN
        SELECT CASE WHEN EXISTS(SELECT 1 FROM mx_fields WHERE module_uid!=OLD.uid AND active=1 AND field_type='relationship' AND json_extract(config_json,'$.target_module_uid')=OLD.uid)
          THEN RAISE(ABORT,'Remove relationships to this module before deleting it.') END;
      END;
      CREATE TRIGGER IF NOT EXISTS mx_link_restore BEFORE UPDATE OF deleted_at ON mx_records
      WHEN OLD.deleted_at IS NOT NULL AND NEW.deleted_at IS NULL BEGIN
        SELECT CASE WHEN EXISTS(SELECT 1 FROM mx_record_links l JOIN mx_records t ON t.uid=l.target_uid WHERE l.record_uid=OLD.uid AND t.deleted_at IS NOT NULL)
          THEN RAISE(ABORT,'Restore the linked records before restoring this record.') END;
      END;
    "#)
}

pub(crate) fn validate_selection(
    c: &Connection,
    fields: &[FieldDefinition],
    values: &BTreeMap<String, Value>,
    actor: &str,
) -> rusqlite::Result<()> {
    if !fields
        .iter()
        .any(|f| f.field_type == "relationship" && values.contains_key(&f.key))
    {
        return Ok(());
    }
    let level: i64 = c.query_row(
        "SELECT access_level FROM users WHERE uid=?1",
        [actor],
        |r| r.get(0),
    )?;
    for field in fields.iter().filter(|f| f.field_type == "relationship") {
        let Some(value) = values.get(&field.key) else {
            continue;
        };
        let target = field.config["target_module_uid"].as_str().unwrap_or("");
        if !ids(value).is_empty()
            && !module_can_for_user_initialized(c, target, actor, level, "read")?
        {
            return Err(invalid("You cannot select records from that module."));
        }
        for id in ids(value) {
            let exists:bool = c.query_row("SELECT EXISTS(SELECT 1 FROM mx_records WHERE uid=?1 AND module_uid=?2 AND deleted_at IS NULL)",params![id,target],|r|r.get(0))?;
            if !exists {
                return Err(invalid(
                    "The selected related record is unavailable. Choose another record.",
                ));
            }
        }
    }
    Ok(())
}

fn scalar(c: &Connection, id: &str, key: &str) -> rusqlite::Result<Value> {
    c.query_row("SELECT f.field_type,v.value_text,v.value_integer,v.value_real,v.value_boolean FROM mx_record_values v JOIN mx_fields f ON f.uid=v.field_uid WHERE v.record_uid=?1 AND COALESCE(f.module_key,f.field_key)=?2 AND f.active=1",params![id,key],|r|{
        let kind:String=r.get(0)?;
        Ok(match kind.as_str(){"integer"|"auto_number"=>r.get::<_,Option<i64>>(2)?.map_or(Value::Null,|v|json!(v)),"decimal"|"formula"=>r.get::<_,Option<f64>>(3)?.map_or(Value::Null,|v|json!(v)),"boolean"=>r.get::<_,Option<i64>>(4)?.map_or(Value::Null,|v|json!(v!=0)),_=>r.get::<_,Option<String>>(1)?.map_or(Value::Null,Value::String)})
    }).optional().map(|v|v.unwrap_or(Value::Null))
}
fn label(c: &Connection, id: &str, key: &str) -> rusqlite::Result<String> {
    let value = scalar(c, id, key)?;
    Ok(value.as_str().map(str::to_owned).unwrap_or_else(|| {
        if value.is_null() {
            "Unnamed record".into()
        } else {
            value.to_string()
        }
    }))
}

pub(crate) fn present(c: &Connection, claims: &Claims, record: &mut Value) -> rusqlite::Result<()> {
    let id = record["uid"].as_str().unwrap_or("").to_owned();
    let module: String = c.query_row(
        "SELECT module_uid FROM mx_records WHERE uid=?1",
        [&id],
        |r| r.get(0),
    )?;
    let fields = load_module_fields_initialized_db(c, &module, false)?;
    present_fields(c, claims, record, &fields, &mut BTreeMap::new())
}

pub(crate) fn present_many(
    c: &Connection,
    claims: &Claims,
    records: &mut [Value],
) -> rusqlite::Result<()> {
    if records.is_empty() {
        return Ok(());
    }
    let module: String = c.query_row(
        "SELECT module_uid FROM mx_records WHERE uid=?1",
        [records[0]["uid"].as_str().unwrap_or("")],
        |r| r.get(0),
    )?;
    let fields = load_module_fields_initialized_db(c, &module, false)?;
    let mut access = BTreeMap::new();
    for record in records {
        present_fields(c, claims, record, &fields, &mut access)?;
    }
    Ok(())
}
fn present_fields(
    c: &Connection,
    claims: &Claims,
    record: &mut Value,
    fields: &[FieldDefinition],
    access: &mut BTreeMap<String, bool>,
) -> rusqlite::Result<()> {
    if !fields
        .iter()
        .any(|f| matches!(f.field_type.as_str(), "relationship" | "lookup" | "rollup"))
    {
        return Ok(());
    }
    let mut labels = json!({});
    let mut allowed = BTreeMap::new();
    for field in fields.iter().filter(|f| f.field_type == "relationship") {
        let target = field.config["target_module_uid"].as_str().unwrap_or("");
        let can = if let Some(can) = access.get(target) {
            *can
        } else {
            let can = module_can_for_user_initialized(
                c,
                target,
                &claims.uid,
                claims.access_level,
                "read",
            )?;
            access.insert(target.to_owned(), can);
            can
        };
        allowed.insert(field.key.clone(), can);
        if !can {
            record["values"][&field.key] = Value::Null;
            labels[&field.key] = json!({"restricted":true,"items":[]});
            continue;
        }
        let items=ids(&record["values"][&field.key]).into_iter().map(|uid|Ok(json!({"uid":uid,"label":label(c,&uid,field.config["label_field"].as_str().unwrap_or(""))?}))).collect::<rusqlite::Result<Vec<_>>>()?;
        labels[&field.key] = json!({"restricted":false,"items":items});
    }
    for field in fields.iter().filter(|f| computed(&f.field_type)) {
        let relation = field.config["relationship_field"].as_str().unwrap_or("");
        if !allowed.get(relation).copied().unwrap_or(false) {
            record["values"][&field.key] = Value::Null;
            continue;
        }
        let linked = ids(&record["values"][relation]);
        let values = linked
            .iter()
            .map(|id| scalar(c, id, field.config["target_field"].as_str().unwrap_or("")))
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let numbers = values.iter().filter_map(Value::as_f64).collect::<Vec<_>>();
        record["values"][&field.key] = if field.field_type == "lookup" {
            json!(values)
        } else {
            match field.config["function"].as_str().unwrap_or("sum") {
                "count" => json!(linked.len()),
                "sum" => json!(numbers.iter().fold(0.0, |sum, number| sum + number)),
                "average" => {
                    if numbers.is_empty() {
                        Value::Null
                    } else {
                        json!(
                            numbers.iter().fold(0.0, |sum, number| sum + number)
                                / numbers.len() as f64
                        )
                    }
                }
                "min" => numbers
                    .iter()
                    .copied()
                    .reduce(f64::min)
                    .map_or(Value::Null, |n| json!(n)),
                "max" => numbers
                    .iter()
                    .copied()
                    .reduce(f64::max)
                    .map_or(Value::Null, |n| json!(n)),
                _ => Value::Null,
            }
        };
    }
    record["relationships"] = labels;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys=ON;CREATE TABLE mx_modules(uid TEXT PRIMARY KEY);INSERT INTO mx_modules VALUES('a'),('b');CREATE TABLE mx_records(uid TEXT PRIMARY KEY,module_uid TEXT,deleted_at INTEGER);INSERT INTO mx_records VALUES('source','a',NULL),('target','b',NULL),('other','a',NULL);CREATE TABLE mx_fields(uid TEXT PRIMARY KEY,module_uid TEXT,field_type TEXT,config_json TEXT,active INTEGER);INSERT INTO mx_fields VALUES('link','a','relationship','{\"target_module_uid\":\"b\"}',1);CREATE TABLE mx_record_values(record_uid TEXT,field_uid TEXT,value_text TEXT);").unwrap();
        ensure(&c).unwrap();
        c
    }
    #[test]
    fn edges_follow_value_changes_and_restrict_trash_and_module_deletion() {
        let c = db();
        c.execute(
            "INSERT INTO mx_record_values VALUES('source','link','target')",
            [],
        )
        .unwrap();
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM mx_record_links", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert!(
            c.execute("UPDATE mx_records SET deleted_at=1 WHERE uid='target'", [])
                .is_err()
        );
        assert!(
            c.execute("DELETE FROM mx_modules WHERE uid='b'", [])
                .is_err()
        );
        assert!(
            c.execute("UPDATE mx_record_values SET value_text='other'", [])
                .is_err()
        );
        assert_eq!(
            c.query_row("SELECT target_uid FROM mx_record_links", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "target"
        );
        c.execute("DELETE FROM mx_record_values", []).unwrap();
        c.execute("UPDATE mx_records SET deleted_at=1 WHERE uid='target'", [])
            .unwrap();
        assert!(
            c.execute(
                "INSERT INTO mx_record_values VALUES('source','link','target')",
                []
            )
            .is_err()
        );
    }
    #[test]
    fn a_trashed_source_cannot_be_restored_with_missing_targets() {
        let c = db();
        c.execute(
            "INSERT INTO mx_record_values VALUES('source','link','target')",
            [],
        )
        .unwrap();
        c.execute("UPDATE mx_records SET deleted_at=1 WHERE uid='source'", [])
            .unwrap();
        c.execute("UPDATE mx_records SET deleted_at=1 WHERE uid='target'", [])
            .unwrap();
        assert!(
            c.execute(
                "UPDATE mx_records SET deleted_at=NULL WHERE uid='source'",
                []
            )
            .is_err()
        );
        c.execute(
            "UPDATE mx_records SET deleted_at=NULL WHERE uid='target'",
            [],
        )
        .unwrap();
        c.execute(
            "UPDATE mx_records SET deleted_at=NULL WHERE uid='source'",
            [],
        )
        .unwrap();
    }
}

#[derive(Deserialize)]
struct Options {
    q: Option<String>,
    page: Option<usize>,
}
pub fn routes() -> Router {
    Router::new()
        .route("/mx/v1/relationships/{field}/options", get(options))
        .route(
            "/mx/v1/relationships/{module}/{record}/related",
            get(related),
        )
        .layer(axum::middleware::from_fn(auth))
}
fn response(result: Result<Value, SqliteDatabaseError>) -> Response {
    match result {
        Ok(v) => Json(v).into_response(),
        Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(message))) => {
            let status = if message.starts_with("Access") {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::BAD_REQUEST
            };
            (status, Json(json!({"response":message}))).into_response()
        }
        Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows)) => (
            StatusCode::NOT_FOUND,
            Json(json!({"response":"Related record or field was not found."})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"response":"Related record lookup failed. Try again."})),
        )
            .into_response(),
    }
}
async fn options(
    claims: Claims,
    Path(uid): Path<String>,
    Query(query): Query<Options>,
) -> Response {
    let result=tokio::task::spawn_blocking(move||with_sql_connection(|c|{
        let source:String=c.query_row("SELECT module_uid FROM mx_fields WHERE uid=?1 AND active=1 AND field_type='relationship'",[&uid],|r|r.get(0))?;
        let field=load_module_fields_initialized_db(c,&source,false)?.into_iter().find(|f|f.uid==uid).ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        let target=field.config["target_module_uid"].as_str().unwrap_or("");
        if !module_can_for_user_initialized(c,&source,&claims.uid,claims.access_level,"read")? || !module_can_for_user_initialized(c,target,&claims.uid,claims.access_level,"read")? {return Err(invalid("Access to linked records is restricted."));}
        let pattern=format!("%{}%",query.q.unwrap_or_default().chars().take(150).collect::<String>().replace('\\',"\\\\").replace('%',"\\%").replace('_',"\\_"));
        let page=query.page.unwrap_or(1).clamp(1,1_000_000);let key=field.config["label_field"].as_str().unwrap_or("");
        let mut statement=c.prepare("SELECT r.uid FROM mx_records r LEFT JOIN mx_fields f ON f.module_uid=r.module_uid AND f.active=1 AND COALESCE(f.module_key,f.field_key)=?2 LEFT JOIN mx_record_values v ON v.record_uid=r.uid AND v.field_uid=f.uid WHERE r.module_uid=?1 AND r.deleted_at IS NULL AND COALESCE(v.value_text,CAST(v.value_integer AS TEXT),CAST(v.value_real AS TEXT),CASE v.value_boolean WHEN 1 THEN 'true' WHEN 0 THEN 'false' END,'Unnamed record') LIKE ?3 ESCAPE '\\' ORDER BY COALESCE(v.value_text,CAST(v.value_integer AS TEXT),CAST(v.value_real AS TEXT),CASE v.value_boolean WHEN 1 THEN 'true' WHEN 0 THEN 'false' END,'Unnamed record') COLLATE NOCASE,r.uid LIMIT 51 OFFSET ?4")?;
        let ids=statement.query_map(params![target,key,pattern,((page-1)*50) as i64],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let items=ids.iter().take(50).map(|id|Ok(json!({"uid":id,"label":label(c,id,key)?}))).collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(json!({"items":items,"page":page,"has_next":ids.len()>50}))
    })).await;
    match result {
        Ok(result) => response(result),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"response":"Relationship task failed."})),
        )
            .into_response(),
    }
}
async fn related(
    claims: Claims,
    Path((module, uid)): Path<(String, String)>,
    Query(query): Query<Options>,
) -> Response {
    let result=tokio::task::spawn_blocking(move||with_sql_connection(|c|{
        if !module_can_for_user_initialized(c,&module,&claims.uid,claims.access_level,"read")? {return Err(invalid("Access denied."));}
        let exists:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM mx_records WHERE uid=?1 AND module_uid=?2 AND deleted_at IS NULL)",params![uid,module],|r|r.get(0))?;
        if !exists{return Err(rusqlite::Error::QueryReturnedNoRows);}
        // Filter allowed modules in SQL BEFORE pagination, never leak hidden counts.
        let modules=c.prepare("SELECT uid FROM mx_modules WHERE active=1")?.query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let allowed=modules.into_iter().filter_map(|m|match module_can_for_user_initialized(c,&m,&claims.uid,claims.access_level,"read"){Ok(true)=>Some(Ok(m)),Ok(false)=>None,Err(e)=>Some(Err(e))}).collect::<rusqlite::Result<Vec<_>>>()?;
        let page=query.page.unwrap_or(1).clamp(1,1_000_000);
        let mut statement=c.prepare("SELECT r.uid,r.module_uid,m.name,f.label FROM mx_record_links l JOIN mx_records r ON r.uid=l.record_uid JOIN mx_modules m ON m.uid=r.module_uid JOIN mx_fields f ON f.uid=l.field_uid WHERE l.target_uid=?1 AND r.deleted_at IS NULL AND f.active=1 AND r.module_uid IN (SELECT value FROM json_each(?2)) ORDER BY m.name,r.uid,f.uid LIMIT 51 OFFSET ?3")?;
        let rows=statement.query_map(params![uid,json!(allowed).to_string(),((page-1)*50) as i64],|r|Ok(json!({"uid":r.get::<_,String>(0)?,"module_uid":r.get::<_,String>(1)?,"module_name":r.get::<_,String>(2)?,"relationship_label":r.get::<_,String>(3)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let more=rows.len()>50;
        let mut display_fields=BTreeMap::new();let mut items=Vec::new();
        for mut row in rows.into_iter().take(50) {
            let source=row["module_uid"].as_str().unwrap_or("");
            if !display_fields.contains_key(source) {
                let key=load_module_fields_initialized_db(c,source,false)?.into_iter().find(|f|!matches!(f.field_type.as_str(),"attachments"|"relationship"|"lookup"|"rollup")).map(|f|f.key).unwrap_or_default();
                display_fields.insert(source.to_owned(),key);
            }
            row["label"]=json!(label(c,row["uid"].as_str().unwrap_or(""),&display_fields[source])?);
            items.push(row);
        }
        Ok(json!({"items":items,"page":page,"has_next":more}))
    })).await;
    match result {
        Ok(result) => response(result),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"response":"Relationship task failed."})),
        )
            .into_response(),
    }
}
