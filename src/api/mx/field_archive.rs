//! Explain every archive dependency using names, not opaque field IDs.
use std::collections::BTreeMap;

use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{Value, json};

use super::{
    schema::{FieldDefinition, ensure_dynamic_schema, load_module_fields_initialized_db},
    storage::ensure_storage_layout_schema,
};
use crate::{
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::Claims,
};

#[derive(Debug, Serialize)]
pub struct ArchiveBlocker {
    pub kind: String,
    pub module_uid: String,
    pub module_name: String,
    pub field_uid: Option<String>,
    pub field_label: Option<String>,
    pub field_key: Option<String>,
    pub field_type: Option<String>,
    pub reason: String,
    pub resolution: String,
    pub section: String,
    pub count: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ArchiveCheck {
    pub field: FieldDefinition,
    pub module_uid: String,
    pub module_name: String,
    pub can_archive: bool,
    pub blockers: Vec<ArchiveBlocker>,
}
impl ArchiveCheck {
    pub fn blocked_response(&self) -> Value {
        let names = self
            .blockers
            .iter()
            .map(|b| {
                format!(
                    "{} → {}",
                    b.module_name,
                    b.field_label.as_deref().unwrap_or(&b.section)
                )
            })
            .collect::<Vec<_>>();
        json!({"error":"field_archive_blocked","response":format!("Cannot archive “{}”. Still used by {}. Open the listed settings and remove those dependencies first.",self.field.label,names.join("; ")),"archive_check":self})
    }
    pub fn status(&self) -> StatusCode {
        if self.blockers.iter().any(|b| b.section == "field") {
            StatusCode::BAD_REQUEST
        } else {
            StatusCode::CONFLICT
        }
    }
}

struct Dependency {
    uid: String,
    module: String,
    module_name: String,
    key: String,
    label: String,
    kind: String,
    config: Value,
}

pub(crate) fn check(c: &Connection, uid: &str) -> rusqlite::Result<ArchiveCheck> {
    let (module_uid,module_name):(String,String)=c.query_row("SELECT f.module_uid,m.name FROM mx_fields f JOIN mx_modules m ON m.uid=f.module_uid WHERE f.uid=?1",[uid],|r|Ok((r.get(0)?,r.get(1)?)))?;
    let field = load_module_fields_initialized_db(c, &module_uid, true)?
        .into_iter()
        .find(|f| f.uid == uid)
        .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
    let dependencies=c.prepare("SELECT f.uid,f.module_uid,m.name,COALESCE(f.module_key,f.field_key),f.label,f.field_type,f.config_json FROM mx_fields f JOIN mx_modules m ON m.uid=f.module_uid WHERE f.active=1 ORDER BY m.name COLLATE NOCASE,f.label COLLATE NOCASE,f.uid")?.query_map([],|r|{
        let raw:String=r.get(6)?;
        Ok(Dependency{uid:r.get(0)?,module:r.get(1)?,module_name:r.get(2)?,key:r.get(3)?,label:r.get(4)?,kind:r.get(5)?,config:serde_json::from_str(&raw).map_err(|e|rusqlite::Error::FromSqlConversionFailure(6,rusqlite::types::Type::Text,Box::new(e)))?})
    })?.collect::<rusqlite::Result<Vec<_>>>()?;
    let relationships = dependencies
        .iter()
        .filter(|d| d.kind == "relationship")
        .map(|d| {
            (
                (d.module.as_str(), d.key.as_str()),
                d.config["target_module_uid"].as_str().unwrap_or(""),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut blockers = Vec::new();
    for d in &dependencies {
        if d.uid == uid {
            continue;
        }
        let mut reasons = Vec::new();
        if d.module == module_uid {
            for (key, usage) in super::form_rules::references(&d.config) {
                if key == field.key {
                    reasons.push(("form_rule",format!("Uses {} in its {}.",field.label,usage),"Open this field's Form rules and remove or change the condition/option mapping, then save its rules.".into()));
                }
            }
        }
        if d.kind == "relationship" {
            if d.config["target_module_uid"] == module_uid && d.config["label_field"] == field.key {
                reasons.push(("relationship_display",format!("Uses {} as the linked record's display name.",field.label),"Choose another Display field in this relationship, or archive the relationship first.".to_owned()));
            }
            for mapping in d.config["auto_fill"].as_array().into_iter().flatten() {
                if d.config["target_module_uid"] == module_uid
                    && mapping["source_field"] == field.key
                {
                    reasons.push(("auto_fill_source",format!("Copies {} into its {} field when a record is selected.",field.label,mapping["target_field"].as_str().unwrap_or("configured")),"Remove or change this mapping under Fill fields automatically, then Save field.".to_owned()));
                }
                if d.module == module_uid && mapping["target_field"] == field.key {
                    reasons.push(("auto_fill_destination",format!("Automatically fills {} from its linked record.",field.label),"Remove or change this destination under Fill fields automatically, then Save field.".to_owned()));
                }
            }
        } else if matches!(d.kind.as_str(), "lookup" | "rollup") {
            let relation = d.config["relationship_field"].as_str().unwrap_or("");
            if d.module == module_uid && relation == field.key {
                reasons.push(("computed_relationship",format!("Uses the {} relationship to calculate its value.",field.label),"Choose another Relationship for this field, or archive this calculated field first.".to_owned()));
            }
            if d.config["target_field"] == field.key
                && relationships.get(&(d.module.as_str(), relation)) == Some(&module_uid.as_str())
            {
                reasons.push((
                    "computed_source",
                    format!(
                        "Reads {} through its {} relationship.",
                        field.label, relation
                    ),
                    "Choose another Linked field for this lookup/rollup, or archive it first."
                        .to_owned(),
                ));
            }
        } else if d.kind == "formula"
            && d.module == module_uid
            && super::formula::referenced_fields(d.config["expression"].as_str().unwrap_or(""))
                .contains(&field.key.to_ascii_lowercase())
        {
            reasons.push(("formula_source",format!("Its formula uses the {} field.",field.label),"Change the formula expression to stop using this field, then Save field; or archive the formula first.".to_owned()));
        }
        for (kind, reason, resolution) in reasons {
            blockers.push(ArchiveBlocker {
                kind: kind.into(),
                module_uid: d.module.clone(),
                module_name: d.module_name.clone(),
                field_uid: Some(d.uid.clone()),
                field_label: Some(d.label.clone()),
                field_key: Some(d.key.clone()),
                field_type: Some(d.kind.clone()),
                reason,
                resolution,
                section: "field".into(),
                count: None,
            });
        }
    }
    if field.field_type == "relationship" {
        let (active,trashed):(i64,i64)=c.query_row("SELECT COUNT(DISTINCT CASE WHEN r.deleted_at IS NULL THEN r.uid END),COUNT(DISTINCT CASE WHEN r.deleted_at IS NOT NULL THEN r.uid END) FROM mx_record_links l JOIN mx_records r ON r.uid=l.record_uid WHERE l.field_uid=?1",[uid],|r|Ok((r.get(0)?,r.get(1)?)))?;
        if active + trashed > 0 {
            blockers.push(ArchiveBlocker{kind:"stored_links".into(),module_uid:module_uid.clone(),module_name:module_name.clone(),field_uid:None,field_label:None,field_key:None,field_type:None,reason:format!("{} active and {} trashed record(s) still contain links in {}.",active,trashed,field.label),resolution:"Open this module's records and clear this relationship. Restore trashed records first if you need to remove their links. Archiving a dependent lookup does not clear stored links.".into(),section:"records".into(),count:Some(active+trashed)});
        }
    }
    let layouts=c.prepare("SELECT m.uid,m.name,'folder' FROM mx_module_storage_layout_folders s JOIN mx_modules m ON m.uid=s.module_uid WHERE s.field_uid=?1 UNION SELECT m.uid,m.name,'prefix' FROM mx_module_storage_layout_meta s JOIN mx_modules m ON m.uid=s.module_uid WHERE s.file_prefix_field_uid=?1 ORDER BY 2,3")?.query_map([uid],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (module, name, role) in layouts {
        blockers.push(ArchiveBlocker{kind:format!("storage_{role}"),module_uid:module,module_name:name,field_uid:None,field_label:None,field_key:None,field_type:None,reason:format!("Uses {} as a {} in its N1 storage layout.",field.label,if role=="folder"{"folder level"}else{"file-name prefix"}),resolution:"Remove or replace this field in the module's N1 storage layout, then save the layout. Stored historical paths are not renamed.".into(),section:"storage".into(),count:None});
    }
    if field.field_type == "attachments" {
        let count: i64 = c.query_row(
            "SELECT COUNT(*) FROM mx_attachments WHERE attachment_field_uid=?1",
            [uid],
            |r| r.get(0),
        )?;
        if count > 0 {
            blockers.push(ArchiveBlocker{kind:"attachment_files".into(),module_uid:module_uid.clone(),module_name:module_name.clone(),field_uid:None,field_label:None,field_key:None,field_type:None,reason:format!("{} stored file(s) still belong to {}.",count,field.label),resolution:"Remove or migrate the files in this module's records before archiving their attachment field. No files are deleted by this dialog.".into(),section:"records".into(),count:Some(count)});
        }
    }
    Ok(ArchiveCheck {
        can_archive: field.active && blockers.is_empty(),
        field,
        module_uid,
        module_name,
        blockers,
    })
}

pub async fn get_archive_check(claims: Claims, Path(uid): Path<String>) -> Response {
    if !claims.can_manage_accounts() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"response":"Access denied."})),
        )
            .into_response();
    }
    match tokio::task::spawn_blocking(move||with_sql_connection(|c|{
        ensure_dynamic_schema(c)?;ensure_storage_layout_schema(c)?;
        let tx=c.unchecked_transaction()?;let result=check(&tx,&uid)?;tx.commit()?;Ok(result)
    })).await {
        Ok(Ok(result))=>Json(result).into_response(),
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows)))=>(StatusCode::NOT_FOUND,Json(json!({"response":"The field no longer exists. Refresh the module settings."}))).into_response(),
        _=>(StatusCode::INTERNAL_SERVER_ERROR,Json(json!({"response":"Could not check field dependencies. Nothing was archived; retry the check."}))).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE mx_modules(uid TEXT PRIMARY KEY,name TEXT);INSERT INTO mx_modules VALUES('employees','Employees'),('inventory','Inventory'),('other','Other');
          CREATE TABLE mx_fields(uid TEXT PRIMARY KEY,module_uid TEXT,field_key TEXT,module_key TEXT,label TEXT,field_type TEXT,required INTEGER DEFAULT 0,unique_value INTEGER DEFAULT 0,searchable INTEGER DEFAULT 1,sortable INTEGER DEFAULT 1,table_visible INTEGER DEFAULT 1,table_priority INTEGER DEFAULT 50,position INTEGER DEFAULT 0,active INTEGER DEFAULT 1,config_json TEXT);
          CREATE TABLE mx_records(uid TEXT,module_uid TEXT,deleted_at INTEGER);
          CREATE TABLE mx_record_links(record_uid TEXT,field_uid TEXT,target_uid TEXT);
          CREATE TABLE mx_module_storage_layout_folders(module_uid TEXT,field_uid TEXT);
          CREATE TABLE mx_module_storage_layout_meta(module_uid TEXT,file_prefix_field_uid TEXT);
          CREATE TABLE mx_attachments(attachment_field_uid TEXT);").unwrap();
        c
    }
    fn field(
        c: &Connection,
        uid: &str,
        module: &str,
        key: &str,
        label: &str,
        kind: &str,
        config: Value,
    ) {
        c.execute("INSERT INTO mx_fields(uid,module_uid,field_key,module_key,label,field_type,config_json) VALUES(?1,?2,?3,?3,?4,?5,?6)",params![uid,module,key,label,kind,config.to_string()]).unwrap();
    }
    #[test]
    fn reports_named_cross_module_dependencies_without_confusing_same_keys() {
        let c = db();
        field(
            &c,
            "name",
            "employees",
            "name",
            "Employee name",
            "text",
            json!({}),
        );
        field(
            &c,
            "other-name",
            "other",
            "name",
            "Other name",
            "text",
            json!({}),
        );
        field(
            &c,
            "issuer",
            "inventory",
            "issuer",
            "Issuer",
            "relationship",
            json!({"target_module_uid":"employees","label_field":"name","auto_fill":[{"source_field":"name","target_field":"issuer_text"}]}),
        );
        field(
            &c,
            "copied",
            "inventory",
            "issuer_text",
            "Copied name",
            "text",
            json!({}),
        );
        field(
            &c,
            "lookup",
            "inventory",
            "looked_up",
            "Current name",
            "lookup",
            json!({"relationship_field":"issuer","target_field":"name"}),
        );
        let result = check(&c, "name").unwrap();
        assert!(!result.can_archive);
        assert_eq!(result.blockers.len(), 3);
        assert!(result.blockers.iter().all(|b| b.module_name == "Inventory"
            && b.field_uid.is_some()
            && !b.resolution.is_empty()));
        assert!(
            result.blocked_response()["response"]
                .as_str()
                .unwrap()
                .contains("Inventory → Issuer")
        );
        assert!(check(&c, "other-name").unwrap().can_archive);
        c.execute(
            "UPDATE mx_fields SET active=0 WHERE uid IN ('issuer','lookup')",
            [],
        )
        .unwrap();
        assert!(check(&c, "name").unwrap().can_archive);
    }
    #[test]
    fn exposes_computed_and_autofill_destinations_and_active_trashed_link_counts() {
        let c = db();
        field(
            &c,
            "issuer",
            "inventory",
            "issuer",
            "Issuer",
            "relationship",
            json!({"target_module_uid":"employees","label_field":"name","auto_fill":[{"source_field":"class","target_field":"classification"}]}),
        );
        field(
            &c,
            "classification",
            "inventory",
            "classification",
            "Classification",
            "text",
            json!({}),
        );
        field(
            &c,
            "lookup",
            "inventory",
            "position",
            "Position",
            "lookup",
            json!({"relationship_field":"issuer","target_field":"name"}),
        );
        field(
            &c,
            "rollup",
            "inventory",
            "count",
            "Linked count",
            "rollup",
            json!({"relationship_field":"issuer","function":"count"}),
        );
        c.execute_batch("INSERT INTO mx_records VALUES('live','inventory',NULL),('trash','inventory',1);INSERT INTO mx_record_links VALUES('live','issuer','a'),('live','issuer','b'),('trash','issuer','a');").unwrap();
        let result = check(&c, "issuer").unwrap();
        assert_eq!(result.blockers.len(), 3);
        let links = result
            .blockers
            .iter()
            .find(|b| b.kind == "stored_links")
            .unwrap();
        assert_eq!(links.count, Some(2));
        assert!(links.reason.contains("1 active and 1 trashed"));
        assert_eq!(
            check(&c, "classification").unwrap().blockers[0].kind,
            "auto_fill_destination"
        );
        assert!(
            result
                .blockers
                .iter()
                .filter(|b| b.kind == "computed_relationship")
                .count()
                == 2
        );
    }
    #[test]
    fn reports_formula_storage_and_attachment_blockers_without_mutating_values() {
        let c = db();
        field(
            &c,
            "amount",
            "inventory",
            "amount",
            "Amount",
            "decimal",
            json!({}),
        );
        field(
            &c,
            "income",
            "inventory",
            "income",
            "Income",
            "formula",
            json!({"expression":"AMOUNT * 2"}),
        );
        field(
            &c,
            "files",
            "inventory",
            "files",
            "Files",
            "attachments",
            json!({}),
        );
        c.execute_batch("INSERT INTO mx_module_storage_layout_folders VALUES('inventory','amount');INSERT INTO mx_module_storage_layout_meta VALUES('inventory','amount');INSERT INTO mx_attachments VALUES('files'),('files');").unwrap();
        let result = check(&c, "amount").unwrap();
        assert_eq!(result.blockers.len(), 3);
        assert!(!result.can_archive);
        assert!(result.blockers.iter().any(|b| b.kind == "formula_source"));
        let files = check(&c, "files").unwrap();
        assert_eq!(files.blockers[0].count, Some(2));
        assert_eq!(files.status(), StatusCode::CONFLICT);
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM mx_attachments", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
        c.execute("UPDATE mx_fields SET active=0 WHERE uid='income'", [])
            .unwrap();
        c.execute_batch("DELETE FROM mx_module_storage_layout_folders;DELETE FROM mx_module_storage_layout_meta;").unwrap();
        assert!(check(&c, "amount").unwrap().can_archive);
    }
}
