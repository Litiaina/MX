//! Server-enforced opt-in record/field isolation. UI visibility is not security.
use super::schema::FieldDefinition;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn ensure(c: &Connection) -> rusqlite::Result<()> {
    if crate::db::schema::initialized(c) {
        return Ok(());
    }
    c.execute_batch("CREATE TABLE IF NOT EXISTS mx_record_owners(
        record_uid TEXT PRIMARY KEY NOT NULL REFERENCES mx_records(uid) ON DELETE CASCADE,
        creator_uid TEXT NOT NULL, provenance TEXT NOT NULL);
        CREATE INDEX IF NOT EXISTS mx_record_owners_creator ON mx_record_owners(creator_uid,record_uid);")?;
    // Only creation evidence establishes historical ownership. A last editor is
    // not the creator. Unknown legacy owners remain administrator-only when an
    // administrator opts into creator isolation; nothing is silently reassigned.
    let has = |table: &str| {
        c.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
            [table],
            |r| r.get::<_, bool>(0),
        )
    };
    if has("mx_record_versions")? {
        c.execute_batch("INSERT OR IGNORE INTO mx_record_owners(record_uid,creator_uid,provenance)
          SELECT r.uid,(SELECT v.actor_uid FROM mx_record_versions v WHERE v.record_uid=r.uid AND v.event='created' ORDER BY v.version_no,v.uid LIMIT 1),'creation-history'
          FROM mx_records r WHERE NOT EXISTS(SELECT 1 FROM mx_record_owners o WHERE o.record_uid=r.uid)
          AND EXISTS(SELECT 1 FROM mx_record_versions v WHERE v.record_uid=r.uid AND v.event='created');")?;
    }
    if has("mx_import_rows")? && has("mx_import_jobs")? {
        c.execute_batch(
            "INSERT OR IGNORE INTO mx_record_owners(record_uid,creator_uid,provenance)
          SELECT ir.record_uid,j.actor_uid,'import-history' FROM mx_import_rows ir
          JOIN mx_import_jobs j ON j.uid=ir.job_uid JOIN mx_records r ON r.uid=ir.record_uid
          WHERE ir.state='imported' AND ir.record_uid IS NOT NULL;",
        )?;
    }
    Ok(())
}
fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
pub(crate) fn record_sql(alias: &str, actor: &str, level: i64) -> String {
    if level == 0 {
        return "1".into();
    }
    format!(
        "({alias}.module_uid IN (SELECT pm.uid FROM mx_modules pm WHERE COALESCE(json_extract(pm.config_json,'$.record_visibility'),'module')='module') OR {alias}.uid IN (SELECT po.record_uid FROM mx_record_owners po WHERE po.creator_uid={}))",
        literal(actor)
    )
}
pub(crate) fn field_sql(field_alias: &str, record_alias: &str, actor: &str, level: i64) -> String {
    if level == 0 {
        return "1".into();
    }
    format!(
        "(COALESCE(json_extract({field_alias}.config_json,'$.access_scope'),'module')='module' OR (json_extract({field_alias}.config_json,'$.access_scope')='creator' AND {record_alias}.uid IN (SELECT pfo.record_uid FROM mx_record_owners pfo WHERE pfo.creator_uid={})))",
        literal(actor)
    )
}
pub(crate) fn scope_sql(scope: &str, record: &str, actor: &str, level: i64) -> String {
    if level == 0 || scope == "module" {
        return "1".into();
    }
    if scope == "creator" {
        return format!(
            "{record}.uid IN (SELECT pso.record_uid FROM mx_record_owners pso WHERE pso.creator_uid={})",
            literal(actor)
        );
    }
    "0".into()
}
pub(crate) fn attachment_sql(alias: &str, record_alias: &str, actor: &str, level: i64) -> String {
    if level == 0 {
        return "1".into();
    }
    format!(
        "({alias}.attachment_field_uid IS NULL OR EXISTS(SELECT 1 FROM mx_fields paf WHERE paf.uid={alias}.attachment_field_uid AND {}))",
        field_sql("paf", record_alias, actor, level)
    )
}
pub(crate) fn creator(c: &Connection, record: &str) -> rusqlite::Result<Option<String>> {
    ensure(c)?;
    c.query_row(
        "SELECT creator_uid FROM mx_record_owners WHERE record_uid=?1",
        [record],
        |r| r.get(0),
    )
    .optional()
}
pub(crate) fn record_allowed(
    c: &Connection,
    record: &str,
    actor: &str,
    level: i64,
) -> rusqlite::Result<bool> {
    ensure(c)?;
    c.query_row(
        &format!(
            "SELECT EXISTS(SELECT 1 FROM mx_records pr WHERE pr.uid=?1 AND {})",
            record_sql("pr", actor, level)
        ),
        [record],
        |r| r.get(0),
    )
}
pub(crate) fn check_record(c: &Connection, record: &str, actor: &str) -> rusqlite::Result<()> {
    let level = level(c, actor)?;
    if record_allowed(c, record, actor, level)? {
        Ok(())
    } else {
        Err(rusqlite::Error::QueryReturnedNoRows)
    }
}
pub(crate) fn level(c: &Connection, actor: &str) -> rusqlite::Result<i64> {
    c.query_row(
        "SELECT access_level FROM users WHERE uid=?1",
        [actor],
        |r| r.get(0),
    )
}
pub(crate) fn field_allowed(field: &FieldDefinition, is_creator: bool, level: i64) -> bool {
    level == 0
        || match field.config["access_scope"].as_str().unwrap_or("module") {
            "module" => true,
            "creator" => is_creator,
            _ => false,
        }
}
pub(crate) fn normalize(raw: &Value, mut clean: Value) -> Result<Value, String> {
    if let Some(scope) = raw.get("access_scope") {
        if !matches!(
            scope.as_str(),
            Some("module" | "creator" | "administrators")
        ) {
            return Err(
                "Choose field access: module members, record creator, or administrators.".into(),
            );
        }
        clean["access_scope"] = scope.clone();
    }
    Ok(clean)
}
pub(crate) fn validate_fields(fields: &[FieldDefinition]) -> Result<(), String> {
    let rank =
        |field: &FieldDefinition| match field.config["access_scope"].as_str().unwrap_or("module") {
            "module" => 0,
            "creator" => 1,
            _ => 2,
        };
    for field in fields.iter().filter(|f| f.field_type == "formula") {
        for key in super::calculated::references(&field.config) {
            if let Some(source) = fields.iter().find(|f| f.key == key) {
                if rank(source) > rank(field) {
                    return Err(format!(
                        "{} uses restricted field {}. Give the calculated field equally restrictive access before protecting its source.",
                        field.label, source.label
                    ));
                }
            }
        }
    }
    for field in fields {
        for (key, usage) in super::form_rules::references(&field.config) {
            let Some(other) = fields.iter().find(|f| f.key == key) else {
                continue;
            };
            if rank(field) != rank(other)
                && (usage.starts_with("automatic values")
                    || super::form_rules::writable(&field.field_type))
            {
                return Err(format!(
                    "{} and {} need the same field access for {}. Conditional clearing and automatic copies must not change fields another editor cannot access.",
                    field.label, other.label, usage
                ));
            }
            let (source, destination) = if usage.starts_with("automatic values") {
                (field, other)
            } else {
                (other, field)
            };
            if rank(source) > rank(destination) {
                return Err(format!(
                    "{} depends on restricted field {} through {}. Give dependent fields equally restrictive access, or remove that rule.",
                    destination.label, source.label, usage
                ));
            }
        }
        if let Some(group) = super::form_rules::rules(field).exclusive_group {
            if fields.iter().any(|other| {
                super::form_rules::rules(other).exclusive_group.as_ref() == Some(&group)
                    && rank(other) != rank(field)
            }) {
                return Err(format!(
                    "{}: mutually exclusive fields in {} need the same access scope.",
                    field.label, group
                ));
            }
        }
    }
    Ok(())
}
pub(crate) fn restricted(
    fields: &[FieldDefinition],
    is_creator: bool,
    level: i64,
) -> BTreeSet<String> {
    let mut hidden = fields
        .iter()
        .filter(|f| !field_allowed(f, is_creator, level))
        .map(|f| f.key.clone())
        .collect::<BTreeSet<_>>();
    // A public formula must not disclose a private input through its result.
    loop {
        let previous = hidden.len();
        for field in fields.iter().filter(|f| f.field_type == "formula") {
            if super::calculated::references(&field.config)
                .iter()
                .any(|key| hidden.contains(key))
            {
                hidden.insert(field.key.clone());
            }
        }
        if hidden.len() == previous {
            return hidden;
        }
    }
}
pub(crate) fn check_values(
    c: &Connection,
    record: Option<&str>,
    fields: &[FieldDefinition],
    values: &BTreeMap<String, Value>,
    actor: &str,
) -> rusqlite::Result<()> {
    let access = level(c, actor)?;
    let is_creator = match record {
        Some(uid) => creator(c, uid)?.as_deref() == Some(actor),
        None => true,
    };
    if let Some(field) = fields
        .iter()
        .find(|f| values.contains_key(&f.key) && !field_allowed(f, is_creator, access))
    {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "MX_FORBIDDEN:You cannot change the restricted field ‘{}’. Your other changes were not saved.",
            field.label
        )));
    }
    Ok(())
}
pub(crate) fn redact(
    c: &Connection,
    actor: &str,
    level: i64,
    record: &mut Value,
) -> rusqlite::Result<()> {
    let uid = record
        .get("uid")
        .or_else(|| record.get("record_uid"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if uid.is_empty() {
        return Ok(());
    }
    if !record_allowed(c, &uid, actor, level)? {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    let module: String = c.query_row(
        "SELECT module_uid FROM mx_records WHERE uid=?1",
        [&uid],
        |r| r.get(0),
    )?;
    let fields = super::schema::load_module_fields_initialized_db(c, &module, true)?;
    let owner = creator(c, &uid)?;
    let hidden = restricted(&fields, owner.as_deref() == Some(actor), level);
    if let Some(values) = record.get_mut("values").and_then(Value::as_object_mut) {
        values.retain(|key, _| !hidden.contains(key));
    }
    if let Some(labels) = record
        .get_mut("relationships")
        .and_then(Value::as_object_mut)
    {
        labels.retain(|key, _| !hidden.contains(key));
    }
    for name in ["attached_files", "attachments"] {
        if let Some(files) = record.get_mut(name).and_then(Value::as_array_mut) {
            files.retain(|file| {
                file.get("attachment_field_uid")
                    .or_else(|| file.get("field_uid"))
                    .and_then(Value::as_str)
                    .is_none_or(|field_uid| {
                        !fields
                            .iter()
                            .any(|f| f.uid == field_uid && hidden.contains(&f.key))
                    })
            });
        }
    }
    record["restricted_fields"] = json!(hidden);
    record["creator_uid"] = json!(owner);
    Ok(())
}
pub(crate) fn field_record_allowed(
    c: &Connection,
    record: &str,
    field_uid: &str,
    actor: &str,
    level: i64,
) -> rusqlite::Result<bool> {
    if !record_allowed(c, record, actor, level)? {
        return Ok(false);
    }
    if field_uid.is_empty() {
        return Ok(true);
    }
    let config: Option<String> = c
        .query_row(
            "SELECT f.config_json FROM mx_fields f JOIN mx_records r ON r.module_uid=f.module_uid WHERE f.uid=?1 AND r.uid=?2",
            params![field_uid,record],
            |r| r.get(0),
        )
        .optional()?;
    let Some(config) = config else {
        return Ok(false);
    };
    let config: Value = serde_json::from_str(&config).unwrap_or(Value::Null);
    Ok(level == 0
        || config["access_scope"].as_str().unwrap_or("module") == "module"
        || (config["access_scope"] == "creator" && creator(c, record)?.as_deref() == Some(actor)))
}
pub(crate) fn claim(c: &Connection, record: &str, actor: &str) -> rusqlite::Result<()> {
    ensure(c)?;
    c.execute(
        "INSERT INTO mx_record_owners(record_uid,creator_uid,provenance) VALUES(?1,?2,'created')",
        params![record, actor],
    )?;
    Ok(())
}
pub(crate) fn waive_hidden_requirements(
    c: &Connection,
    fields: &mut [FieldDefinition],
    actor: &str,
) -> rusqlite::Result<()> {
    let level = level(c, actor)?;
    for field in fields {
        if !field_allowed(field, true, level) {
            field.required = false;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn database() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys=ON;
            CREATE TABLE mx_modules(uid TEXT PRIMARY KEY,config_json TEXT NOT NULL);
            CREATE TABLE mx_records(uid TEXT PRIMARY KEY,module_uid TEXT NOT NULL);
            CREATE TABLE mx_record_versions(uid TEXT PRIMARY KEY,record_uid TEXT,actor_uid TEXT,event TEXT,version_no INTEGER);
            CREATE TABLE mx_import_jobs(uid TEXT PRIMARY KEY,actor_uid TEXT);
            CREATE TABLE mx_import_rows(record_uid TEXT,job_uid TEXT,state TEXT);
            INSERT INTO mx_modules VALUES('private','{\"record_visibility\":\"creator\"}');
            INSERT INTO mx_records VALUES('created','private'),('imported','private'),('unknown','private');
            INSERT INTO mx_record_versions VALUES('v1','created','alice','created',1),('v2','created','bob','updated',2),('v3','unknown','bob','updated',1);
            INSERT INTO mx_import_jobs VALUES('job','importer');
            INSERT INTO mx_import_rows VALUES('imported','job','imported'),('unknown','job','attention');").unwrap();
        c
    }
    #[test]
    fn historical_ownership_never_uses_last_editor_or_failed_import() {
        let c = database();
        ensure(&c).unwrap();
        assert_eq!(creator(&c, "created").unwrap().as_deref(), Some("alice"));
        assert_eq!(
            creator(&c, "imported").unwrap().as_deref(),
            Some("importer")
        );
        assert_eq!(creator(&c, "unknown").unwrap(), None);
        assert!(!record_allowed(&c, "created", "bob", 2).unwrap());
        assert!(!record_allowed(&c, "unknown", "bob", 2).unwrap());
        assert!(record_allowed(&c, "unknown", "admin", 0).unwrap());
        c.execute(
            "UPDATE mx_record_versions SET actor_uid='other' WHERE uid='v1'",
            [],
        )
        .unwrap();
        ensure(&c).unwrap();
        assert_eq!(creator(&c, "created").unwrap().as_deref(), Some("alice"));
    }
    #[test]
    fn scope_sql_quotes_actor_identity_and_unknown_scopes_fail_closed() {
        let c = database();
        ensure(&c).unwrap();
        assert!(!record_allowed(&c, "created", "x' OR 1=1 --", 2).unwrap());
        assert_eq!(scope_sql("unrecognized", "r", "alice", 2), "0");
        c.execute("INSERT INTO mx_records VALUES('new','private')", [])
            .unwrap();
        claim(&c, "new", "x' OR 1=1 --").unwrap();
        assert!(record_allowed(&c, "new", "x' OR 1=1 --", 2).unwrap());
        assert!(!record_allowed(&c, "new", "alice", 2).unwrap());
    }
    fn field(key: &str, kind: &str, config: Value) -> FieldDefinition {
        serde_json::from_value(json!({"uid":key,"key":key,"label":key,"field_type":kind,"config":config,"required":false,"unique_value":false,"searchable":true,"sortable":true,"table_visible":true,"table_priority":50,"position":0,"active":true})).unwrap()
    }
    #[test]
    fn formulas_conditions_and_automatic_values_cannot_declassify_private_inputs() {
        let source = field("secret", "text", json!({"access_scope":"creator"}));
        let derived = field(
            "combined",
            "formula",
            json!({"result_type":"text","parts":["secret"]}),
        );
        assert!(
            validate_fields(&[source.clone(), derived.clone()])
                .unwrap_err()
                .contains("equally restrictive")
        );
        assert!(restricted(&[source.clone(), derived], false, 2).contains("combined"));
        let dependent = field(
            "detail",
            "text",
            json!({"form_rules":{"visible_when":{"match":"all","conditions":[{"field":"secret","operator":"not_empty"}]}}}),
        );
        assert!(validate_fields(&[source, dependent]).is_err());
        let selector = field(
            "choice",
            "select",
            json!({"access_scope":"creator","options":["SL"],"option_details":[{"value":"SL","fill":{"public":"secret"}}]}),
        );
        assert!(validate_fields(&[selector, field("public", "text", json!({}))]).is_err());
    }
}
