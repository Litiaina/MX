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
        live::{publish_live_event, publish_user_event},
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_permission: Option<ModulePermission>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModulePermission {
    pub access_level: i64,
    pub can_read: bool,
    pub can_create: bool,
    pub can_update: bool,
    pub can_delete: bool,
    pub can_configure: bool,
    #[serde(default)]
    pub can_report: bool,
    #[serde(default)]
    pub can_attachments: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountModuleGrant {
    pub module_uid: String,
    pub can_read: bool,
    pub can_create: bool,
    pub can_update: bool,
    pub can_delete: bool,
    pub can_configure: bool,
    pub can_report: bool,
    pub can_attachments: bool,
}

#[derive(Debug, Deserialize)]
pub struct SaveAccountModuleGrantsRequest {
    pub revision: i64,
    #[serde(default)]
    pub grants: Vec<AccountModuleGrant>,
}

#[derive(Debug, Serialize)]
pub struct AccountModuleGrantView {
    pub module_uid: String,
    pub module_name: String,
    pub module_icon: String,
    pub module_color: String,
    pub module_active: bool,
    pub role_permission: ModulePermission,
    pub grant: AccountModuleGrant,
}

#[derive(Debug)]
enum SaveAccountGrantsOutcome {
    Saved(i64),
    RevisionConflict(i64),
    AccountNotFound,
    Administrator,
}

fn claim_module_access_revision(
    connection: &rusqlite::Connection,
    user_uid: &str,
    expected_revision: i64,
) -> rusqlite::Result<Result<i64, i64>> {
    let next = connection
        .query_row(
            "UPDATE mx_module_access_revisions SET revision=revision+1 WHERE user_uid=?1 AND revision=?2 RETURNING revision",
            params![user_uid,expected_revision],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    if let Some(next) = next {
        return Ok(Ok(next));
    }
    let current = connection.query_row(
        "SELECT revision FROM mx_module_access_revisions WHERE user_uid=?1",
        params![user_uid],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(Err(current))
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

        CREATE TABLE IF NOT EXISTS mx_module_account_permissions (
            module_uid      TEXT NOT NULL,
            user_uid        TEXT NOT NULL,
            can_read        INTEGER NOT NULL DEFAULT 0 CHECK(can_read IN (0,1)),
            can_create      INTEGER NOT NULL DEFAULT 0 CHECK(can_create IN (0,1)),
            can_update      INTEGER NOT NULL DEFAULT 0 CHECK(can_update IN (0,1)),
            can_delete      INTEGER NOT NULL DEFAULT 0 CHECK(can_delete IN (0,1)),
            can_configure   INTEGER NOT NULL DEFAULT 0 CHECK(can_configure IN (0,1)),
            can_report      INTEGER NOT NULL DEFAULT 0 CHECK(can_report IN (0,1)),
            can_attachments INTEGER NOT NULL DEFAULT 0 CHECK(can_attachments IN (0,1)),
            updated_by      TEXT,
            updated_at      INTEGER NOT NULL,
            PRIMARY KEY(module_uid, user_uid),
            FOREIGN KEY(module_uid) REFERENCES mx_modules(uid) ON DELETE CASCADE,
            FOREIGN KEY(user_uid) REFERENCES users(uid) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS mx_module_access_revisions (
            user_uid TEXT PRIMARY KEY NOT NULL,
            revision INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY(user_uid) REFERENCES users(uid) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS mx_module_access_meta (
            meta_key TEXT PRIMARY KEY NOT NULL,
            meta_value TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_mx_module_account_permissions_user
        ON mx_module_account_permissions(user_uid, module_uid);
        "#,
    )?;
    if !table_has_column(connection, "mx_module_permissions", "can_report")? {
        connection.execute(
            "ALTER TABLE mx_module_permissions ADD COLUMN can_report INTEGER NOT NULL DEFAULT 1 CHECK(can_report IN (0,1))",
            [],
        )?;
    }
    if !table_has_column(connection, "mx_module_permissions", "can_attachments")? {
        connection.execute(
            "ALTER TABLE mx_module_permissions ADD COLUMN can_attachments INTEGER NOT NULL DEFAULT 1 CHECK(can_attachments IN (0,1))",
            [],
        )?;
    }
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
            "INSERT OR IGNORE INTO mx_module_permissions(module_uid, access_level, can_read, can_create, can_update, can_delete, can_configure, can_report, can_attachments) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?3, ?3)",
            params![DEFAULT_MODULE_UID, level, read, create, update, delete, configure],
        )?;
    }

    let account_grants_migrated = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM mx_module_access_meta WHERE meta_key='account_grants_v1')",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    // Some focused tests and older installations can expose a legacy `users`
    // table before the access-level migration has run.  Seeding must be
    // schema-aware: merely finding a table called `users` is not enough.
    if !account_grants_migrated
        && table_exists(connection, "users")?
        && table_has_column(connection, "users", "uid")?
        && table_has_column(connection, "users", "access_level")?
    {
        let transaction = connection.unchecked_transaction()?;
        transaction.execute_batch(
            r#"
            INSERT OR IGNORE INTO mx_module_account_permissions(
                module_uid,user_uid,can_read,can_create,can_update,can_delete,
                can_configure,can_report,can_attachments,updated_by,updated_at
            )
            SELECT permission.module_uid,user.uid,permission.can_read,permission.can_create,
                   permission.can_update,permission.can_delete,permission.can_configure,
                   permission.can_report,permission.can_attachments,NULL,
                   CAST(strftime('%s','now') AS INTEGER) * 1000
            FROM users user
            JOIN mx_module_permissions permission ON permission.access_level=user.access_level
            WHERE user.access_level<>0;

            INSERT OR IGNORE INTO mx_module_access_revisions(user_uid, revision)
            SELECT uid, 0 FROM users WHERE access_level<>0;

            INSERT INTO mx_module_access_meta(meta_key,meta_value)
            VALUES('account_grants_v1','seeded-existing-accounts');
            "#,
        )?;
        transaction.commit()?;
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
    let mut statement = connection.prepare("SELECT access_level, can_read, can_create, can_update, can_delete, can_configure, can_report, can_attachments FROM mx_module_permissions WHERE module_uid = ?1 ORDER BY access_level")?;
    statement
        .query_map(params![module_uid], |row| {
            Ok(ModulePermission {
                access_level: row.get(0)?,
                can_read: row.get::<_, i64>(1)? != 0,
                can_create: row.get::<_, i64>(2)? != 0,
                can_update: row.get::<_, i64>(3)? != 0,
                can_delete: row.get::<_, i64>(4)? != 0,
                can_configure: row.get::<_, i64>(5)? != 0,
                can_report: row.get::<_, i64>(6)? != 0,
                can_attachments: row.get::<_, i64>(7)? != 0,
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
        effective_permission: None,
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

pub(crate) fn effective_module_permission(
    connection: &rusqlite::Connection,
    module_uid: &str,
    user_uid: &str,
    access_level: i64,
) -> rusqlite::Result<Option<ModulePermission>> {
    ensure_module_schema(connection)?;
    if access_level == 0 {
        let active = connection
            .query_row(
                "SELECT active FROM mx_modules WHERE uid=?1",
                params![module_uid],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0)
            != 0;
        return Ok(active.then_some(ModulePermission {
            access_level,
            can_read: true,
            can_create: true,
            can_update: true,
            can_delete: true,
            can_configure: true,
            can_report: true,
            can_attachments: true,
        }));
    }
    connection
        .query_row(
            r#"
            SELECT
                role.can_read AND account.can_read,
                role.can_create AND account.can_create,
                role.can_update AND account.can_update,
                role.can_delete AND account.can_delete,
                role.can_configure AND account.can_configure,
                role.can_report AND account.can_report,
                role.can_attachments AND account.can_attachments
            FROM mx_modules module
            JOIN mx_module_permissions role
              ON role.module_uid=module.uid AND role.access_level=?3
            JOIN mx_module_account_permissions account
              ON account.module_uid=module.uid AND account.user_uid=?2
            WHERE module.uid=?1 AND module.active=1
            "#,
            params![module_uid, user_uid, access_level],
            |row| {
                Ok(ModulePermission {
                    access_level,
                    can_read: row.get::<_, i64>(0)? != 0,
                    can_create: row.get::<_, i64>(1)? != 0,
                    can_update: row.get::<_, i64>(2)? != 0,
                    can_delete: row.get::<_, i64>(3)? != 0,
                    can_configure: row.get::<_, i64>(4)? != 0,
                    can_report: row.get::<_, i64>(5)? != 0,
                    can_attachments: row.get::<_, i64>(6)? != 0,
                })
            },
        )
        .optional()
}

pub(crate) fn module_can_for_user(
    connection: &rusqlite::Connection,
    module_uid: &str,
    user_uid: &str,
    access_level: i64,
    capability: &str,
) -> rusqlite::Result<bool> {
    let Some(permission) =
        effective_module_permission(connection, module_uid, user_uid, access_level)?
    else {
        return Ok(false);
    };
    let allowed = match capability {
        "read" => permission.can_read,
        "create" => permission.can_read && permission.can_create,
        "update" => permission.can_read && permission.can_update,
        "delete" => permission.can_read && permission.can_delete,
        "configure" => permission.can_configure,
        "report" => permission.can_read && permission.can_report,
        "attachments" => permission.can_read && permission.can_attachments,
        _ => false,
    };
    Ok(allowed)
}

fn api_json(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

pub async fn list_modules(claims: Claims) -> Response {
    let access_level = claims.access_level;
    let user_uid = claims.uid;
    let result = tokio::task::spawn_blocking(move || with_sql_connection(|connection| {
        ensure_module_schema(connection)?;
        let sql = "SELECT uid, slug, name, singular_name, description, icon, color, position, active, config_json FROM mx_modules WHERE active=1 OR ?1=0 ORDER BY active DESC, position, name COLLATE NOCASE";
        let mut statement = connection.prepare(sql)?;
        let mut rows = statement.query(params![access_level])?;
        let mut modules = Vec::new();
        while let Some(row) = rows.next()? {
            modules.push(row_to_module(connection, row)?);
        }
        drop(rows);
        drop(statement);

        let mut visible_modules = Vec::with_capacity(modules.len());
        for mut module in modules {
            if access_level == 0 {
                module.effective_permission = Some(ModulePermission {
                    access_level,
                    can_read: true,
                    can_create: true,
                    can_update: true,
                    can_delete: true,
                    can_configure: true,
                    can_report: true,
                    can_attachments: true,
                });
                visible_modules.push(module);
                continue;
            }
            let permission = effective_module_permission(connection, &module.uid, &user_uid, access_level)?;
            if permission.as_ref().is_some_and(|item| item.can_read || item.can_configure) {
                module.effective_permission = permission;
                visible_modules.push(module);
            }
        }
        Ok(visible_modules)
    })).await;
    match result {
        Ok(Ok(modules)) => api_json(StatusCode::OK, json!({"modules":modules})),
        Ok(Err(error)) => {
            crate::report_error!(format!("{error}"), "modules", "list_modules()");
            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"failed to load modules"}),
            )
        }
        Err(error) => {
            crate::report_error!(format!("{error}"), "modules", "list_modules()");
            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"module list task failed"}),
            )
        }
    }
}

pub async fn get_account_module_grants(claims: Claims, Path(user_uid): Path<String>) -> Response {
    if !claims.can_manage_accounts() {
        return api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"administrator access is required"}),
        );
    }
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_module_schema(connection)?;
            let account = connection
                .query_row(
                    "SELECT name, access_level FROM users WHERE uid=?1",
                    params![user_uid],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
                )
                .optional()?;
            let Some((account_name, access_level)) = account else {
                return Ok(None);
            };
            connection.execute(
                "INSERT OR IGNORE INTO mx_module_access_revisions(user_uid,revision) VALUES(?1,0)",
                params![user_uid],
            )?;
            let revision = connection.query_row(
                "SELECT revision FROM mx_module_access_revisions WHERE user_uid=?1",
                params![user_uid],
                |row| row.get::<_, i64>(0),
            )?;
            let mut statement = connection.prepare(
                "SELECT uid,name,icon,color,active FROM mx_modules ORDER BY active DESC,position,name COLLATE NOCASE",
            )?;
            let modules = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)? != 0,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let mut views = Vec::with_capacity(modules.len());
            for (module_uid, module_name, module_icon, module_color, module_active) in modules {
                let role_permission = load_permissions(connection, &module_uid)?
                    .into_iter()
                    .find(|permission| permission.access_level == access_level)
                    .unwrap_or(ModulePermission {
                        access_level,
                        can_read: false,
                        can_create: false,
                        can_update: false,
                        can_delete: false,
                        can_configure: false,
                        can_report: false,
                        can_attachments: false,
                    });
                let grant = connection
                    .query_row(
                        "SELECT can_read,can_create,can_update,can_delete,can_configure,can_report,can_attachments FROM mx_module_account_permissions WHERE module_uid=?1 AND user_uid=?2",
                        params![module_uid,user_uid],
                        |row| Ok(AccountModuleGrant {
                            module_uid: module_uid.clone(),
                            can_read: row.get::<_,i64>(0)? != 0,
                            can_create: row.get::<_,i64>(1)? != 0,
                            can_update: row.get::<_,i64>(2)? != 0,
                            can_delete: row.get::<_,i64>(3)? != 0,
                            can_configure: row.get::<_,i64>(4)? != 0,
                            can_report: row.get::<_,i64>(5)? != 0,
                            can_attachments: row.get::<_,i64>(6)? != 0,
                        }),
                    )
                    .optional()?
                    .unwrap_or(AccountModuleGrant {
                        module_uid: module_uid.clone(),
                        can_read: false,
                        can_create: false,
                        can_update: false,
                        can_delete: false,
                        can_configure: false,
                        can_report: false,
                        can_attachments: false,
                    });
                views.push(AccountModuleGrantView {
                    module_uid,
                    module_name,
                    module_icon,
                    module_color,
                    module_active,
                    role_permission,
                    grant,
                });
            }
            Ok(Some((account_name, access_level, revision, views)))
        })
    })
    .await;
    match result {
        Ok(Ok(Some((account_name, access_level, revision, modules)))) => api_json(
            StatusCode::OK,
            json!({"account_name":account_name,"access_level":access_level,"revision":revision,"modules":modules}),
        ),
        Ok(Ok(None)) => api_json(
            StatusCode::NOT_FOUND,
            json!({"response":"account was not found"}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"module access could not be loaded"}),
        ),
    }
}

pub async fn save_account_module_grants(
    claims: Claims,
    Path(user_uid): Path<String>,
    Json(request): Json<SaveAccountModuleGrantsRequest>,
) -> Response {
    if !claims.can_manage_accounts() {
        return api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"administrator access is required"}),
        );
    }
    if request.revision < 0 || request.grants.len() > 512 {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"the module access request is invalid"}),
        );
    }
    let unique = request
        .grants
        .iter()
        .map(|grant| grant.module_uid.trim())
        .collect::<std::collections::HashSet<_>>();
    if unique.len() != request.grants.len() || unique.iter().any(|uid| uid.is_empty()) {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"each module may appear only once"}),
        );
    }
    let actor_uid = claims.uid;
    let event_actor_uid = actor_uid.clone();
    let event_user_uid = user_uid.clone();
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_module_schema(connection)?;
            let account_level = connection
                .query_row(
                    "SELECT access_level FROM users WHERE uid=?1",
                    params![user_uid],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            let Some(access_level) = account_level else {
                return Ok(SaveAccountGrantsOutcome::AccountNotFound);
            };
            if access_level == 0 {
                return Ok(SaveAccountGrantsOutcome::Administrator);
            }
            let transaction = connection.unchecked_transaction()?;
            transaction.execute(
                "INSERT OR IGNORE INTO mx_module_access_revisions(user_uid,revision) VALUES(?1,0)",
                params![user_uid],
            )?;
            let next_revision = match claim_module_access_revision(
                &transaction,
                &user_uid,
                request.revision,
            )? {
                Ok(revision) => revision,
                Err(current) => {
                    return Ok(SaveAccountGrantsOutcome::RevisionConflict(current));
                }
            };
            transaction.execute(
                "DELETE FROM mx_module_account_permissions WHERE user_uid=?1",
                params![user_uid],
            )?;
            let now = chrono::Utc::now().timestamp_millis();
            for grant in request.grants {
                let module_uid = grant.module_uid.trim();
                let role = transaction
                    .query_row(
                        "SELECT can_read,can_create,can_update,can_delete,can_configure,can_report,can_attachments FROM mx_module_permissions WHERE module_uid=?1 AND access_level=?2",
                        params![module_uid,access_level],
                        |row| Ok((
                            row.get::<_,i64>(0)? != 0,row.get::<_,i64>(1)? != 0,
                            row.get::<_,i64>(2)? != 0,row.get::<_,i64>(3)? != 0,
                            row.get::<_,i64>(4)? != 0,row.get::<_,i64>(5)? != 0,
                            row.get::<_,i64>(6)? != 0,
                        )),
                    )
                    .optional()?;
                let Some(role) = role else { continue; };
                let values = (
                    grant.can_read && role.0,
                    grant.can_create && role.1,
                    grant.can_update && role.2,
                    grant.can_delete && role.3,
                    grant.can_configure && role.4,
                    grant.can_report && role.5,
                    grant.can_attachments && role.6,
                );
                if !values.0 && !values.4 { continue; }
                transaction.execute(
                    "INSERT INTO mx_module_account_permissions(module_uid,user_uid,can_read,can_create,can_update,can_delete,can_configure,can_report,can_attachments,updated_by,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                    params![module_uid,user_uid,values.0 as i64,values.1 as i64,values.2 as i64,values.3 as i64,values.4 as i64,values.5 as i64,values.6 as i64,actor_uid,now],
                )?;
            }
            transaction.commit()?;
            Ok(SaveAccountGrantsOutcome::Saved(next_revision))
        })
    })
    .await;
    match result {
        Ok(Ok(SaveAccountGrantsOutcome::Saved(revision))) => {
            publish_user_event(
                &event_user_uid,
                "module.access.changed",
                Some(&event_actor_uid),
                json!({"user_uid":event_user_uid.clone(),"revision":revision}),
            );
            api_json(
                StatusCode::OK,
                json!({"response":"module access saved","revision":revision}),
            )
        }
        Ok(Ok(SaveAccountGrantsOutcome::RevisionConflict(revision))) => api_json(
            StatusCode::CONFLICT,
            json!({"response":"Module access changed in another administrator session. Reload and review before saving.","revision":revision}),
        ),
        Ok(Ok(SaveAccountGrantsOutcome::AccountNotFound)) => api_json(
            StatusCode::NOT_FOUND,
            json!({"response":"account was not found"}),
        ),
        Ok(Ok(SaveAccountGrantsOutcome::Administrator)) => api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"administrators always have access to every active module"}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"module access could not be saved"}),
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
        for (level, read, create, update, delete, configure) in [(0,1,1,1,1,1),(1,1,1,1,1,0),(2,1,1,1,0,0),(3,1,0,0,0,0)] { transaction.execute("INSERT INTO mx_module_permissions(module_uid,access_level,can_read,can_create,can_update,can_delete,can_configure,can_report,can_attachments) VALUES (?1,?2,?3,?4,?5,?6,?7,?3,?3)", params![uid,level,read,create,update,delete,configure])?; }
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
        if let Some(permissions) = request.permissions { for permission in permissions { if !(0..=3).contains(&permission.access_level) { continue; } let administrator = permission.access_level == 0; transaction.execute("INSERT INTO mx_module_permissions(module_uid,access_level,can_read,can_create,can_update,can_delete,can_configure,can_report,can_attachments) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(module_uid,access_level) DO UPDATE SET can_read=excluded.can_read,can_create=excluded.can_create,can_update=excluded.can_update,can_delete=excluded.can_delete,can_configure=excluded.can_configure,can_report=excluded.can_report,can_attachments=excluded.can_attachments", params![db_uid,permission.access_level,(administrator || permission.can_read) as i64,(administrator || permission.can_create) as i64,(administrator || permission.can_update) as i64,(administrator || permission.can_delete) as i64,(administrator || permission.can_configure) as i64,(administrator || permission.can_report) as i64,(administrator || permission.can_attachments) as i64])?; } }
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
                if let Some(config_text) = config_text
                    && let Ok(mut config) = serde_json::from_str::<Value>(&config_text)
                    && let Some(widgets) = config.get_mut("widgets").and_then(Value::as_array_mut)
                {
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

    #[test]
    fn account_module_access_is_default_deny_and_cannot_exceed_the_role() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE users(uid TEXT PRIMARY KEY NOT NULL, access_level INTEGER NOT NULL);",
            )
            .unwrap();
        ensure_module_schema(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO users(uid,access_level) VALUES('editor-new',2)",
                [],
            )
            .unwrap();

        assert!(
            effective_module_permission(&connection, DEFAULT_MODULE_UID, "editor-new", 2)
                .unwrap()
                .is_none()
        );
        connection
            .execute(
                "INSERT INTO mx_module_account_permissions(module_uid,user_uid,can_read,can_create,can_update,can_delete,can_configure,can_report,can_attachments,updated_at) VALUES(?1,'editor-new',1,1,1,1,1,1,1,0)",
                params![DEFAULT_MODULE_UID],
            )
            .unwrap();
        let effective =
            effective_module_permission(&connection, DEFAULT_MODULE_UID, "editor-new", 2)
                .unwrap()
                .unwrap();
        assert!(effective.can_read && effective.can_create && effective.can_update);
        assert!(effective.can_report && effective.can_attachments);
        assert!(!effective.can_delete && !effective.can_configure);
    }

    #[test]
    fn module_access_revision_rejects_a_stale_administrator_write() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE users(uid TEXT PRIMARY KEY NOT NULL, access_level INTEGER NOT NULL);",
            )
            .unwrap();
        ensure_module_schema(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO users(uid,access_level) VALUES('editor-new',2)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO mx_module_access_revisions(user_uid,revision) VALUES('editor-new',0)",
                [],
            )
            .unwrap();

        assert_eq!(
            claim_module_access_revision(&connection, "editor-new", 0).unwrap(),
            Ok(1)
        );
        assert_eq!(
            claim_module_access_revision(&connection, "editor-new", 0).unwrap(),
            Err(1)
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
