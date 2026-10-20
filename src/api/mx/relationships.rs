//! Persistent record links, not edit sessions. Values participate in normal
//! record revisions; the indexed edges enforce referential integrity.
use std::collections::{BTreeMap, BTreeSet};

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
    routing::{get, post},
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
            .ok_or_else(|| match key {
                "target_module_uid" => {
                    "Choose a module in ‘Linked module’ for this relationship field.".to_string()
                }
                "label_field" => {
                    "Choose a field in ‘Display field’ from the linked module.".to_string()
                }
                "relationship_field" => format!(
                    "Choose a field in ‘Relationship’ before configuring this {kind} field."
                ),
                _ => format!("Complete the {kind} field configuration."),
            })
    };
    if kind == "relationship" {
        let module = required("target_module_uid")?;
        let label = required("label_field")?;
        let mappings = value.get("auto_fill").cloned().unwrap_or_else(|| json!([]));
        let Some(items) = mappings.as_array() else {
            return Err("Auto-fill must be a list of field mappings.".into());
        };
        if items.len() > 64 {
            return Err("Configure at most 64 auto-fill mappings.".into());
        }
        let mut destinations = BTreeSet::new();
        for item in items {
            for key in ["source_field", "target_field"] {
                if item
                    .get(key)
                    .and_then(Value::as_str)
                    .is_none_or(|s| s.trim().is_empty())
                {
                    return Err("Choose both a linked source field and a field to fill.".into());
                }
            }
            if !destinations.insert(item["target_field"].as_str().unwrap()) {
                return Err("Each destination field can only be filled once.".into());
            }
        }
        if !items.is_empty() && value.get("multiple") == Some(&Value::Bool(true)) {
            return Err(
                "Auto-fill requires a single linked record, not multiple selections.".into(),
            );
        }
        Ok(
            json!({"target_module_uid":module,"label_field":label,"multiple":value.get("multiple").and_then(Value::as_bool).unwrap_or(false),"on_delete":"restrict","auto_fill":mappings}),
        )
    } else {
        if kind == "rollup" && value["direction"] == "incoming" {
            let source = required("source_module_uid")?;
            let relation = required("relationship_field")?;
            let function = value["function"].as_str().unwrap_or("sum");
            let target = value["target_field"].as_str().unwrap_or("");
            if !matches!(function, "count" | "sum" | "average" | "min" | "max")
                || (function != "count" && target.is_empty())
            {
                return Err("Choose a calculation and a numeric source field.".into());
            }
            return Ok(
                json!({"direction":"incoming","source_module_uid":source,"relationship_field":relation,"target_field":target,"function":function,"filter":value.get("filter").cloned().unwrap_or(Value::Null)}),
            );
        }
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

/// Run inside the schema transaction, after a definition changes. An incoming
/// total lives in a DIFFERENT module, so validating only the edited module can
/// leave a broken reference even when it contains no records yet.
pub(crate) fn validate_dependents(c: &Connection, changed_module: &str) -> rusqlite::Result<()> {
    let definitions=c.prepare("SELECT f.module_uid,COALESCE(f.module_key,f.field_key),f.label,f.field_type,f.config_json,m.name FROM mx_fields f JOIN mx_modules m ON m.uid=f.module_uid WHERE f.active=1 AND m.active=1 AND f.field_type IN ('relationship','lookup','rollup')")?
        .query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (module, key, label, kind, raw, module_name) in definitions {
        let config: Value = serde_json::from_str(&raw)
            .map_err(|e| invalid(format!("MX_CONFIG:{module_name} → {label}: {e}")))?;
        if module == changed_module
            || config["source_module_uid"] == changed_module
            || config["target_module_uid"] == changed_module
        {
            validate_config(c,&module,&key,&kind,&config).map_err(|e|invalid(format!("MX_CONFIG:{module_name} → {label}: reconfigure this dependency before changing its source. {e}")))?;
        }
    }
    Ok(())
}

pub(crate) fn validate_config(
    c: &Connection,
    module: &str,
    field_key: &str,
    kind: &str,
    config: &Value,
) -> rusqlite::Result<()> {
    if kind == "rollup" && config["direction"] == "incoming" {
        let source = config["source_module_uid"].as_str().unwrap_or("");
        if !c.query_row(
            "SELECT EXISTS(SELECT 1 FROM mx_modules WHERE uid=?1 AND active=1)",
            [source],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(invalid(
                "MX_CONFIG:The incoming aggregate's source module is unavailable. Choose an active source module.",
            ));
        }
        let source_fields = load_module_fields_initialized_db(c, source, false)?;
        let relation=source_fields.iter().find(|f|f.field_type=="relationship"&&f.key==config["relationship_field"].as_str().unwrap_or("")&&f.config["target_module_uid"]==module).ok_or_else(||invalid("MX_CONFIG:Choose a relationship in the source module that links back to this module."))?;
        if config["function"] != "count" {
            let target = source_fields
                .iter()
                .find(|f| f.key == config["target_field"].as_str().unwrap_or(""))
                .ok_or_else(|| invalid("MX_CONFIG:Choose an active numeric source field."))?;
            if !super::calculated::numeric(target)
                || super::calculated::live(target, &source_fields)
                || target.field_type == "select"
            {
                return Err(invalid(
                    "MX_CONFIG:Aggregate a saved numeric field or a numeric formula using local values; live linked calculations cannot be aggregated recursively.",
                ));
            }
        }
        if !config["filter"].is_null() {
            super::form_rules::validate_filter(&config["filter"], &source_fields)
                .map_err(|e| invalid(format!("MX_CONFIG:{e}")))?;
        }
        let _ = relation;
        return Ok(());
    }
    if kind == "relationship" {
        let target = config["target_module_uid"].as_str().unwrap_or("");
        let valid: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM mx_modules WHERE uid=?1 AND active=1)",
            [target],
            |r| r.get(0),
        )?;
        if !valid {
            return Err(invalid(
                "MX_CONFIG:The selected linked module is unavailable or archived. Choose an active module in ‘Linked module’.",
            ));
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
                "MX_CONFIG:The selected display field is unavailable. Choose an active display field in ‘Display field’ from the linked module.",
            ));
        }
        let local = load_module_fields_initialized_db(c, module, false)?;
        for mapping in config["auto_fill"].as_array().into_iter().flatten() {
            let source = fields
                .iter()
                .find(|f| Some(f.key.as_str()) == mapping["source_field"].as_str())
                .ok_or_else(|| invalid("MX_CONFIG:Choose an active linked source field."))?;
            let destination = local
                .iter()
                .find(|f| Some(f.key.as_str()) == mapping["target_field"].as_str())
                .ok_or_else(|| {
                    invalid("MX_CONFIG:Choose an existing active field to fill in this module.")
                })?;
            let source_kind =
                if source.field_type == "formula" && super::calculated::text(&source.config) {
                    "text"
                } else {
                    &source.field_type
                };
            if !compatible(source_kind, &destination.field_type) {
                return Err(invalid(
                    "MX_CONFIG:Auto-fill fields must have compatible types. Calculated fields, links and attachments cannot be filled.",
                ));
            }
            if local.iter().any(|f| {
                f.field_type == "relationship"
                    && f.key != field_key
                    && f.config["auto_fill"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|m| m["target_field"] == mapping["target_field"])
            }) {
                return Err(invalid(
                    "MX_CONFIG:This destination is already filled by another relationship.",
                ));
            }
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

fn compatible(source: &str, target: &str) -> bool {
    match target {
        "text" | "long_text" | "select" => matches!(source, "text" | "long_text" | "select"),
        "integer" => matches!(source, "integer" | "auto_number"),
        "decimal" => matches!(source, "integer" | "auto_number" | "decimal" | "formula"),
        "date" => source == "date",
        "boolean" => source == "boolean",
        _ => false,
    }
}

// Copies are ordinary record values. No edit sessions or extra version store.
fn mapped_values(
    c: &Connection,
    fields: &[FieldDefinition],
    field: &FieldDefinition,
    value: &Value,
    explicit: &BTreeMap<String, Value>,
    actor: &str,
) -> rusqlite::Result<BTreeMap<String, Value>> {
    let mappings = field.config["auto_fill"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if mappings.is_empty() {
        return Ok(BTreeMap::new());
    }
    validate_config(
        c,
        &c.query_row(
            "SELECT module_uid FROM mx_fields WHERE uid=?1",
            [&field.uid],
            |r| r.get::<_, String>(0),
        )?,
        &field.key,
        "relationship",
        &field.config,
    )?;
    let linked = ids(value);
    let source_values = if let Some(id) = linked.first() {
        let target = field.config["target_module_uid"].as_str().unwrap_or("");
        let source_fields = load_module_fields_initialized_db(c, target, false)?;
        let level = super::privacy::level(c, actor)?;
        let hidden = super::privacy::restricted(
            &source_fields,
            super::privacy::creator(c, id)?.as_deref() == Some(actor),
            level,
        );
        if mappings.iter().any(|mapping| {
            mapping["source_field"]
                .as_str()
                .is_some_and(|key| hidden.contains(key))
        }) {
            return Err(invalid(
                "Access to an automatic source value is restricted. No values were copied.",
            ));
        }
        let mut values = super::records::load_record_values(c, std::slice::from_ref(id))?
            .remove(id)
            .unwrap_or_default();
        super::records::calculate_formula_json_values(&source_fields, &mut values)
            .map_err(invalid)?;
        values
    } else {
        BTreeMap::new()
    };
    let mut result = BTreeMap::new();
    for mapping in mappings {
        let key = mapping["target_field"].as_str().unwrap_or("");
        if explicit.contains_key(key) {
            continue;
        }
        let destination = fields
            .iter()
            .find(|f| f.key == key)
            .ok_or_else(|| invalid("Refresh this module's auto-fill configuration."))?;
        let value = source_values
            .get(mapping["source_field"].as_str().unwrap_or(""))
            .cloned()
            .unwrap_or(Value::Null);
        super::records::validate_autofill_value(destination, &value).map_err(invalid)?;
        result.insert(key.to_owned(), value);
    }
    Ok(result)
}

pub(crate) fn fill_missing(
    c: &Connection,
    fields: &[FieldDefinition],
    changes: &mut BTreeMap<String, Value>,
    actor: &str,
) -> rusqlite::Result<()> {
    validate_selection(c, fields, changes, actor)?;
    let mut fills = BTreeMap::new();
    for field in fields.iter().filter(|f| f.field_type == "relationship") {
        if let Some(value) = changes.get(&field.key) {
            // Explicitly supplied destination values are intentional overrides.
            if field.config["auto_fill"]
                .as_array()
                .into_iter()
                .flatten()
                .all(|m| changes.contains_key(m["target_field"].as_str().unwrap_or("")))
            {
                continue;
            }
            fills.extend(mapped_values(c, fields, field, value, changes, actor)?);
        }
    }
    for (key, value) in fills {
        changes.entry(key).or_insert(value);
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
    if crate::db::schema::initialized(c) {
        return Ok(());
    }
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
            if !exists || !super::privacy::record_allowed(c, &id, actor, level)? {
                return Err(invalid(
                    "The selected related record is unavailable. Choose another record.",
                ));
            }
        }
    }
    Ok(())
}

fn scalar(c: &Connection, id: &str, key: &str) -> rusqlite::Result<Value> {
    c.query_row("SELECT f.field_type,v.value_text,v.value_integer,v.value_real,v.value_boolean,f.config_json FROM mx_record_values v JOIN mx_fields f ON f.uid=v.field_uid WHERE v.record_uid=?1 AND COALESCE(f.module_key,f.field_key)=?2 AND f.active=1",params![id,key],|r|{
        let kind:String=r.get(0)?;
        let config:Value=serde_json::from_str(&r.get::<_,String>(5)?).unwrap_or(Value::Null);
        Ok(match kind.as_str(){"integer"|"auto_number"=>r.get::<_,Option<i64>>(2)?.map_or(Value::Null,|v|json!(v)),"formula" if super::calculated::text(&config)=>r.get::<_,Option<String>>(1)?.map_or(Value::Null,Value::String),"decimal"|"formula"=>r.get::<_,Option<f64>>(3)?.map_or(Value::Null,|v|json!(v)),"boolean"=>r.get::<_,Option<i64>>(4)?.map_or(Value::Null,|v|json!(v!=0)),_=>r.get::<_,Option<String>>(1)?.map_or(Value::Null,Value::String)})
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
        let mut items = Vec::new();
        let mut visible_ids = Vec::new();
        let source_fields = load_module_fields_initialized_db(c, target, false)?;
        let label_key = field.config["label_field"].as_str().unwrap_or("");
        let label_field = source_fields.iter().find(|f| f.key == label_key);
        for uid in ids(&record["values"][&field.key]) {
            if !super::privacy::record_allowed(c, &uid, &claims.uid, claims.access_level)? {
                continue;
            }
            let label_allowed = label_field.is_some_and(|f| {
                super::privacy::field_allowed(
                    f,
                    super::privacy::creator(c, &uid).ok().flatten().as_deref() == Some(&claims.uid),
                    claims.access_level,
                )
            });
            items.push(json!({"uid":uid,"label":if label_allowed{label(c,&uid,label_key)?}else{"Restricted field".into()}}));
            visible_ids.push(uid);
        }
        let restricted = visible_ids.len() != ids(&record["values"][&field.key]).len();
        if restricted {
            record["values"][&field.key] = if field.config["multiple"] == true {
                json!(visible_ids)
            } else {
                Value::Null
            };
            allowed.insert(field.key.clone(), false);
        }
        labels[&field.key] = json!({"restricted":restricted,"items":items});
    }
    for field in fields.iter().filter(|f| computed(&f.field_type)) {
        if field.field_type == "rollup" && field.config["direction"] == "incoming" {
            record["values"][&field.key] =
                incoming(c, claims, record["uid"].as_str().unwrap_or(""), field)?;
            continue;
        }
        let relation = field.config["relationship_field"].as_str().unwrap_or("");
        if !allowed.get(relation).copied().unwrap_or(false) {
            record["values"][&field.key] = Value::Null;
            continue;
        }
        let linked = ids(&record["values"][relation]);
        let target = fields
            .iter()
            .find(|f| f.key == relation)
            .and_then(|f| f.config["target_module_uid"].as_str())
            .unwrap_or("");
        let target_fields = load_module_fields_initialized_db(c, target, false)?;
        let source = target_fields
            .iter()
            .find(|f| f.key == field.config["target_field"].as_str().unwrap_or(""));
        let mut restricted = false;
        for uid in &linked {
            if source.is_some_and(|f| {
                !super::privacy::field_allowed(
                    f,
                    super::privacy::creator(c, uid).ok().flatten().as_deref() == Some(&claims.uid),
                    claims.access_level,
                )
            }) {
                restricted = true;
            }
        }
        if restricted {
            record["values"][&field.key] = Value::Null;
            continue;
        }
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
    let mut values: BTreeMap<String, Value> =
        serde_json::from_value(record["values"].clone()).unwrap_or_default();
    super::calculated::evaluate(fields, &mut values, true).map_err(invalid)?;
    record["values"] = json!(values);
    Ok(())
}

fn incoming(
    c: &Connection,
    claims: &Claims,
    uid: &str,
    field: &FieldDefinition,
) -> rusqlite::Result<Value> {
    if uid.is_empty() {
        return Ok(Value::Null);
    }
    let source = field.config["source_module_uid"].as_str().unwrap_or("");
    if !module_can_for_user_initialized(c, source, &claims.uid, claims.access_level, "read")? {
        return Ok(Value::Null);
    }
    let fields = load_module_fields_initialized_db(c, source, false)?;
    let relationship=fields.iter().find(|f|f.key==field.config["relationship_field"].as_str().unwrap_or("")).ok_or_else(||invalid("The aggregate relationship is unavailable. Ask an administrator to reconfigure this field."))?;
    let target = fields
        .iter()
        .find(|f| f.key == field.config["target_field"].as_str().unwrap_or(""));
    let method = field.config["function"].as_str().unwrap_or("sum");
    let row_scope = super::privacy::record_sql("r", &claims.uid, claims.access_level);
    let relation_scope = super::privacy::scope_sql(
        relationship.config["access_scope"]
            .as_str()
            .unwrap_or("module"),
        "r",
        &claims.uid,
        claims.access_level,
    );
    let field_scope = target
        .map(|f| {
            super::privacy::scope_sql(
                f.config["access_scope"].as_str().unwrap_or("module"),
                "r",
                &claims.uid,
                claims.access_level,
            )
        })
        .unwrap_or_else(|| "1".into());
    let filter = if field.config["filter"].is_null() {
        None
    } else {
        Some(
            super::form_rules::validate_filter(&field.config["filter"], &fields)
                .map_err(invalid)?,
        )
    };
    let mut bind = vec![
        rusqlite::types::Value::Text(uid.to_owned()),
        rusqlite::types::Value::Text(relationship.uid.clone()),
        rusqlite::types::Value::Text(source.to_owned()),
    ];
    let filter_sql = filter
        .as_ref()
        .map(|g| filter_sql(g, &fields, &mut bind, &claims.uid, claims.access_level))
        .transpose()?
        .unwrap_or_else(|| "1".into());
    let expression=target.map(|f|format!("(SELECT COALESCE(v.value_real,v.value_integer) FROM mx_record_values v WHERE v.record_uid=r.uid AND v.field_uid='{}')",f.uid.replace('\'',"''"))).unwrap_or_else(||"1".into());
    let calculation = match method {
        "count" => "COUNT(*)".into(),
        "sum" => format!("SUM({expression})"),
        "average" => format!("AVG({expression})"),
        "min" => format!("MIN({expression})"),
        "max" => format!("MAX({expression})"),
        _ => return Err(invalid("Invalid aggregate calculation.")),
    };
    let sql = format!(
        "SELECT {calculation} FROM mx_record_links l JOIN mx_records r ON r.uid=l.record_uid WHERE l.target_uid=?1 AND l.field_uid=?2 AND r.module_uid=?3 AND r.deleted_at IS NULL AND {row_scope} AND {relation_scope} AND {field_scope} AND {filter_sql}"
    );
    let value: Option<f64> =
        c.query_row(&sql, rusqlite::params_from_iter(bind.iter()), |r| r.get(0))?;
    if value.is_some_and(|n| !n.is_finite()) {
        return Err(invalid("Aggregate result is not finite."));
    }
    Ok(value.map_or_else(
        || {
            if matches!(method, "count" | "sum") {
                json!(0)
            } else {
                Value::Null
            }
        },
        |v| json!(v),
    ))
}
fn filter_sql(
    group: &super::form_rules::Group,
    fields: &[FieldDefinition],
    bind: &mut Vec<rusqlite::types::Value>,
    actor: &str,
    level: i64,
) -> rusqlite::Result<String> {
    let mut terms = Vec::new();
    for condition in &group.conditions {
        let field = fields
            .iter()
            .find(|f| f.key == condition.field)
            .ok_or_else(|| invalid("Filter field is unavailable."))?;
        let column = match field.field_type.as_str() {
            "integer" | "auto_number" => "value_integer",
            "decimal" => "value_real",
            "boolean" => "value_boolean",
            _ => "value_text COLLATE NOCASE",
        };
        let expr = format!(
            "(SELECT {column} FROM mx_record_values WHERE record_uid=r.uid AND field_uid='{}')",
            field.uid.replace('\'', "''")
        );
        let values = if matches!(condition.operator.as_str(), "empty" | "not_empty") {
            vec![]
        } else if matches!(condition.operator.as_str(), "in" | "not_in") {
            condition.value.as_array().unwrap().clone()
        } else {
            vec![condition.value.clone()]
        };
        let scope = super::privacy::scope_sql(
            field.config["access_scope"].as_str().unwrap_or("module"),
            "r",
            actor,
            level,
        );
        let mut slots = Vec::new();
        for value in values {
            bind.push(match value {
                Value::String(s) => rusqlite::types::Value::Text(s),
                Value::Number(n) => n
                    .as_i64()
                    .map(rusqlite::types::Value::Integer)
                    .unwrap_or(rusqlite::types::Value::Real(n.as_f64().unwrap())),
                Value::Bool(v) => rusqlite::types::Value::Integer(v as i64),
                _ => rusqlite::types::Value::Null,
            });
            slots.push(format!("?{}", bind.len()));
        }
        let test = match condition.operator.as_str() {
            "empty" => format!("({expr} IS NULL OR {expr}='')"),
            "not_empty" => format!("({expr} IS NOT NULL AND {expr}<>'')"),
            "in" => format!("{expr} IN ({})", slots.join(",")),
            "not_in" => format!("({expr} IS NULL OR {expr} NOT IN ({}))", slots.join(",")),
            op => {
                let comparison = match op {
                    "eq" => "IS",
                    "neq" => "IS NOT",
                    "gt" => ">",
                    "gte" => ">=",
                    "lt" => "<",
                    "lte" => "<=",
                    _ => return Err(invalid("Unsupported aggregate filter comparison.")),
                };
                format!("{expr} {comparison} {}", slots[0])
            }
        };
        terms.push(format!("({scope} AND {test})"));
    }
    for child in &group.groups {
        terms.push(filter_sql(child, fields, bind, actor, level)?);
    }
    Ok(format!(
        "({})",
        terms.join(if group.r#match == "any" {
            " OR "
        } else {
            " AND "
        })
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_relationship_settings_name_the_operator_controls() {
        assert!(
            config("relationship", &json!({}))
                .unwrap_err()
                .contains("Linked module")
        );
        assert!(
            config("relationship", &json!({"target_module_uid":"employees"}))
                .unwrap_err()
                .contains("Display field")
        );
        assert!(
            config("lookup", &json!({}))
                .unwrap_err()
                .contains("Relationship")
        );
    }
    #[test]
    fn auto_fill_configuration_requires_unambiguous_single_record_mappings() {
        let base = json!({"target_module_uid":"employees","label_field":"name"});
        assert_eq!(
            config("relationship", &base).unwrap()["auto_fill"],
            json!([])
        );
        let mut config_value = base;
        config_value["auto_fill"] =
            json!([{"source_field":"class","target_field":"classification"}]);
        assert_eq!(
            config("relationship", &config_value).unwrap()["auto_fill"],
            config_value["auto_fill"]
        );
        config_value["multiple"] = json!(true);
        assert!(config("relationship", &config_value).is_err());
        config_value["multiple"] = json!(false);
        config_value["auto_fill"] = json!([{"source_field":"a","target_field":"b"},{"source_field":"c","target_field":"b"}]);
        assert!(config("relationship", &config_value).is_err());
        config_value["auto_fill"] = json!([{"source_field":"a"}]);
        assert!(config("relationship", &config_value).is_err());
        config_value["auto_fill"] = json!({});
        assert!(config("relationship", &config_value).is_err());
    }
    #[test]
    fn auto_fill_type_compatibility_does_not_allow_calculated_or_lossy_destinations() {
        assert!(compatible("select", "text"));
        assert!(compatible("text", "select"));
        assert!(compatible("auto_number", "integer"));
        assert!(compatible("formula", "decimal"));
        assert!(compatible("boolean", "boolean"));
        assert!(!compatible("decimal", "integer"));
        assert!(!compatible("text", "boolean"));
        for kind in [
            "relationship",
            "lookup",
            "rollup",
            "attachments",
            "formula",
            "auto_number",
        ] {
            assert!(!compatible("text", kind));
        }
    }
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
        .route("/mx/v1/relationships/{field}/preview", post(preview))
        .route(
            "/mx/v1/relationships/{module}/{record}/related",
            get(related),
        )
        .layer(axum::middleware::from_fn(auth))
}
#[derive(Deserialize)]
struct PreviewRequest {
    value: Value,
}

async fn preview(
    claims: Claims,
    Path(uid): Path<String>,
    Json(request): Json<PreviewRequest>,
) -> Response {
    let result = tokio::task::spawn_blocking(move || with_sql_connection(|c| {
        let tx = c.unchecked_transaction()?;
        let module: String = tx.query_row("SELECT module_uid FROM mx_fields WHERE uid=?1 AND active=1 AND field_type='relationship'", [&uid], |r| r.get(0))?;
        if !module_can_for_user_initialized(&tx, &module, &claims.uid, claims.access_level, "read")? { return Err(invalid("Access denied.")); }
        let fields = load_module_fields_initialized_db(&tx, &module, false)?;
        let field = fields.iter().find(|f| f.uid == uid).ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        normalize(field, &request.value).map_err(invalid)?;
        let target = field.config["target_module_uid"].as_str().unwrap_or("");
        if !ids(&request.value).is_empty() && !module_can_for_user_initialized(&tx, target, &claims.uid, claims.access_level, "read")? { return Err(invalid("Access to linked records is restricted.")); }
        let selected = BTreeMap::from([(field.key.clone(), request.value.clone())]);
        validate_selection(&tx, &fields, &selected, &claims.uid)?;
            let fills = mapped_values(&tx, &fields, field, &request.value, &BTreeMap::new(),&claims.uid)?;
        let mut record = json!({"values":selected});
        present_fields(&tx, &claims, &mut record, &fields, &mut BTreeMap::new())?;
        let computed = fields.iter().filter(|f| computed(&f.field_type) && f.config["relationship_field"] == field.key)
            .map(|f| (f.key.clone(), record["values"][&f.key].clone())).collect::<BTreeMap<_,_>>();
        tx.commit()?;
        Ok(json!({"values":fills,"computed":computed}))
    })).await;
    match result {
        Ok(result) => response(result),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"response":"Relationship preview failed. Retry the selection."})),
        )
            .into_response(),
    }
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
        super::privacy::ensure(c)?;
        let row_scope=super::privacy::record_sql("r",&claims.uid,claims.access_level);
        let field_scope=super::privacy::field_sql("f","r",&claims.uid,claims.access_level);
        let pattern=format!("%{}%",query.q.unwrap_or_default().chars().take(150).collect::<String>().replace('\\',"\\\\").replace('%',"\\%").replace('_',"\\_"));
        let page=query.page.unwrap_or(1).clamp(1,1_000_000);let key=field.config["label_field"].as_str().unwrap_or("");
        let mut statement=c.prepare(&format!("SELECT r.uid FROM mx_records r LEFT JOIN mx_fields f ON f.module_uid=r.module_uid AND f.active=1 AND COALESCE(f.module_key,f.field_key)=?2 LEFT JOIN mx_record_values v ON v.record_uid=r.uid AND v.field_uid=f.uid WHERE r.module_uid=?1 AND r.deleted_at IS NULL AND {row_scope} AND {field_scope} AND COALESCE(v.value_text,CAST(v.value_integer AS TEXT),CAST(v.value_real AS TEXT),CASE v.value_boolean WHEN 1 THEN 'true' WHEN 0 THEN 'false' END,'Unnamed record') LIKE ?3 ESCAPE '\\' ORDER BY COALESCE(v.value_text,CAST(v.value_integer AS TEXT),CAST(v.value_real AS TEXT),CASE v.value_boolean WHEN 1 THEN 'true' WHEN 0 THEN 'false' END,'Unnamed record') COLLATE NOCASE,r.uid LIMIT 51 OFFSET ?4"))?;
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
        if !super::privacy::record_allowed(c,&uid,&claims.uid,claims.access_level)?{return Err(rusqlite::Error::QueryReturnedNoRows);}
        let exists:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM mx_records WHERE uid=?1 AND module_uid=?2 AND deleted_at IS NULL)",params![uid,module],|r|r.get(0))?;
        if !exists{return Err(rusqlite::Error::QueryReturnedNoRows);}
        // Filter allowed modules in SQL BEFORE pagination, never leak hidden counts.
        let modules=c.prepare("SELECT uid FROM mx_modules WHERE active=1")?.query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let allowed=modules.into_iter().filter_map(|m|match module_can_for_user_initialized(c,&m,&claims.uid,claims.access_level,"read"){Ok(true)=>Some(Ok(m)),Ok(false)=>None,Err(e)=>Some(Err(e))}).collect::<rusqlite::Result<Vec<_>>>()?;
        let page=query.page.unwrap_or(1).clamp(1,1_000_000);
        let row_scope=super::privacy::record_sql("r",&claims.uid,claims.access_level);
        let field_scope=super::privacy::field_sql("f","r",&claims.uid,claims.access_level);
        let mut statement=c.prepare(&format!("SELECT r.uid,r.module_uid,m.name,f.label FROM mx_record_links l JOIN mx_records r ON r.uid=l.record_uid JOIN mx_modules m ON m.uid=r.module_uid JOIN mx_fields f ON f.uid=l.field_uid WHERE l.target_uid=?1 AND r.deleted_at IS NULL AND f.active=1 AND {row_scope} AND {field_scope} AND r.module_uid IN (SELECT value FROM json_each(?2)) ORDER BY m.name,r.uid,f.uid LIMIT 51 OFFSET ?3"))?;
        let rows=statement.query_map(params![uid,json!(allowed).to_string(),((page-1)*50) as i64],|r|Ok(json!({"uid":r.get::<_,String>(0)?,"module_uid":r.get::<_,String>(1)?,"module_name":r.get::<_,String>(2)?,"relationship_label":r.get::<_,String>(3)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let more=rows.len()>50;
        let mut display_fields=BTreeMap::new();let mut items=Vec::new();
        for mut row in rows.into_iter().take(50) {
            let source=row["module_uid"].as_str().unwrap_or("");
            if !display_fields.contains_key(source) {
                display_fields.insert(source.to_owned(),load_module_fields_initialized_db(c,source,false)?);
            }
            let record=row["uid"].as_str().unwrap_or("");
            let is_creator=super::privacy::creator(c,record)?.as_deref()==Some(&claims.uid);
            let key=display_fields[source].iter().find(|f|!matches!(f.field_type.as_str(),"attachments"|"relationship"|"lookup"|"rollup")&&super::privacy::field_allowed(f,is_creator,claims.access_level)).map(|f|f.key.as_str()).unwrap_or("");
            row["label"]=json!(if key.is_empty(){"Record".to_owned()}else{label(c,record,key)?});
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
