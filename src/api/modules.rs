use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    api::{
        live::publish_live_event,
        mx::handler::{ensure_mx_record_schema, n1_recover, n1_soft_delete},
    },
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::Claims,
    util::authentication::{CredentialStatus, verify_user_credentials},
};

pub const DEFAULT_MODULE_UID: &str = "mx-default-records";

#[derive(Debug, Clone, Serialize)]
pub struct ModuleDefinition {
    pub uid: String,
    pub slug: String,
    pub name: String,
    pub singular_name: String,
    pub description: String,
    pub icon: String,
    pub color: String,
    pub position: i64,
    pub active: bool,
    pub config: Value,
    pub permissions: Vec<ModulePermission>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModulePermission {
    pub access_level: i64,
    pub can_read: bool,
    pub can_create: bool,
    pub can_update: bool,
    pub can_delete: bool,
    pub can_configure: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateModuleRequest {
    pub name: String,
    pub singular_name: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub color: String,
    pub position: Option<i64>,
    #[serde(default)]
    pub config: Value,
}

#[derive(Debug, Deserialize)]
pub struct UpdateModuleRequest {
    pub name: Option<String>,
    pub singular_name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub position: Option<i64>,
    pub active: Option<bool>,
    pub config: Option<Value>,
    pub permissions: Option<Vec<ModulePermission>>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteModuleRequest {
    pub confirmation: String,
    pub admin_password: String,
    pub admin_otp: Option<String>,
    pub admin_recovery_code: Option<String>,
}

#[derive(Debug)]
struct ModuleDeletionPlan {
    name: String,
    record_count: i64,
    attachment_count: i64,
    object_keys: Vec<String>,
}

#[derive(Debug)]
enum ModuleDeletionPreflight {
    Ready(ModuleDeletionPlan),
    NotFound,
    ConfirmationMismatch,
    Credentials(CredentialStatus),
}

fn table_has_column(
    connection: &rusqlite::Connection,
    table: &str,
    column: &str,
) -> rusqlite::Result<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2)",
            params![table, column],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value != 0)
}

fn table_exists(connection: &rusqlite::Connection, table: &str) -> rusqlite::Result<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            params![table],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value != 0)
}

pub(crate) fn ensure_module_schema(connection: &rusqlite::Connection) -> rusqlite::Result<()> {
    ensure_mx_record_schema(connection)?;
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS mx_modules (
            uid           TEXT PRIMARY KEY NOT NULL,
            slug          TEXT NOT NULL UNIQUE COLLATE NOCASE,
            name          TEXT NOT NULL,
            singular_name TEXT NOT NULL,
            description   TEXT NOT NULL DEFAULT '',
            icon          TEXT NOT NULL DEFAULT 'file-text',
            color         TEXT NOT NULL DEFAULT '#1d4ed8',
            position      INTEGER NOT NULL DEFAULT 0,
            active        INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0,1)),
            config_json   TEXT NOT NULL DEFAULT '{}',
            created_at    INTEGER NOT NULL,
            updated_at    INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS mx_module_permissions (
            module_uid     TEXT NOT NULL,
            access_level   INTEGER NOT NULL,
            can_read       INTEGER NOT NULL DEFAULT 0 CHECK(can_read IN (0,1)),
            can_create     INTEGER NOT NULL DEFAULT 0 CHECK(can_create IN (0,1)),
            can_update     INTEGER NOT NULL DEFAULT 0 CHECK(can_update IN (0,1)),
            can_delete     INTEGER NOT NULL DEFAULT 0 CHECK(can_delete IN (0,1)),
            can_configure  INTEGER NOT NULL DEFAULT 0 CHECK(can_configure IN (0,1)),
            PRIMARY KEY(module_uid, access_level),
            FOREIGN KEY(module_uid) REFERENCES mx_modules(uid) ON DELETE CASCADE
        );
        "#,
    )?;
    let now = chrono::Utc::now().timestamp_millis();
    connection.execute(
        r#"
        INSERT OR IGNORE INTO mx_modules(
            uid, slug, name, singular_name, description, icon, color,
            position, active, config_json, created_at, updated_at
        ) VALUES (?1, 'records', 'Records', 'Record',
            'The original MX record workspace, migrated as the default module.',
            'file-text', '#1d4ed8', 0, 1, '{}', ?2, ?2)
        "#,
        params![DEFAULT_MODULE_UID, now],
    )?;
    for (level, read, create, update, delete, configure) in [
        (0, 1, 1, 1, 1, 1),
        (1, 1, 1, 1, 1, 0),
        (2, 1, 1, 1, 0, 0),
        (3, 1, 0, 0, 0, 0),
    ] {
        connection.execute(
            "INSERT OR IGNORE INTO mx_module_permissions(module_uid, access_level, can_read, can_create, can_update, can_delete, can_configure) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![DEFAULT_MODULE_UID, level, read, create, update, delete, configure],
        )?;
    }

    if !table_has_column(connection, "mx_records", "module_uid")? {
        connection.execute(
            "ALTER TABLE mx_records ADD COLUMN module_uid TEXT NOT NULL DEFAULT 'mx-default-records'",
            [],
        )?;
    }
    connection.execute(
        "UPDATE mx_records SET module_uid = ?1 WHERE module_uid IS NULL OR TRIM(module_uid) = ''",
        params![DEFAULT_MODULE_UID],
    )?;
    connection.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_mx_records_module ON mx_records(module_uid);",
    )?;

    if table_exists(connection, "mx_fields")? {
        if !table_has_column(connection, "mx_fields", "module_uid")? {
            connection.execute(
                "ALTER TABLE mx_fields ADD COLUMN module_uid TEXT NOT NULL DEFAULT 'mx-default-records'",
                [],
            )?;
        }
        if !table_has_column(connection, "mx_fields", "module_key")? {
            connection.execute("ALTER TABLE mx_fields ADD COLUMN module_key TEXT", [])?;
        }
        connection.execute("UPDATE mx_fields SET module_uid = ?1 WHERE module_uid IS NULL OR TRIM(module_uid) = ''", params![DEFAULT_MODULE_UID])?;
        connection.execute("UPDATE mx_fields SET module_key = field_key WHERE module_key IS NULL OR TRIM(module_key) = ''", [])?;
        connection.execute_batch(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_mx_fields_module_key ON mx_fields(module_uid, module_key COLLATE NOCASE);\nCREATE INDEX IF NOT EXISTS idx_mx_fields_module_position ON mx_fields(module_uid, active, position);",
        )?;
    }
    Ok(())
}

fn clean(value: &str, maximum: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(maximum)
        .collect()
}

fn slug(value: &str) -> String {
    let value = value
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let mut result = value
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    result.truncate(48);
    if result.is_empty() {
        "module".to_string()
    } else {
        result
    }
}

fn valid_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn load_permissions(
    connection: &rusqlite::Connection,
    module_uid: &str,
) -> rusqlite::Result<Vec<ModulePermission>> {
    let mut statement = connection.prepare("SELECT access_level, can_read, can_create, can_update, can_delete, can_configure FROM mx_module_permissions WHERE module_uid = ?1 ORDER BY access_level")?;
    statement
        .query_map(params![module_uid], |row| {
            Ok(ModulePermission {
                access_level: row.get(0)?,
                can_read: row.get::<_, i64>(1)? != 0,
                can_create: row.get::<_, i64>(2)? != 0,
                can_update: row.get::<_, i64>(3)? != 0,
                can_delete: row.get::<_, i64>(4)? != 0,
                can_configure: row.get::<_, i64>(5)? != 0,
            })
        })?
        .collect()
}

fn row_to_module(
    connection: &rusqlite::Connection,
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ModuleDefinition> {
    let uid: String = row.get(0)?;
    let text: String = row.get(9)?;
    Ok(ModuleDefinition {
        permissions: load_permissions(connection, &uid)?,
        uid,
        slug: row.get(1)?,
        name: row.get(2)?,
        singular_name: row.get(3)?,
        description: row.get(4)?,
        icon: row.get(5)?,
        color: row.get(6)?,
        position: row.get(7)?,
        active: row.get::<_, i64>(8)? != 0,
        config: serde_json::from_str(&text).unwrap_or_else(|_| json!({})),
    })
}

pub(crate) fn load_module_db(
    connection: &rusqlite::Connection,
    uid: &str,
) -> rusqlite::Result<Option<ModuleDefinition>> {
    ensure_module_schema(connection)?;
    let mut statement = connection.prepare("SELECT uid, slug, name, singular_name, description, icon, color, position, active, config_json FROM mx_modules WHERE uid = ?1")?;
    let mut rows = statement.query(params![uid])?;
    match rows.next()? {
        Some(row) => row_to_module(connection, row).map(Some),
        None => Ok(None),
    }
}

pub(crate) fn module_can(
    connection: &rusqlite::Connection,
    module_uid: &str,
    access_level: i64,
    capability: &str,
) -> rusqlite::Result<bool> {
    ensure_module_schema(connection)?;
    if access_level == 0 {
        return connection
            .query_row(
                "SELECT active FROM mx_modules WHERE uid = ?1",
                params![module_uid],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map(|value| value.unwrap_or(0) != 0);
    }
    let column = match capability {
        "read" => "can_read",
        "create" => "can_create",
        "update" => "can_update",
        "delete" => "can_delete",
        "configure" => "can_configure",
        _ => return Ok(false),
    };
    let sql = format!(
        "SELECT p.{column} FROM mx_module_permissions p JOIN mx_modules m ON m.uid = p.module_uid WHERE p.module_uid = ?1 AND p.access_level = ?2 AND m.active = 1"
    );
    Ok(connection
        .query_row(&sql, params![module_uid, access_level], |row| {
            row.get::<_, i64>(0)
        })
        .optional()?
        .unwrap_or(0)
        != 0)
}

fn api_json(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

pub async fn list_modules(claims: Claims) -> Response {
    let access_level = claims.access_level;
    let result = tokio::task::spawn_blocking(move || with_sql_connection(|connection| {
        ensure_module_schema(connection)?;
        let sql = "SELECT m.uid, m.slug, m.name, m.singular_name, m.description, m.icon, m.color, m.position, m.active, m.config_json FROM mx_modules m JOIN mx_module_permissions p ON p.module_uid = m.uid AND p.access_level = ?1 WHERE (p.can_read = 1 OR p.can_configure = 1) AND (m.active = 1 OR ?1 = 0) ORDER BY m.active DESC, m.position, m.name COLLATE NOCASE";
        let mut statement = connection.prepare(sql)?;
        let mut rows = statement.query(params![access_level])?;
        let mut modules = Vec::new();
        while let Some(row) = rows.next()? { modules.push(row_to_module(connection, row)?); }
        Ok(modules)
    })).await;
    match result {
        Ok(Ok(modules)) => api_json(StatusCode::OK, json!({"modules":modules})),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to load modules"}),
        ),
    }
}

pub async fn create_module(claims: Claims, Json(request): Json<CreateModuleRequest>) -> Response {
    if !claims.can_manage_accounts() {
        return api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"administrator access is required"}),
        );
    }
    let name = clean(&request.name, 80);
    if name.is_empty() {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"module name is required"}),
        );
    }
    let singular = request
        .singular_name
        .as_deref()
        .map(|value| clean(value, 80))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| name.trim_end_matches('s').to_string());
    let description = clean(&request.description, 500);
    let icon = {
        let value = clean(&request.icon, 12);
        if value.is_empty() {
            "file-text".to_string()
        } else {
            value
        }
    };
    let color = if valid_color(&request.color) {
        request.color.to_ascii_lowercase()
    } else {
        "#1d4ed8".to_string()
    };
    let base_slug = slug(&name);
    let uid = Uuid::new_v4().to_string();
    let event_uid = uid.clone();
    let actor_uid = claims.uid;
    let config = if request.config.is_object() {
        request.config
    } else {
        json!({})
    };
    let config_text = config.to_string();
    let position = request.position;
    let result = tokio::task::spawn_blocking(move || -> Result<ModuleDefinition, SqliteDatabaseError> { with_sql_connection(|connection| {
        ensure_module_schema(connection)?; let transaction = connection.unchecked_transaction()?; let now = chrono::Utc::now().timestamp_millis();
        let mut candidate = base_slug.clone(); let mut suffix = 2; while transaction.query_row("SELECT EXISTS(SELECT 1 FROM mx_modules WHERE slug = ?1)", params![candidate], |row| row.get::<_,bool>(0))? { candidate = format!("{base_slug}-{suffix}"); suffix += 1; }
        let position = position.unwrap_or(transaction.query_row("SELECT COALESCE(MAX(position), -10) + 10 FROM mx_modules", [], |row| row.get(0))?);
        transaction.execute("INSERT INTO mx_modules(uid,slug,name,singular_name,description,icon,color,position,active,config_json,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,1,?9,?10,?10)", params![uid,candidate,name,singular,description,icon,color,position,config_text,now])?;
        for (level, read, create, update, delete, configure) in [(0,1,1,1,1,1),(1,1,1,1,1,0),(2,1,1,1,0,0),(3,1,0,0,0,0)] { transaction.execute("INSERT INTO mx_module_permissions(module_uid,access_level,can_read,can_create,can_update,can_delete,can_configure) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![uid,level,read,create,update,delete,configure])?; }
        transaction.commit()?; load_module_db(connection, &uid)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
    })}).await;
    match result {
        Ok(Ok(module)) => {
            publish_live_event(
                "module.created",
                Some(&actor_uid),
                json!({"module_uid":event_uid}),
            );
            api_json(StatusCode::CREATED, json!({"module":module}))
        }
        Ok(Err(error)) => {
            crate::report_error!(format!("{error}"), "modules", "create_module()");
            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"failed to create module"}),
            )
        }
        Err(_) => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"module creation task failed"}),
        ),
    }
}

pub async fn update_module(
    claims: Claims,
    Path(uid): Path<String>,
    Json(request): Json<UpdateModuleRequest>,
) -> Response {
    if !claims.can_manage_accounts() {
        return api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"administrator access is required"}),
        );
    }
    if uid == DEFAULT_MODULE_UID && request.active == Some(false) {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"the default migrated module cannot be archived"}),
        );
    }
    let db_uid = uid.clone();
    let actor_uid = claims.uid;
    let result = tokio::task::spawn_blocking(move || -> Result<ModuleDefinition, SqliteDatabaseError> { with_sql_connection(|connection| {
        ensure_module_schema(connection)?; let existing = load_module_db(connection,&db_uid)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?; let transaction = connection.unchecked_transaction()?;
        let name = request.name.as_deref().map(|value| clean(value,80)).filter(|value| !value.is_empty()).unwrap_or(existing.name);
        let singular = request.singular_name.as_deref().map(|value| clean(value,80)).filter(|value| !value.is_empty()).unwrap_or(existing.singular_name);
        let description = request.description.as_deref().map(|value| clean(value,500)).unwrap_or(existing.description); let icon = request.icon.as_deref().map(|value| clean(value,12)).filter(|value| !value.is_empty()).unwrap_or(existing.icon);
        let color = request.color.filter(|value| valid_color(value)).unwrap_or(existing.color); let config = request.config.filter(Value::is_object).unwrap_or(existing.config);
        transaction.execute("UPDATE mx_modules SET name=?2,singular_name=?3,description=?4,icon=?5,color=?6,position=?7,active=?8,config_json=?9,updated_at=?10 WHERE uid=?1", params![db_uid,name,singular,description,icon,color,request.position.unwrap_or(existing.position),if request.active.unwrap_or(existing.active){1}else{0},config.to_string(),chrono::Utc::now().timestamp_millis()])?;
        if let Some(permissions) = request.permissions { for permission in permissions { if !(0..=3).contains(&permission.access_level) { continue; } let administrator = permission.access_level == 0; transaction.execute("INSERT INTO mx_module_permissions(module_uid,access_level,can_read,can_create,can_update,can_delete,can_configure) VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(module_uid,access_level) DO UPDATE SET can_read=excluded.can_read,can_create=excluded.can_create,can_update=excluded.can_update,can_delete=excluded.can_delete,can_configure=excluded.can_configure", params![db_uid,permission.access_level,(administrator || permission.can_read) as i64,(administrator || permission.can_create) as i64,(administrator || permission.can_update) as i64,(administrator || permission.can_delete) as i64,(administrator || permission.can_configure) as i64])?; } }
        transaction.commit()?; load_module_db(connection,&db_uid)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
    })}).await;
    match result {
        Ok(Ok(module)) => {
            publish_live_event(
                "module.updated",
                Some(&actor_uid),
                json!({"module_uid":uid}),
            );
            api_json(StatusCode::OK, json!({"module":module}))
        }
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => api_json(
            StatusCode::NOT_FOUND,
            json!({"response":"module was not found"}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to update module"}),
        ),
    }
}

async fn recover_module_objects(object_keys: &[String]) -> bool {
    let mut recovered = true;
    for object_key in object_keys.iter().rev() {
        if n1_recover(object_key).await.is_err() {
            recovered = false;
        }
    }
    recovered
}

pub async fn delete_module(
    claims: Claims,
    Path(uid): Path<String>,
    Json(request): Json<DeleteModuleRequest>,
) -> Response {
    if !claims.can_manage_accounts() {
        return api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"administrator access is required"}),
        );
    }
    if uid == DEFAULT_MODULE_UID {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"the core migrated module cannot be deleted; create another module and archive unused fields instead"}),
        );
    }

    let admin_uid = claims.uid.clone();
    let preflight_uid = uid.clone();
    let preflight = tokio::task::spawn_blocking(move || -> Result<ModuleDeletionPreflight, SqliteDatabaseError> {
        with_sql_connection(|connection| {
            ensure_module_schema(connection)?;
            let transaction = connection.unchecked_transaction()?;
            let module_name = transaction
                .query_row(
                    "SELECT name FROM mx_modules WHERE uid = ?1",
                    params![&preflight_uid],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            let Some(module_name) = module_name else {
                return Ok(ModuleDeletionPreflight::NotFound);
            };
            if request.confirmation.trim() != module_name {
                return Ok(ModuleDeletionPreflight::ConfirmationMismatch);
            }

            let credential_status = verify_user_credentials(
                &transaction,
                &admin_uid,
                &request.admin_password,
                request.admin_otp.as_deref(),
                request.admin_recovery_code.as_deref(),
            )?;
            if credential_status != CredentialStatus::Valid {
                return Ok(ModuleDeletionPreflight::Credentials(credential_status));
            }

            let record_count = transaction.query_row(
                "SELECT COUNT(*) FROM mx_records WHERE module_uid = ?1",
                params![&preflight_uid],
                |row| row.get::<_, i64>(0),
            )?;
            let mut object_keys = Vec::new();
            if table_exists(&transaction, "mx_attachments")? {
                let mut statement = transaction.prepare(
                    "SELECT attachment.object_key FROM mx_attachments attachment JOIN mx_records record ON record.uid = attachment.entry_uid WHERE record.module_uid = ?1",
                )?;
                let keys = statement.query_map(params![&preflight_uid], |row| row.get::<_, String>(0))?;
                for key in keys { object_keys.push(key?); }
            }
            if table_exists(&transaction, "mx_record_versions")?
                && table_exists(&transaction, "mx_record_version_attachments")?
            {
                let mut statement = transaction.prepare(
                    "SELECT attachment.object_key FROM mx_record_version_attachments attachment JOIN mx_record_versions version ON version.uid = attachment.version_uid WHERE version.module_uid = ?1",
                )?;
                let keys = statement.query_map(params![&preflight_uid], |row| row.get::<_, String>(0))?;
                for key in keys { object_keys.push(key?); }
            }
            object_keys.sort();
            object_keys.dedup();
            let attachment_count = object_keys.len() as i64;
            transaction.commit()?;
            Ok(ModuleDeletionPreflight::Ready(ModuleDeletionPlan {
                name: module_name,
                record_count,
                attachment_count,
                object_keys,
            }))
        })
    }).await;

    let plan = match preflight {
        Ok(Ok(ModuleDeletionPreflight::Ready(plan))) => plan,
        Ok(Ok(ModuleDeletionPreflight::NotFound)) => {
            return api_json(
                StatusCode::NOT_FOUND,
                json!({"response":"module was not found"}),
            );
        }
        Ok(Ok(ModuleDeletionPreflight::ConfirmationMismatch)) => {
            return api_json(
                StatusCode::BAD_REQUEST,
                json!({"response":"type the exact module name to confirm deletion"}),
            );
        }
        Ok(Ok(ModuleDeletionPreflight::Credentials(CredentialStatus::SecondFactorRequired))) => {
            return api_json(
                StatusCode::BAD_REQUEST,
                json!({"response":"administrator authenticator or recovery code is required"}),
            );
        }
        Ok(Ok(ModuleDeletionPreflight::Credentials(_))) => {
            return api_json(
                StatusCode::UNAUTHORIZED,
                json!({"response":"administrator re-authentication failed"}),
            );
        }
        Ok(Err(error)) => {
            crate::report_error!(format!("{error}"), "modules", "delete_module_preflight()");
            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"failed to authorize module deletion"}),
            );
        }
        Err(error) => {
            crate::report_error!(format!("{error}"), "modules", "delete_module_preflight()");
            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"module deletion task failed"}),
            );
        }
    };

    let mut moved_objects = Vec::new();
    for object_key in &plan.object_keys {
        if let Err(error) = n1_soft_delete(object_key).await {
            let recovered = recover_module_objects(&moved_objects).await;
            crate::report_error!(format!("{error:?}"), "modules", "delete_module_n1()");
            return api_json(
                StatusCode::BAD_GATEWAY,
                json!({"response": if recovered { "N1 could not move every attachment to trash; the module was not deleted" } else { "N1 could not move every attachment to trash; the module was not deleted, but some attachment objects need manual recovery in N1" }}),
            );
        }
        moved_objects.push(object_key.clone());
    }

    let delete_uid = uid.clone();
    let deletion = tokio::task::spawn_blocking(move || -> Result<bool, SqliteDatabaseError> {
        with_sql_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            if table_exists(&transaction, "mx_notifications")? {
                transaction.execute("DELETE FROM mx_notifications WHERE module_uid = ?1", params![&delete_uid])?;
            }
            if table_exists(&transaction, "mx_message_record_links")? {
                transaction.execute("DELETE FROM mx_message_record_links WHERE record_uid IN (SELECT uid FROM mx_records WHERE module_uid = ?1)", params![&delete_uid])?;
            }
            if table_exists(&transaction, "mx_record_versions")? {
                transaction.execute("DELETE FROM mx_record_versions WHERE module_uid = ?1", params![&delete_uid])?;
            }
            if table_exists(&transaction, "mx_record_field_revisions")? {
                transaction.execute("DELETE FROM mx_record_field_revisions WHERE record_uid IN (SELECT uid FROM mx_records WHERE module_uid = ?1)", params![&delete_uid])?;
            }
            transaction.execute("DELETE FROM mx_records WHERE module_uid = ?1", params![&delete_uid])?;

            if table_exists(&transaction, "mx_module_storage_layout_folders")? {
                transaction.execute("DELETE FROM mx_module_storage_layout_folders WHERE module_uid = ?1", params![&delete_uid])?;
            }
            if table_exists(&transaction, "mx_module_storage_layout_meta")? {
                transaction.execute("DELETE FROM mx_module_storage_layout_meta WHERE module_uid = ?1", params![&delete_uid])?;
            }
            if table_exists(&transaction, "mx_field_sequences")? {
                transaction.execute("DELETE FROM mx_field_sequences WHERE field_uid IN (SELECT uid FROM mx_fields WHERE module_uid = ?1)", params![&delete_uid])?;
            }
            if table_exists(&transaction, "mx_unique_values")? {
                transaction.execute("DELETE FROM mx_unique_values WHERE field_uid IN (SELECT uid FROM mx_fields WHERE module_uid = ?1)", params![&delete_uid])?;
            }
            if table_exists(&transaction, "mx_record_values")? {
                transaction.execute("DELETE FROM mx_record_values WHERE field_uid IN (SELECT uid FROM mx_fields WHERE module_uid = ?1)", params![&delete_uid])?;
            }
            if table_exists(&transaction, "mx_fields")? {
                transaction.execute("DELETE FROM mx_fields WHERE module_uid = ?1", params![&delete_uid])?;
            }

            if table_exists(&transaction, "mx_dashboard_config")? {
                let config_text = transaction
                    .query_row("SELECT config_json FROM mx_dashboard_config WHERE id = 1", [], |row| row.get::<_, String>(0))
                    .optional()?;
                if let Some(config_text) = config_text {
                    if let Ok(mut config) = serde_json::from_str::<Value>(&config_text) {
                        if let Some(widgets) = config.get_mut("widgets").and_then(Value::as_array_mut) {
                            let previous = widgets.len();
                            widgets.retain(|widget| widget.get("module_uid").and_then(Value::as_str) != Some(delete_uid.as_str()));
                            if widgets.len() != previous {
                                transaction.execute(
                                    "UPDATE mx_dashboard_config SET config_json = ?1, revision = revision + 1, updated_at = ?2 WHERE id = 1",
                                    params![config.to_string(), chrono::Utc::now().timestamp_millis()],
                                )?;
                            }
                        }
                    }
                }
            }

            let deleted = transaction.execute("DELETE FROM mx_modules WHERE uid = ?1", params![&delete_uid])?;
            transaction.commit()?;
            Ok(deleted == 1)
        })
    }).await;

    match deletion {
        Ok(Ok(true)) => {
            publish_live_event(
                "module.deleted",
                Some(&claims.uid),
                json!({"module_uid":uid}),
            );
            api_json(
                StatusCode::OK,
                json!({
                    "response":"module deleted",
                    "module_uid":uid,
                    "module_name":plan.name,
                    "records_deleted":plan.record_count,
                    "attachments_moved_to_n1_trash":plan.attachment_count
                }),
            )
        }
        Ok(Ok(false)) => {
            let _ = recover_module_objects(&moved_objects).await;
            api_json(
                StatusCode::NOT_FOUND,
                json!({"response":"module was not found"}),
            )
        }
        Ok(Err(error)) => {
            let recovered = recover_module_objects(&moved_objects).await;
            crate::report_error!(format!("{error}"), "modules", "delete_module()");
            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response": if recovered { "module deletion failed; N1 attachments were restored" } else { "module deletion failed; some attachment objects need manual recovery in N1" }}),
            )
        }
        Err(error) => {
            let recovered = recover_module_objects(&moved_objects).await;
            crate::report_error!(format!("{error}"), "modules", "delete_module()");
            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response": if recovered { "module deletion task failed; N1 attachments were restored" } else { "module deletion task failed; some attachment objects need manual recovery in N1" }}),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mx::schema::ensure_dynamic_schema;

    fn administrator_claims() -> Claims {
        Claims {
            uid: "administrator-1".to_string(),
            email: "administrator@mx.local".to_string(),
            access_level: 0,
            auth_version: 0,
            token_kind: "access".to_string(),
            exp: usize::MAX,
        }
    }

    #[test]
    fn module_migration_assigns_legacy_records_and_fields() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();
        ensure_dynamic_schema(&connection).unwrap();

        let record_has_module: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('mx_records') WHERE name = 'module_uid')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let field_has_module: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('mx_fields') WHERE name = 'module_uid')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(record_has_module && field_has_module);
        assert!(
            load_module_db(&connection, DEFAULT_MODULE_UID)
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn core_module_cannot_be_deleted_even_with_an_administrator_request() {
        let response = delete_module(
            administrator_claims(),
            Path(DEFAULT_MODULE_UID.to_string()),
            Json(DeleteModuleRequest {
                confirmation: "Records".to_string(),
                admin_password: "not-used".to_string(),
                admin_otp: None,
                admin_recovery_code: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
