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
    api::{live::publish_live_event, mx::handler::ensure_mx_record_schema},
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::Claims,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mx::schema::ensure_dynamic_schema;

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
}
