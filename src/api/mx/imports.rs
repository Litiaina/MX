//! Durable, create-only imports. A row's success is committed in the SAME
//! transaction as its normal record, request deduplication and version history.
use super::{
    import_parser::{self, Cell},
    records,
    schema::{FieldDefinition, load_module_fields_initialized_db},
};
use crate::{
    api::{
        live::publish_live_event,
        modules::{ensure_module_schema, module_can_for_user_initialized},
        operations::ClientOperation,
    },
    db::connector::{SqliteDatabaseError, with_sql_connection, with_sql_write},
    middleware::auth::{Claims, auth},
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};
use uuid::Uuid;

// Bounded preparation memory, and a single writer per import API instance.
// SQLite transactions also enforce row admission, including across processes.
static WRITE: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));
static READ_FILE: LazyLock<tokio::sync::Semaphore> =
    LazyLock::new(|| tokio::sync::Semaphore::new(1));
fn invalid(s: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(s.into())
}
fn decode<T: serde::de::DeserializeOwned>(s: &str) -> rusqlite::Result<T> {
    serde_json::from_str(s).map_err(|_| invalid("Stored import data is unreadable."))
}
pub(crate) fn ensure(c: &Connection) -> rusqlite::Result<()> {
    if crate::db::schema::initialized(c) {
        return Ok(());
    }
    c.execute_batch("CREATE TABLE IF NOT EXISTS mx_import_jobs (
        uid TEXT PRIMARY KEY,
        module_uid TEXT NOT NULL REFERENCES mx_modules(uid) ON DELETE CASCADE,
        actor_uid TEXT NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
        filename TEXT NOT NULL, fingerprint TEXT NOT NULL, sheets_json TEXT NOT NULL,
        config_json TEXT NOT NULL DEFAULT '{}', state TEXT NOT NULL DEFAULT 'draft',
        created_at INTEGER NOT NULL DEFAULT(unixepoch()));
        CREATE INDEX IF NOT EXISTS mx_import_owner ON mx_import_jobs(module_uid,actor_uid,created_at);
        CREATE TABLE IF NOT EXISTS mx_import_rows (
        job_uid TEXT NOT NULL REFERENCES mx_import_jobs(uid) ON DELETE CASCADE,
        sheet INTEGER NOT NULL, source_row INTEGER NOT NULL, cells_json TEXT NOT NULL,
        state TEXT NOT NULL DEFAULT 'source', values_json TEXT NOT NULL DEFAULT '{}',
        errors_json TEXT NOT NULL DEFAULT '[]', operation_uid TEXT NOT NULL, record_uid TEXT,
        PRIMARY KEY(job_uid,sheet,source_row));
        CREATE INDEX IF NOT EXISTS mx_import_progress ON mx_import_rows(job_uid,state,sheet,source_row);")
}
fn access(c: &Connection, claims: &Claims, module: &str) -> rusqlite::Result<()> {
    ensure_module_schema(c)?;
    if !claims.can_write_records()
        || !module_can_for_user_initialized(c, module, &claims.uid, claims.access_level, "create")?
    {
        return Err(invalid(
            "IMPORT_FORBIDDEN:You need permission to view and create records in this module.",
        ));
    }
    Ok(())
}
fn own(c: &Connection, claims: &Claims, module: &str, uid: &str) -> rusqlite::Result<()> {
    access(c, claims, module)?;
    ensure(c)?;
    let exists: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM mx_import_jobs WHERE uid=?1 AND module_uid=?2 AND actor_uid=?3)",params![uid,module,claims.uid],|r|r.get(0))?;
    if !exists {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    Ok(())
}
fn job(c: &Connection, uid: &str) -> rusqlite::Result<Value> {
    let mut result = c.query_row("SELECT uid,filename,state,sheets_json,config_json,created_at FROM mx_import_jobs WHERE uid=?1",[uid],|r|Ok(json!({"uid":r.get::<_,String>(0)?,"filename":r.get::<_,String>(1)?,"state":r.get::<_,String>(2)?,"sheets":decode::<Value>(&r.get::<_,String>(3)?)?,"config":decode::<Value>(&r.get::<_,String>(4)?)?,"created_at":r.get::<_,i64>(5)?})))?;
    let mut counts = BTreeMap::new();
    let mut s =
        c.prepare("SELECT state,count(*) FROM mx_import_rows WHERE job_uid=?1 GROUP BY state")?;
    for r in s.query_map([uid], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
        let (k, v) = r?;
        counts.insert(k, v);
    }
    let pending = counts.get("pending").copied().unwrap_or(0);
    let imported = counts.get("imported").copied().unwrap_or(0);
    let failed = counts.get("failed").copied().unwrap_or(0);
    result["total"] = json!(pending + imported + failed);
    result["processed"] = json!(imported + failed);
    result["imported"] = json!(imported);
    result["failed"] = json!(failed);
    result["pending"] = json!(pending);
    Ok(result)
}

// Diagnostics are loaded at preview/reopen, not for every processing batch.
// Keep common blockers visible without repeatedly scanning all error JSON while
// a large import is running. Counts refer to rows, not the number of messages.
fn job_with_validation(c: &Connection, uid: &str) -> rusqlite::Result<Value> {
    let mut result = job(c, uid)?;
    let (ready, invalid): (i64, i64) = c.query_row(
        "SELECT coalesce(sum(state='pending' AND errors_json='[]'),0),
                coalesce(sum(state IN ('pending','failed') AND errors_json<>'[]'),0)
         FROM mx_import_rows WHERE job_uid=?1",
        [uid],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut statement = c.prepare(
        "SELECT json_extract(e.value,'$.field'), json_extract(e.value,'$.message'),
                count(DISTINCT printf('%d:%d',r.sheet,r.source_row)) AS affected
         FROM mx_import_rows r, json_each(r.errors_json) e
         WHERE r.job_uid=?1 AND r.state IN ('pending','failed')
         GROUP BY 1,2 ORDER BY affected DESC,1,2 LIMIT 8",
    )?;
    let issues = statement
        .query_map([uid], |r| {
            Ok(json!({"field":r.get::<_,String>(0)?,"message":r.get::<_,String>(1)?,"rows":r.get::<_,i64>(2)?}))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    result["validation"] = json!({"ready":ready,"invalid":invalid,"issues":issues});
    Ok(result)
}
async fn db<F>(work: F) -> Response
where
    F: FnOnce(&Connection) -> rusqlite::Result<Value> + Send + 'static,
{
    match tokio::task::spawn_blocking(move || with_sql_connection(work)).await {
        Ok(Ok(value)) => Json(value).into_response(),
        Ok(Err(error)) => error_response(error),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR,Json(json!({"response":"Import task stopped. Reopen Import history to reconcile progress."}))).into_response(),
    }
}
fn error_response(error: SqliteDatabaseError) -> Response {
    if !matches!(
        &error,
        SqliteDatabaseError::Sqlite(
            rusqlite::Error::InvalidParameterName(_) | rusqlite::Error::QueryReturnedNoRows
        )
    ) {
        tracing::error!(error=%error,"Spreadsheet import storage failure");
    }
    let (status,message) = match error {
        SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows) => (StatusCode::NOT_FOUND,"Import was not found.".into()),
        SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(s)) => if let Some(s)=s.strip_prefix("IMPORT_FORBIDDEN:") {(StatusCode::FORBIDDEN,s.into())} else {(StatusCode::BAD_REQUEST,s)},
        _ => (StatusCode::SERVICE_UNAVAILABLE,"Import storage is temporarily unavailable. Resume from Import history; committed rows will not be duplicated.".into()),
    };
    (status, Json(json!({"response":message}))).into_response()
}
pub fn routes() -> Router {
    Router::new()
        .route(
            "/mx/v1/modules/{module}/imports",
            get(history).post(upload).layer(DefaultBodyLimit::max(
                import_parser::MAX_UPLOAD + 1024 * 1024,
            )),
        )
        .route(
            "/mx/v1/modules/{module}/imports/{uid}",
            get(detail).delete(discard),
        )
        .route(
            "/mx/v1/modules/{module}/imports/{uid}/configure",
            put(configure),
        )
        .route(
            "/mx/v1/modules/{module}/imports/{uid}/process",
            post(process),
        )
        .route("/mx/v1/modules/{module}/imports/{uid}/rows", get(rows))
        .route("/mx/v1/modules/{module}/imports/{uid}/retry", post(retry))
        .layer(axum::middleware::from_fn(auth))
}
async fn upload(claims: Claims, Path(module): Path<String>, mut multipart: Multipart) -> Response {
    let checked = tokio::task::spawn_blocking({
        let claims = claims.clone();
        let module = module.clone();
        move || with_sql_connection(|c| access(c, &claims, &module))
    })
    .await;
    match checked {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return error_response(e),
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"response":"MX could not check import permissions. Try again."})),
            )
                .into_response();
        }
    }
    let _permit = READ_FILE.acquire().await.unwrap();
    let mut uid = String::new();
    let mut filename = String::new();
    let mut bytes = None;
    let mut delimiter = None;
    loop {
        let field =
            match multipart.next_field().await {
                Ok(Some(f)) => f,
                Ok(None) => break,
                Err(_) => return (
                    StatusCode::BAD_REQUEST,
                    Json(
                        json!({"response":"File transfer was incomplete. Select the file again."}),
                    ),
                )
                    .into_response(),
            };
        match field.name().unwrap_or("") {
            "job_uid" => {
                uid = field.text().await.unwrap_or_default();
            }
            "delimiter" => {
                delimiter = match field.text().await.unwrap_or_default().as_str() {
                    "semicolon" => Some(b';'),
                    "tab" => Some(b'\t'),
                    _ => Some(b','),
                };
            }
            "file" if bytes.is_none() => {
                filename = field
                    .file_name()
                    .unwrap_or("import.csv")
                    .chars()
                    .take(1024)
                    .collect();
                bytes=match field.bytes().await {Ok(b) if b.len()<=import_parser::MAX_UPLOAD=>Some(b),_=>return (StatusCode::PAYLOAD_TOO_LARGE,Json(json!({"response":"Spreadsheet is incomplete or exceeds the 64 MiB import safety budget."}))).into_response()};
            }
            _ => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"response":"Send one file, a job UUID and optional delimiter."})),
                )
                    .into_response();
            }
        }
    }
    let Some(bytes) = bytes else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"response":"Select a spreadsheet."})),
        )
            .into_response();
    };
    if Uuid::parse_str(&uid).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"response":"job_uid must be a UUID."})),
        )
            .into_response();
    }
    db(move |c| {
        access(c,&claims,&module)?; ensure(c)?;
        let mut hash=Sha256::new();hash.update([delimiter.unwrap_or(if filename.to_ascii_lowercase().ends_with(".tsv"){b'\t'}else{b','})]);hash.update(&bytes);
        let fingerprint:String=hash.finalize().iter().map(|b|format!("{b:02x}")).collect();
        if let Some(old)=c.query_row("SELECT fingerprint FROM mx_import_jobs WHERE uid=?1 AND actor_uid=?2 AND module_uid=?3",params![uid,claims.uid,module],|r|r.get::<_,String>(0)).optional()? {
            if old!=fingerprint {return Err(invalid("This import ID belongs to a different file. Start a new import."));} return job(c,&uid);
        }
        let sheets=import_parser::parse(&filename,&bytes,delimiter).map_err(invalid)?;
        let metadata: Vec<Value>=sheets.iter().enumerate().map(|(index,s)|json!({"index":index,"name":s.name,"row_count":s.rows.len(),"error":s.error,"sample":s.rows.iter().take(12).map(|row|json!({"row":row.row,"cells":row.cells.iter().map(|(key,cell)|(*key,Cell{text:cell.text.chars().take(256).collect(),kind:cell.kind.clone()})).collect::<BTreeMap<_,_>>()})).collect::<Vec<_>>(),"columns":s.rows.iter().flat_map(|r|r.cells.keys()).copied().max().unwrap_or(0)})).collect();
        let tx=rusqlite::Transaction::new_unchecked(c,rusqlite::TransactionBehavior::Immediate)?;
        tx.execute("INSERT INTO mx_import_jobs(uid,module_uid,actor_uid,filename,fingerprint,sheets_json) VALUES(?1,?2,?3,?4,?5,?6)",params![uid,module,claims.uid,filename,fingerprint,json!(metadata).to_string()])?;
        {let mut stmt=tx.prepare("INSERT INTO mx_import_rows(job_uid,sheet,source_row,cells_json,operation_uid) VALUES(?1,?2,?3,?4,?5)")?;
        for (i,sheet) in sheets.iter().enumerate(){for row in &sheet.rows {stmt.execute(params![uid,i as i64,row.row,json!(row.cells).to_string(),Uuid::new_v4().to_string()])?;}}}
        tx.commit()?;job(c,&uid)
    }).await
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Column {
    pub column: u32,
    pub key: String,
    #[serde(default)]
    pub fill_down: bool,
    #[serde(default)]
    pub date_format: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    pub index: u32,
    pub header_row: u32,
    pub first_data_row: u32,
    pub columns: Vec<Column>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub sheets: Vec<Mapping>,
}

pub(super) fn writable(field: &FieldDefinition) -> bool {
    field.active
        && !matches!(
            field.field_type.as_str(),
            "attachments" | "auto_number" | "formula" | "lookup" | "rollup"
        )
}
fn convert(cell: &Cell, field: &FieldDefinition, format: &str) -> Result<Value, String> {
    if cell.kind == "error" {
        return Err(cell.text.clone());
    }
    let text = cell.text.trim();
    if text.is_empty() {
        return Ok(Value::Null);
    }
    match field.field_type.as_str() {
        "date" => {
            let fmt = if cell.kind == "date" {
                "%Y-%m-%d"
            } else {
                match format {
                    "dmy" => "%d/%m/%Y",
                    "mdy" => "%m/%d/%Y",
                    "mdy-short-dot" => "%m.%d.%y",
                    "month-long" => "%B %d %Y",
                    "month-short" => "%b %d,%Y",
                    _ => "%Y-%m-%d",
                }
            };
            let text = if cell.kind == "date" {
                text.get(..10).unwrap_or(text)
            } else {
                text
            };
            chrono::NaiveDate::parse_from_str(text,fmt).map(|d|json!(d.to_string())).map_err(|_|format!("Invalid date '{text}'. Choose the matching date format or correct this row."))
        }
        "boolean" => match text.to_ascii_lowercase().as_str() {
            "true" | "yes" | "1" => Ok(json!(true)),
            "false" | "no" | "0" => Ok(json!(false)),
            _ => Err("Use Yes/No, True/False or 1/0.".into()),
        },
        "select" => {
            let options: Vec<&str> = field.config["options"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            if let Some(exact) = options.iter().find(|o| **o == text) {
                return Ok(json!(exact));
            }
            let matches: Vec<&str> = options
                .into_iter()
                .filter(|o| o.eq_ignore_ascii_case(text))
                .collect();
            match matches.as_slice() {
                [only] => Ok(json!(only)),
                [] => Err(format!(
                    "Unknown choice '{text}'. Use one of this field's configured values or correct the row after importing."
                )),
                _ => Err(
                    "This choice matches multiple configured values. Use their exact spelling."
                        .into(),
                ),
            }
        }
        "relationship" => {
            if field.config["multiple"] == true {
                serde_json::from_str(text).map_err(|_| {
                    "Use a JSON array of MX record IDs for a multiple relationship.".into()
                })
            } else {
                Ok(json!(text))
            }
        }
        _ => Ok(json!(cell.text)),
    }
}
async fn configure(
    claims: Claims,
    Path((module, uid)): Path<(String, String)>,
    Json(config): Json<Configuration>,
) -> Response {
    let _lock = WRITE.lock().await;
    db(move |c|{
        own(c,&claims,&module,&uid)?;
        let tx=rusqlite::Transaction::new_unchecked(c,rusqlite::TransactionBehavior::Immediate)?;
        let imported:i64=tx.query_row("SELECT count(*) FROM mx_import_rows WHERE job_uid=?1 AND state='imported'",[&uid],|r|r.get(0))?;
        if imported>0{return Err(invalid("An import with saved records cannot be remapped. Correct failed rows instead; saved records will not be imported again."));}
        let fields=load_module_fields_initialized_db(&tx,&module,false)?;
        let metadata=job(&tx,&uid)?;let mut indexes=BTreeSet::new();
        if config.sheets.is_empty()||config.sheets.len()>128{return Err(invalid("Select at least one sheet."));}
        tx.execute("UPDATE mx_import_rows SET state='excluded',values_json='{}',errors_json='[]' WHERE job_uid=?1",[&uid])?;
        for mapping in &config.sheets{
            if !indexes.insert(mapping.index)||metadata["sheets"].as_array().is_none_or(|s|mapping.index as usize>=s.len()){return Err(invalid("Select distinct sheets from this workbook."));}
            if metadata["sheets"][mapping.index as usize]["error"].is_string(){return Err(invalid("This sheet could not be prepared. Correct the reported spreadsheet issue or choose another sheet."));}
            if mapping.first_data_row<=mapping.header_row||mapping.header_row==0||mapping.columns.is_empty()||mapping.columns.len()>512{return Err(invalid("Choose a header row, a later first data row, and at least one column mapping."));}
            let mut keys=BTreeSet::new();let mut columns=BTreeSet::new();
            for column in &mapping.columns{
                if column.column==0||column.column>512||!columns.insert(column.column)||!keys.insert(&column.key)||!fields.iter().any(|f|f.key==column.key&&writable(f)){return Err(invalid("Each column must map once to a distinct writable field. Automatic and attachment fields cannot be imported."));}
                if !matches!(column.date_format.as_str(),""|"iso"|"dmy"|"mdy"|"mdy-short-dot"|"month-long"|"month-short"){return Err(invalid("Unknown date format."));}
            }
            let mut statement=tx.prepare("SELECT source_row,cells_json FROM mx_import_rows WHERE job_uid=?1 AND sheet=?2 AND source_row>=?3 ORDER BY source_row")?;
            let source=statement.query_map(params![uid,mapping.index,mapping.first_data_row],|r|Ok((r.get::<_,u32>(0)?,decode::<BTreeMap<u32,Cell>>(&r.get::<_,String>(1)?)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            drop(statement);let mut previous:BTreeMap<u32,Cell>=BTreeMap::new();
            for (row,cells) in source{
                let mut values=BTreeMap::new();let mut errors=Vec::new();let mut populated=false;
                for column in &mapping.columns{
                    let raw=cells.get(&column.column).filter(|c|!c.text.trim().is_empty()).cloned();
                    // A note in an unmapped column alone must not create a record;
                    // carried values alone do not turn blank lines into records.
                    populated|=raw.is_some();
                    if let Some(v)=&raw{previous.insert(column.column,v.clone());}
                    let cell=raw.or_else(||if column.fill_down{previous.get(&column.column).cloned()}else{None}).unwrap_or_default();
                    let field=fields.iter().find(|f|f.key==column.key).unwrap();
                    match convert(&cell,field,&column.date_format){Ok(v)=>{values.insert(column.key.clone(),v);},Err(message)=>errors.push(json!({"field":field.label,"column":column.column,"message":message,"value":cell.text}))}
                }
                if !populated{continue;}
                if errors.is_empty(){ match records::import_validate(&tx,&fields,&values,&claims.uid){Ok(_)=>{},Err(message)=>errors.push(json!({"field":"Record validation","message":message}))} }
                tx.execute("UPDATE mx_import_rows SET state='pending',values_json=?1,errors_json=?2 WHERE job_uid=?3 AND sheet=?4 AND source_row=?5",params![json!(values).to_string(),json!(errors).to_string(),uid,mapping.index,row])?;
            }
        }
        tx.execute("UPDATE mx_import_jobs SET config_json=?1,state='ready' WHERE uid=?2",params![json!(config).to_string(),uid])?;
        let j=job_with_validation(&tx,&uid)?;if j["total"]==0{return Err(invalid("No populated mapped rows were found after the chosen first data rows."));}tx.commit()?;Ok(j)
    }).await
}

pub(super) struct Commit {
    pub job: String,
    pub sheet: u32,
    pub row: u32,
    pub auth_version: i64,
}
fn current_actor(c: &Connection, actor: &str, version: i64) -> rusqlite::Result<i64> {
    let current: Option<(i64, i64)> = c
        .query_row(
            "SELECT access_level,auth_version FROM users WHERE uid=?1",
            [actor],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    match current {
        Some((level, v))
            if v == version
                && matches!(
                    level,
                    crate::middleware::auth::ACCESS_ADMINISTRATOR
                        | crate::middleware::auth::ACCESS_MANAGER
                        | crate::middleware::auth::ACCESS_EDITOR
                ) =>
        {
            Ok(level)
        }
        _ => Err(invalid(
            "IMPORT_FORBIDDEN:This account or session no longer has record creation authority. Sign in again and check permissions before resuming.",
        )),
    }
}
pub(super) fn admit(
    c: &Connection,
    ctx: &Commit,
    module: &str,
    actor: &str,
    values: &BTreeMap<String, Value>,
) -> rusqlite::Result<()> {
    let level = current_actor(c, actor, ctx.auth_version)?;
    if !module_can_for_user_initialized(c, module, actor, level, "create")? {
        return Err(invalid(
            "IMPORT_FORBIDDEN:Record creation permission was revoked.",
        ));
    }
    let stored:Option<String>=c.query_row("SELECT r.values_json FROM mx_import_rows r JOIN mx_import_jobs j ON j.uid=r.job_uid WHERE r.job_uid=?1 AND r.sheet=?2 AND r.source_row=?3 AND r.state='pending' AND j.module_uid=?4 AND j.actor_uid=?5",params![ctx.job,ctx.sheet,ctx.row,module,actor],|r|r.get(0)).optional()?;
    if stored.is_none_or(|s| decode::<BTreeMap<String, Value>>(&s).ok().as_ref() != Some(values)) {
        return Err(invalid(
            "IMPORT_CHANGED:This row was already processed or its submitted values changed. Refresh import progress.",
        ));
    }
    Ok(())
}
pub(super) fn complete(c: &Connection, ctx: &Commit, record: &str) -> rusqlite::Result<()> {
    let changed=c.execute("UPDATE mx_import_rows SET state='imported',record_uid=?1,errors_json='[]' WHERE job_uid=?2 AND sheet=?3 AND source_row=?4 AND state='pending'",params![record,ctx.job,ctx.sheet,ctx.row])?;
    if changed != 1 {
        return Err(invalid(
            "IMPORT_CHANGED:The pending import row disappeared before commit.",
        ));
    }
    Ok(())
}
fn run(
    claims: &Claims,
    module: &str,
    uid: &str,
    only: Option<(u32, u32)>,
) -> Result<Value, SqliteDatabaseError> {
    let pending = with_sql_connection(|c| {
        own(c, claims, module, uid)?;
        let state: String = c.query_row(
            "SELECT state FROM mx_import_jobs WHERE uid=?1",
            [uid],
            |r| r.get(0),
        )?;
        if state == "draft" {
            return Err(invalid(
                "Map the columns and preview before starting this import.",
            ));
        }
        let mut s=c.prepare("SELECT sheet,source_row,values_json,errors_json,operation_uid FROM mx_import_rows WHERE job_uid=?1 AND state='pending' AND (?2 IS NULL OR (sheet=?2 AND source_row=?3)) ORDER BY sheet,source_row LIMIT 20")?;
        s.query_map(params![uid, only.map(|p| p.0), only.map(|p| p.1)], |r| {
            Ok((
                r.get::<_, u32>(0)?,
                r.get::<_, u32>(1)?,
                decode::<BTreeMap<String, Value>>(&r.get::<_, String>(2)?)?,
                decode::<Vec<Value>>(&r.get::<_, String>(3)?)?,
                r.get::<_, String>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
    })?;
    let started = std::time::Instant::now();
    for (index, (sheet, row, values, mut errors, operation)) in pending.into_iter().enumerate() {
        // Under concurrent interactive writes, twenty individually admitted
        // rows can exceed the client's request deadline. Always make progress,
        // then return committed counters promptly; remaining rows stay pending.
        if index > 0 && started.elapsed() >= std::time::Duration::from_secs(1) { break; }
        if errors.is_empty() {
            let operation =
                ClientOperation::parse(Some(operation), &json!(values)).map_err(invalid)?;
            match records::create_record_commit(module.into(),values,claims.uid.clone(),operation,Some(Commit{job:uid.into(),sheet,row,auth_version:claims.auth_version})) {
                Ok(_)=>continue,
                Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(s))) if s.starts_with("IMPORT_FORBIDDEN:")||s.starts_with("IMPORT_CHANGED:")=>return Err(invalid(s).into()),
                Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(s)))=>errors.push(json!({"field":"Record validation","message":s})),
                Err(SqliteDatabaseError::Sqlite(rusqlite::Error::SqliteFailure(e,_))) if matches!(e.extended_code,rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE|rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY|rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY)=>errors.push(json!({"field":"Unique field / relationship","message":"This row conflicts with an existing unique value, control number or linked record. Inspect its fields and correct it; no partial record was created."})),
                Err(error)=>return Err(error), // infrastructure failures pause, never skip data
            }
        }
        with_sql_write(|c| {
            c.execute("UPDATE mx_import_rows SET state='failed',errors_json=?1 WHERE job_uid=?2 AND sheet=?3 AND source_row=?4 AND state='pending'",params![json!(errors).to_string(),uid,sheet,row])?;
            Ok(())
        })?;
    }
    with_sql_write(|c| {
        let j = job(c, uid)?;
        c.execute(
            "UPDATE mx_import_jobs SET state=?1 WHERE uid=?2",
            params![
                if j["pending"] == 0 {
                    "completed"
                } else {
                    "paused"
                },
                uid
            ],
        )?;
        job(c, uid)
    })
}
async fn process(claims: Claims, Path((module, uid)): Path<(String, String)>) -> Response {
    let _lock = WRITE.lock().await;
    let event_module = module.clone();
    let actor = claims.uid.clone();
    let result = tokio::task::spawn_blocking(move || run(&claims, &module, &uid, None)).await;
    publish_live_event(
        "record.imported",
        Some(&actor),
        json!({"module_uid":event_module}),
    );
    match result {
        Ok(Ok(j)) => Json(j).into_response(),
        Ok(Err(e)) => error_response(e),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"response":"Import paused. Reopen history to resume committed progress."})),
        )
            .into_response(),
    }
}
async fn detail(claims: Claims, Path((module, uid)): Path<(String, String)>) -> Response {
    db(move |c| {
        own(c, &claims, &module, &uid)?;
        job_with_validation(c, &uid)
    })
    .await
}
#[derive(Default, Deserialize)]
struct Page {
    #[serde(default)]
    page: u32,
    #[serde(default)]
    state: String,
    #[serde(default)]
    sheet: Option<u32>,
}
async fn history(claims: Claims, Path(module): Path<String>, Query(p): Query<Page>) -> Response {
    db(move|c|{
    access(c,&claims,&module)?;ensure(c)?;let page=p.page.max(1);
    let total:i64=c.query_row("SELECT count(*) FROM mx_import_jobs WHERE module_uid=?1 AND actor_uid=?2",params![module,claims.uid],|r|r.get(0))?;
    let mut s=c.prepare("SELECT uid FROM mx_import_jobs WHERE module_uid=?1 AND actor_uid=?2 ORDER BY created_at DESC,uid LIMIT 20 OFFSET ?3")?;
    let ids=s.query_map(params![module,claims.uid,i64::from(page-1)*20],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(json!({"jobs":ids.iter().map(|id|job(c,id)).collect::<rusqlite::Result<Vec<_>>>()?,"total":total,"page":page,"limit":20}))
}).await
}
async fn rows(
    claims: Claims,
    Path((module, uid)): Path<(String, String)>,
    Query(p): Query<Page>,
) -> Response {
    db(move|c|{
    own(c,&claims,&module,&uid)?;let page=p.page.max(1);let state=if p.state.is_empty(){"pending"}else{p.state.as_str()};
    if !matches!(state,"pending"|"failed"|"imported"|"source"|"issues"){return Err(invalid("Choose pending, failed, imported, source or preview issues."));}
    let total:i64=c.query_row("SELECT count(*) FROM mx_import_rows WHERE job_uid=?1 AND (state=?2 OR (?2='issues' AND state IN ('pending','failed') AND errors_json<>'[]')) AND (?3 IS NULL OR sheet=?3)",params![uid,state,p.sheet],|r|r.get(0))?;
    let mut s=c.prepare("SELECT sheet,source_row,cells_json,values_json,errors_json,record_uid FROM mx_import_rows WHERE job_uid=?1 AND (state=?2 OR (?2='issues' AND state IN ('pending','failed') AND errors_json<>'[]')) AND (?3 IS NULL OR sheet=?3) ORDER BY sheet,source_row LIMIT 20 OFFSET ?4")?;
    let mut rows=s.query_map(params![uid,state,p.sheet,i64::from(page-1)*20],|r|Ok(json!({"sheet":r.get::<_,u32>(0)?,"row":r.get::<_,u32>(1)?,"cells":decode::<Value>(&r.get::<_,String>(2)?)?,"values":decode::<Value>(&r.get::<_,String>(3)?)?,"errors":decode::<Value>(&r.get::<_,String>(4)?)?,"record_uid":r.get::<_,Option<String>>(5)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
    if state=="pending" {
        let fields=load_module_fields_initialized_db(c,&module,false)?;
        for row in &mut rows {let values:BTreeMap<String,Value>=serde_json::from_value(row["values"].clone()).map_err(|_|invalid("Stored mapped values are unreadable."))?;
            if let Ok(prepared)=records::import_validate(c,&fields,&values,&claims.uid){row["preview_values"]=json!(prepared);}
        }
    }
    Ok(json!({"rows":rows,"total":total,"page":page,"limit":20}))
}).await
}
#[derive(Deserialize)]
struct Correction {
    sheet: u32,
    row: u32,
    values: BTreeMap<String, Value>,
}
async fn retry(
    claims: Claims,
    Path((module, uid)): Path<(String, String)>,
    Json(request): Json<Correction>,
) -> Response {
    let _lock = WRITE.lock().await;
    let actor = claims.uid.clone();
    let event_module = module.clone();
    let result=tokio::task::spawn_blocking(move||{
    let existing=with_sql_connection(|c|{
        own(c,&claims,&module,&uid)?;
        let tx=rusqlite::Transaction::new_unchecked(c,rusqlite::TransactionBehavior::Immediate)?;
        let (state,values,record):(String,String,Option<String>)=tx.query_row("SELECT state,values_json,record_uid FROM mx_import_rows WHERE job_uid=?1 AND sheet=?2 AND source_row=?3",params![uid,request.sheet,request.row],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        if state=="imported"{return Ok(record);}
        if state=="pending" && decode::<BTreeMap<String,Value>>(&values)?!=request.values{return Err(invalid("This correction is already being saved with different values. Reconcile progress first."));}
        if !matches!(state.as_str(),"pending"|"failed"){return Err(invalid("Only a failed row can be corrected."));}
        if state=="failed"{
            tx.execute("UPDATE mx_import_rows SET state='pending',values_json=?1,errors_json='[]',operation_uid=?2 WHERE job_uid=?3 AND sheet=?4 AND source_row=?5",params![json!(request.values).to_string(),Uuid::new_v4().to_string(),uid,request.sheet,request.row])?;
        }
        tx.commit()?;
        // Drop this pool connection before run acquires others.
        Ok(None)
    })?;
    if let Some(record)=existing{return Ok::<_,SqliteDatabaseError>(json!({"record_uid":record}));}
    run(&claims,&module,&uid,Some((request.sheet,request.row)))?;
    with_sql_connection(|c|{
        let (record,errors):(Option<String>,String)=c.query_row("SELECT record_uid,errors_json FROM mx_import_rows WHERE job_uid=?1 AND sheet=?2 AND source_row=?3",params![uid,request.sheet,request.row],|r|Ok((r.get(0)?,r.get(1)?)))?;
        match record {Some(record)=>Ok(json!({"record_uid":record})),None=>{
            let errors:Vec<Value>=decode(&errors)?;
            Err(invalid(errors.iter().filter_map(|e|e["message"].as_str()).collect::<Vec<_>>().join("\n")))
        }}
    })
    }).await;
    publish_live_event(
        "record.imported",
        Some(&actor),
        json!({"module_uid":event_module}),
    );
    match result {
        Ok(Ok(j)) => Json(j).into_response(),
        Ok(Err(e)) => error_response(e),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"response":"Correction paused. Reopen the import log before retrying."})),
        )
            .into_response(),
    }
}
async fn discard(claims: Claims, Path((module, uid)): Path<(String, String)>) -> Response {
    let _lock = WRITE.lock().await;
    db(move|c|{own(c,&claims,&module,&uid)?;c.execute("DELETE FROM mx_import_jobs WHERE uid=?1",[uid])?;Ok(json!({"response":"Import source and log removed. Imported records and their version history are unchanged."}))}).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_counts_rows_and_groups_blockers_without_changing_progress() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE users(uid TEXT PRIMARY KEY);CREATE TABLE mx_modules(uid TEXT PRIMARY KEY);INSERT INTO users VALUES('actor');INSERT INTO mx_modules VALUES('module');").unwrap();
        ensure(&c).unwrap();
        c.execute("INSERT INTO mx_import_jobs(uid,module_uid,actor_uid,filename,fingerprint,sheets_json) VALUES('job','module','actor','file','hash','[]')",[]).unwrap();
        let issue = json!({"field":"Record validation","message":"Department is required."});
        let other = json!({"field":"Control number","message":"Use an integer."});
        for (sheet, row, state, errors) in [
            (0, 2, "pending", json!([issue, issue])),
            (1, 2, "failed", json!([issue])),
            (0, 3, "pending", json!([other])),
            (0, 4, "pending", json!([])),
            (0, 5, "imported", json!([])),
            (0, 6, "excluded", json!([issue])),
        ] {
            c.execute("INSERT INTO mx_import_rows(job_uid,sheet,source_row,cells_json,operation_uid,state,errors_json) VALUES('job',?1,?2,'{}','op',?3,?4)",params![sheet,row,state,errors.to_string()]).unwrap();
        }
        let preview = job_with_validation(&c, "job").unwrap();
        assert_eq!(preview["validation"]["ready"], 1);
        assert_eq!(preview["validation"]["invalid"], 3);
        assert_eq!(preview["validation"]["issues"][0]["rows"], 2);
        assert_eq!(preview["validation"]["issues"][1]["rows"], 1);
        assert_eq!(preview["imported"], 1);
        assert_eq!(preview["failed"], 1);
        assert_eq!(preview["pending"], 3);
        assert_eq!(job(&c, "job").unwrap()["total"], 5);
        c.execute("DELETE FROM mx_import_rows", []).unwrap();
        let empty = job_with_validation(&c, "job").unwrap();
        assert_eq!(
            empty["validation"],
            json!({"ready":0,"invalid":0,"issues":[]})
        );
    }
    #[test]
    fn deleted_module_or_account_does_not_leave_private_source_logs() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys=ON;CREATE TABLE users(uid TEXT PRIMARY KEY);CREATE TABLE mx_modules(uid TEXT PRIMARY KEY);INSERT INTO users VALUES('actor');INSERT INTO mx_modules VALUES('module');").unwrap();
        ensure(&c).unwrap();
        let add = || {
            c.execute("INSERT INTO mx_import_jobs(uid,module_uid,actor_uid,filename,fingerprint,sheets_json) VALUES('job','module','actor','file','hash','[]')",[]).unwrap();
            c.execute("INSERT INTO mx_import_rows(job_uid,sheet,source_row,cells_json,operation_uid) VALUES('job',0,2,'{}','op')",[]).unwrap();
        };
        add();
        c.execute("DELETE FROM mx_modules", []).unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM mx_import_rows", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        c.execute("INSERT INTO mx_modules VALUES('module')", [])
            .unwrap();
        add();
        c.execute("DELETE FROM users", []).unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM mx_import_jobs", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn authority_is_rechecked_inside_each_commit() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE users(uid TEXT,access_level INTEGER,auth_version INTEGER);INSERT INTO users VALUES('editor',2,7);").unwrap();
        assert_eq!(current_actor(&c, "editor", 7).unwrap(), 2);
        c.execute("UPDATE users SET access_level=3", []).unwrap();
        assert!(current_actor(&c, "editor", 7).is_err());
        c.execute("UPDATE users SET access_level=2,auth_version=8", [])
            .unwrap();
        assert!(current_actor(&c, "editor", 7).is_err());
        assert_eq!(current_actor(&c, "editor", 8).unwrap(), 2);
        c.execute("DELETE FROM users", []).unwrap();
        assert!(current_actor(&c, "editor", 8).is_err());
    }
    #[test]
    fn success_marker_rolls_back_with_record_write() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE users(uid TEXT PRIMARY KEY);CREATE TABLE mx_modules(uid TEXT PRIMARY KEY);INSERT INTO users VALUES('actor');INSERT INTO mx_modules VALUES('module');").unwrap();
        ensure(&c).unwrap();
        c.execute("INSERT INTO mx_import_jobs(uid,module_uid,actor_uid,filename,fingerprint,sheets_json) VALUES('job','module','actor','file','hash','[]')",[]).unwrap();
        c.execute("INSERT INTO mx_import_rows(job_uid,sheet,source_row,cells_json,operation_uid,state) VALUES('job',0,2,'{}','operation','pending')",[]).unwrap();
        let ctx = Commit {
            job: "job".into(),
            sheet: 0,
            row: 2,
            auth_version: 1,
        };
        {
            let tx = c.unchecked_transaction().unwrap();
            complete(&tx, &ctx, "record").unwrap();
        }
        let state: String = c
            .query_row("SELECT state FROM mx_import_rows", [], |r| r.get(0))
            .unwrap();
        assert_eq!(state, "pending");
        {
            let tx = c.unchecked_transaction().unwrap();
            complete(&tx, &ctx, "record").unwrap();
            tx.commit().unwrap();
        }
        let record: String = c
            .query_row("SELECT record_uid FROM mx_import_rows", [], |r| r.get(0))
            .unwrap();
        assert_eq!(record, "record");
        assert!(complete(&c, &ctx, "other").is_err());
    }
}
