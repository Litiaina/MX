//! Personal-drive metadata and authorization. Object bytes live only in N1.
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub(super) const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub uid: String,
    pub owner_uid: String,
    pub parent_uid: Option<String>,
    pub name: String,
    pub kind: String,
    pub revision: i64,
    pub mime_type: String,
    pub size: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub trashed_at: Option<i64>,
    pub starred: bool,
    pub permission: String,
    pub original_file_name: String,
    #[serde(skip_serializing)]
    pub object_key: Option<String>,
}

pub(super) fn problem(code: u16, message: &str) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(format!("DRIVE:{code}:{message}"))
}
pub(super) fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
pub(super) fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub(super) fn name(value: &str) -> rusqlite::Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 240
        || value == "."
        || value == ".."
        || value
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
    {
        return Err(problem(
            400,
            "Choose a name of 1–240 bytes without slashes or control characters.",
        ));
    }
    Ok(value.to_owned())
}

pub(crate) fn ensure(connection: &Connection) -> rusqlite::Result<()> {
    crate::api::collaboration::ensure_space_directory_schema(connection)?;
    connection.execute_batch(r#"
    CREATE TABLE IF NOT EXISTS mx_drive_quotas (
      user_uid TEXT PRIMARY KEY REFERENCES users(uid) ON DELETE CASCADE,
      quota_bytes INTEGER CHECK(quota_bytes IS NULL OR quota_bytes>=0), revision INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE IF NOT EXISTS mx_drive_quota_activity (
      uid TEXT PRIMARY KEY, user_uid TEXT NOT NULL, actor_uid TEXT NOT NULL,
      previous_bytes INTEGER, quota_bytes INTEGER, created_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS mx_drive_items (
      uid TEXT PRIMARY KEY, owner_uid TEXT NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
      parent_uid TEXT REFERENCES mx_drive_items(uid) ON DELETE CASCADE, name TEXT NOT NULL,
      kind TEXT NOT NULL CHECK(kind IN ('file','folder')), revision INTEGER NOT NULL DEFAULT 1,
      current_version_uid TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, trashed_at INTEGER
    );
    CREATE INDEX IF NOT EXISTS mx_drive_children ON mx_drive_items(owner_uid,parent_uid,trashed_at);
    CREATE UNIQUE INDEX IF NOT EXISTS mx_drive_names ON mx_drive_items(owner_uid,COALESCE(parent_uid,''),name COLLATE NOCASE) WHERE trashed_at IS NULL;
    CREATE TABLE IF NOT EXISTS mx_drive_versions (
      uid TEXT PRIMARY KEY, item_uid TEXT NOT NULL REFERENCES mx_drive_items(uid) ON DELETE CASCADE,
      object_key TEXT NOT NULL, file_name TEXT NOT NULL, mime_type TEXT NOT NULL, size INTEGER NOT NULL CHECK(size>=0),
      sha256 TEXT NOT NULL, actor_uid TEXT NOT NULL, created_at INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS mx_drive_version_items ON mx_drive_versions(item_uid,created_at);
    CREATE TABLE IF NOT EXISTS mx_drive_grants (
      item_uid TEXT NOT NULL REFERENCES mx_drive_items(uid) ON DELETE CASCADE,
      user_uid TEXT NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
      role TEXT NOT NULL CHECK(role IN ('viewer','editor')), PRIMARY KEY(item_uid,user_uid)
    );
    CREATE INDEX IF NOT EXISTS mx_drive_grant_user ON mx_drive_grants(user_uid,item_uid);
    CREATE TABLE IF NOT EXISTS mx_drive_space_grants (
      item_uid TEXT NOT NULL REFERENCES mx_drive_items(uid) ON DELETE CASCADE,
      space_uid TEXT NOT NULL REFERENCES mx_channels(uid) ON DELETE CASCADE,
      role TEXT NOT NULL CHECK(role IN ('viewer','editor')),PRIMARY KEY(item_uid,space_uid)
    );
    CREATE INDEX IF NOT EXISTS mx_drive_grant_space ON mx_drive_space_grants(space_uid,item_uid);
    CREATE INDEX IF NOT EXISTS mx_drive_page_name ON mx_drive_items(owner_uid,parent_uid,trashed_at,kind DESC,name COLLATE NOCASE,uid);
    CREATE INDEX IF NOT EXISTS mx_drive_page_modified ON mx_drive_items(owner_uid,parent_uid,trashed_at,kind DESC,updated_at DESC,uid);
    CREATE INDEX IF NOT EXISTS mx_drive_parent ON mx_drive_items(parent_uid,trashed_at,kind,name COLLATE NOCASE,uid);
    CREATE TABLE IF NOT EXISTS mx_drive_stars (
      item_uid TEXT NOT NULL REFERENCES mx_drive_items(uid) ON DELETE CASCADE,
      user_uid TEXT NOT NULL REFERENCES users(uid) ON DELETE CASCADE, PRIMARY KEY(item_uid,user_uid)
    );
    CREATE TABLE IF NOT EXISTS mx_drive_links (
      uid TEXT PRIMARY KEY, item_uid TEXT NOT NULL REFERENCES mx_drive_items(uid) ON DELETE CASCADE,
      token_hash TEXT NOT NULL UNIQUE, expires_at INTEGER, revoked_at INTEGER, created_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS mx_drive_activity (
      uid TEXT PRIMARY KEY, item_uid TEXT NOT NULL REFERENCES mx_drive_items(uid) ON DELETE CASCADE,
      actor_uid TEXT NOT NULL, action TEXT NOT NULL, created_at INTEGER NOT NULL
      ,data TEXT NOT NULL DEFAULT '{}'
    );
    CREATE TABLE IF NOT EXISTS mx_drive_uploads (
      actor_uid TEXT NOT NULL, operation_uid TEXT NOT NULL, fingerprint TEXT NOT NULL, owner_uid TEXT NOT NULL, object_key TEXT NOT NULL,
      item_uid TEXT NOT NULL, version_uid TEXT NOT NULL UNIQUE, parent_uid TEXT,
      name TEXT NOT NULL, mime_type TEXT NOT NULL, size INTEGER NOT NULL, sha256 TEXT NOT NULL,
      base_revision INTEGER, attempt_uid TEXT NOT NULL, attempted_at INTEGER NOT NULL,
      completed INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(actor_uid,operation_uid)
    );
    CREATE INDEX IF NOT EXISTS mx_drive_upload_resume ON mx_drive_uploads(actor_uid,fingerprint,completed,attempted_at);
    CREATE TABLE IF NOT EXISTS mx_drive_tombstones (uid TEXT PRIMARY KEY,owner_uid TEXT NOT NULL,deleted_at INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS mx_drive_sessions (
      actor_uid TEXT NOT NULL,operation_uid TEXT NOT NULL,n1_version TEXT NOT NULL,n1_session TEXT NOT NULL,
      coordinator TEXT,part_size INTEGER NOT NULL,PRIMARY KEY(actor_uid,operation_uid),
      FOREIGN KEY(actor_uid,operation_uid) REFERENCES mx_drive_uploads(actor_uid,operation_uid) ON DELETE CASCADE
    );
    CREATE TABLE IF NOT EXISTS mx_drive_parts (
      actor_uid TEXT NOT NULL,operation_uid TEXT NOT NULL,part_index INTEGER NOT NULL,sha256 TEXT NOT NULL,
      PRIMARY KEY(actor_uid,operation_uid,part_index),
      FOREIGN KEY(actor_uid,operation_uid) REFERENCES mx_drive_uploads(actor_uid,operation_uid) ON DELETE CASCADE
    );
    CREATE TABLE IF NOT EXISTS mx_drive_tickets (
      token_hash TEXT PRIMARY KEY,item_uid TEXT NOT NULL REFERENCES mx_drive_items(uid) ON DELETE CASCADE,
      actor_uid TEXT NOT NULL REFERENCES users(uid) ON DELETE CASCADE,auth_version INTEGER NOT NULL,
      version_uid TEXT,expires_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS mx_drive_gc (
      object_key TEXT PRIMARY KEY, attempts INTEGER NOT NULL DEFAULT 0,
      next_attempt_at INTEGER NOT NULL, created_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS mx_drive_cancel (
      object_key TEXT NOT NULL,n1_version TEXT NOT NULL,coordinator TEXT,attempts INTEGER NOT NULL DEFAULT 0,
      next_attempt_at INTEGER NOT NULL,PRIMARY KEY(object_key,n1_version)
    );
    CREATE TRIGGER IF NOT EXISTS mx_drive_cancel_pending BEFORE DELETE ON mx_drive_uploads WHEN OLD.completed=0 BEGIN
      INSERT OR IGNORE INTO mx_drive_cancel(object_key,n1_version,coordinator,next_attempt_at)
      SELECT OLD.object_key,s.n1_version,s.coordinator,0
      FROM mx_drive_sessions s WHERE s.actor_uid=OLD.actor_uid AND s.operation_uid=OLD.operation_uid;
    END;
    CREATE TRIGGER IF NOT EXISTS mx_drive_version_cleanup BEFORE DELETE ON mx_drive_versions BEGIN
      INSERT OR IGNORE INTO mx_drive_gc(object_key,next_attempt_at,created_at)
      VALUES(OLD.object_key,CAST(strftime('%s','now') AS INTEGER)*1000,CAST(strftime('%s','now') AS INTEGER)*1000);
    END;
    "#)?;
    // Copies have independent metadata/history but can reference immutable N1
    // bytes. No browser download/re-upload, including multi-gigabyte files.
    let unique_object_key: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_index_list('mx_drive_versions') l JOIN pragma_index_info(l.name) i WHERE l.\"unique\"=1 AND i.name='object_key')",
        [], |r| r.get(0),
    )?;
    if unique_object_key {
        let tx = rusqlite::Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
        tx.execute_batch("DROP TRIGGER IF EXISTS mx_drive_version_cleanup;
          CREATE TABLE mx_drive_versions_migrated (
            uid TEXT PRIMARY KEY,item_uid TEXT NOT NULL REFERENCES mx_drive_items(uid) ON DELETE CASCADE,
            object_key TEXT NOT NULL,file_name TEXT NOT NULL,mime_type TEXT NOT NULL,size INTEGER NOT NULL CHECK(size>=0),
            sha256 TEXT NOT NULL,actor_uid TEXT NOT NULL,created_at INTEGER NOT NULL);
          INSERT INTO mx_drive_versions_migrated SELECT * FROM mx_drive_versions;
          DROP TABLE mx_drive_versions;
          ALTER TABLE mx_drive_versions_migrated RENAME TO mx_drive_versions;
          CREATE INDEX mx_drive_version_items ON mx_drive_versions(item_uid,created_at);
          CREATE TRIGGER mx_drive_version_cleanup BEFORE DELETE ON mx_drive_versions BEGIN
            INSERT OR IGNORE INTO mx_drive_gc(object_key,next_attempt_at,created_at)
            VALUES(OLD.object_key,CAST(strftime('%s','now') AS INTEGER)*1000,CAST(strftime('%s','now') AS INTEGER)*1000);
          END;")?;
        tx.commit()?;
    }
    connection.execute_batch("CREATE INDEX IF NOT EXISTS mx_drive_object_refs ON mx_drive_versions(object_key);
      CREATE TABLE IF NOT EXISTS mx_drive_transfers (
        actor_uid TEXT NOT NULL,operation_uid TEXT NOT NULL,fingerprint TEXT NOT NULL,result TEXT NOT NULL,
        created_at INTEGER NOT NULL,PRIMARY KEY(actor_uid,operation_uid));")
}

pub(super) fn quota(connection: &Connection, owner: &str, default: i64) -> rusqlite::Result<i64> {
    Ok(connection
        .query_row(
            "SELECT quota_bytes FROM mx_drive_quotas WHERE user_uid=?1",
            [owner],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()?
        .flatten()
        .unwrap_or(default))
}

pub(super) fn row_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<Item> {
    Ok(Item {
        uid: row.get(0)?,
        owner_uid: row.get(1)?,
        parent_uid: row.get(2)?,
        name: row.get(3)?,
        kind: row.get(4)?,
        revision: row.get(5)?,
        mime_type: row.get(6)?,
        size: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        trashed_at: row.get(10)?,
        object_key: row.get(11)?,
        starred: false,
        permission: String::new(),
        original_file_name: row.get(12)?,
    })
}
pub(super) const ITEM_SQL: &str = "SELECT i.uid,i.owner_uid,i.parent_uid,i.name,i.kind,i.revision,COALESCE(v.mime_type,''),COALESCE(v.size,0),i.created_at,i.updated_at,i.trashed_at,v.object_key,COALESCE(v.file_name,'') FROM mx_drive_items i LEFT JOIN mx_drive_versions v ON v.uid=i.current_version_uid";

pub(super) fn raw(connection: &Connection, uid: &str) -> rusqlite::Result<Item> {
    connection
        .query_row(&format!("{ITEM_SQL} WHERE i.uid=?1"), [uid], row_item)
        .optional()?
        .ok_or_else(|| problem(404, "File or folder not found."))
}

pub(super) fn ancestors(connection: &Connection, uid: &str) -> rusqlite::Result<Vec<Item>> {
    let mut items = Vec::new();
    let mut next = Some(uid.to_owned());
    while let Some(uid) = next {
        if items.len() >= MAX_DEPTH || items.iter().any(|item: &Item| item.uid == uid) {
            return Err(problem(409, "Folder hierarchy is too deep or invalid."));
        }
        let item = raw(connection, &uid)?;
        next = item.parent_uid.clone();
        items.push(item);
    }
    Ok(items)
}

pub(super) fn accessible(
    connection: &Connection,
    uid: &str,
    actor: &str,
    include_trash: bool,
) -> rusqlite::Result<Item> {
    let chain = ancestors(connection, uid)?;
    let mut item = chain[0].clone();
    if chain.iter().any(|parent| parent.trashed_at.is_some())
        && !(include_trash && item.owner_uid == actor)
    {
        return Err(problem(404, "File or folder not found."));
    }
    let mut permission = if item.owner_uid == actor { "owner" } else { "" };
    for parent in &chain {
        if parent.owner_uid != item.owner_uid {
            return Err(problem(404, "File or folder not found."));
        }
        let grant: Option<String> = connection
            .query_row(
                "SELECT role FROM (SELECT role FROM mx_drive_grants WHERE item_uid=?1 AND user_uid=?2 UNION ALL SELECT g.role FROM mx_drive_space_grants g JOIN mx_channels s ON s.uid=g.space_uid JOIN mx_channel_members m ON m.channel_uid=s.uid AND m.user_uid=?2 WHERE g.item_uid=?1 AND s.kind<>'direct' AND s.archived_at IS NULL) ORDER BY role='editor' DESC LIMIT 1",
                params![parent.uid, actor],
                |row| row.get(0),
            )
            .optional()?;
        if permission != "owner" {
            match grant.as_deref() {
                Some("editor") => permission = "editor",
                Some("viewer") if permission.is_empty() => permission = "viewer",
                _ => {}
            }
        }
    }
    if permission.is_empty() {
        return Err(problem(404, "File or folder not found."));
    }
    item.permission = permission.into();
    item.starred = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM mx_drive_stars WHERE item_uid=?1 AND user_uid=?2)",
        params![uid, actor],
        |row| row.get(0),
    )?;
    Ok(item)
}

pub(super) fn owner(
    connection: &Connection,
    uid: &str,
    actor: &str,
    trash: bool,
) -> rusqlite::Result<Item> {
    let item = accessible(connection, uid, actor, trash)?;
    if item.permission != "owner" {
        return Err(problem(403, "Only the owner can perform this action."));
    }
    Ok(item)
}
pub(super) fn base(item: &Item, revision: i64) -> rusqlite::Result<()> {
    if item.revision != revision {
        return Err(problem(
            409,
            "This item changed elsewhere. Refresh before trying again.",
        ));
    }
    Ok(())
}
pub(super) fn parent(
    connection: &Connection,
    uid: Option<&str>,
    actor: &str,
    moving: Option<&str>,
) -> rusqlite::Result<()> {
    if let Some(uid) = uid {
        let item = owner(connection, uid, actor, false)?;
        if item.kind != "folder" {
            return Err(problem(400, "The destination must be a folder."));
        }
        let chain = ancestors(connection, uid)?;
        if chain.len() >= MAX_DEPTH - 1
            || chain.iter().any(|item| Some(item.uid.as_str()) == moving)
        {
            return Err(problem(
                409,
                "A folder cannot be moved into itself or its descendants.",
            ));
        }
    }
    Ok(())
}
pub(super) fn unique_name(
    connection: &Connection,
    owner: &str,
    parent: Option<&str>,
    name: &str,
    except: &str,
) -> rusqlite::Result<()> {
    let exists: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM mx_drive_items WHERE owner_uid=?1 AND parent_uid IS ?2 AND name=?3 COLLATE NOCASE AND uid<>?4 AND trashed_at IS NULL)", params![owner,parent,name,except], |row| row.get(0))?;
    if exists {
        return Err(problem(
            409,
            "An item with this name already exists in that folder. Rename it or upload a new version.",
        ));
    }
    Ok(())
}
pub(super) fn activity(
    connection: &Connection,
    uid: &str,
    actor: &str,
    action: &str,
) -> rusqlite::Result<()> {
    activity_detail(connection, uid, actor, action, &json!({}))
}
pub(super) fn activity_detail(
    connection: &Connection,
    uid: &str,
    actor: &str,
    action: &str,
    data: &Value,
) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO mx_drive_activity(uid,item_uid,actor_uid,action,created_at,data) VALUES(?1,?2,?3,?4,?5,?6)", params![Uuid::new_v4().to_string(),uid,actor,action,now(),data.to_string()])?;
    Ok(())
}

pub(super) fn folder(
    connection: &Connection,
    actor: &str,
    uid: &str,
    folder_name: &str,
    parent_uid: Option<&str>,
) -> rusqlite::Result<Item> {
    let folder_name = name(folder_name)?;
    if Uuid::parse_str(uid).is_err() {
        return Err(problem(400, "operation_uid must be a UUID."));
    }
    let tx = rusqlite::Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
    let deleted: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM mx_drive_tombstones WHERE uid=?1)",
        [uid],
        |row| row.get(0),
    )?;
    if deleted {
        return Err(problem(
            410,
            "This create operation was permanently removed.",
        ));
    }
    // Replaying a folder create cannot restore a deleted folder or change its payload.
    if tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM mx_drive_items WHERE uid=?1)",
        [uid],
        |r| r.get::<_, bool>(0),
    )? {
        let item = raw(&tx, uid)?;
        if item.owner_uid != actor
            || item.kind != "folder"
            || item.name != folder_name
            || item.parent_uid.as_deref() != parent_uid
        {
            return Err(problem(409, "This create identifier is already in use."));
        }
        let item = accessible(&tx, uid, actor, false)?;
        return Ok(item);
    }
    parent(&tx, parent_uid, actor, None)?;
    unique_name(&tx, actor, parent_uid, &folder_name, uid)?;
    tx.execute("INSERT INTO mx_drive_items(uid,owner_uid,parent_uid,name,kind,created_at,updated_at) VALUES(?1,?2,?3,?4,'folder',?5,?5)", params![uid,actor,parent_uid,folder_name,now()])?;
    activity(&tx, uid, actor, "folder.created")?;
    let item = accessible(&tx, uid, actor, false)?;
    tx.commit()?;
    Ok(item)
}

#[cfg(test)]
fn list(
    connection: &Connection,
    actor: &str,
    view: &str,
    folder: Option<&str>,
    query: &str,
    offset: usize,
) -> rusqlite::Result<Value> {
    super::listing::list(connection,actor,&super::listing::Listing{view:view.into(),parent_uid:folder.map(str::to_owned),q:query.into(),offset,limit:100,..Default::default()})
}

pub(super) fn mutate(
    connection: &Connection,
    actor: &str,
    uid: &str,
    revision: i64,
    action: &str,
    new_name: Option<&str>,
    destination: Option<&str>,
) -> rusqlite::Result<Item> {
    let tx = rusqlite::Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
    let item = mutate_in_transaction(&tx, actor, uid, revision, action, new_name, destination)?;
    tx.commit()?;
    Ok(item)
}

pub(super) fn mutate_in_transaction(
    tx: &Connection,
    actor: &str,
    uid: &str,
    revision: i64,
    action: &str,
    new_name: Option<&str>,
    destination: Option<&str>,
) -> rusqlite::Result<Item> {
    let item = if action == "rename" {
        accessible(&tx, uid, actor, false)?
    } else {
        owner(&tx, uid, actor, action == "restore")?
    };
    base(&item, revision)?;
    if item.permission == "viewer" {
        return Err(problem(403, "You have view-only access."));
    }
    match action {
        "rename" => {
            let value = name(new_name.unwrap_or_default())?;
            unique_name(
                &tx,
                &item.owner_uid,
                item.parent_uid.as_deref(),
                &value,
                uid,
            )?;
            tx.execute(
                "UPDATE mx_drive_items SET name=?1,revision=revision+1,updated_at=?2 WHERE uid=?3",
                params![value, now(), uid],
            )?;
        }
        "move" => {
            parent(&tx, destination, actor, Some(uid))?;
            unique_name(&tx, actor, destination, &item.name, uid)?;
            let height=tx.query_row("WITH RECURSIVE tree(uid,depth) AS (SELECT ?1,1 UNION ALL SELECT i.uid,t.depth+1 FROM mx_drive_items i JOIN tree t ON i.parent_uid=t.uid WHERE t.depth<=64) SELECT MAX(depth) FROM tree",[uid],|r|r.get::<_,i64>(0))? as usize;
            let depth = if let Some(destination) = destination {
                ancestors(&tx, destination)?.len()
            } else {
                0
            };
            if height + depth > MAX_DEPTH {
                return Err(problem(
                    409,
                    "The destination would exceed the folder depth limit.",
                ));
            }
            tx.execute("UPDATE mx_drive_items SET parent_uid=?1,revision=revision+1,updated_at=?2 WHERE uid=?3",params![destination,now(),uid])?;
        }
        "trash" => {
            tx.execute("UPDATE mx_drive_items SET trashed_at=?1,revision=revision+1,updated_at=?1 WHERE uid=?2",params![now(),uid])?;
            // Restoring a file must not silently reactivate a previously public link.
            tx.execute("WITH RECURSIVE children(uid) AS (SELECT ?1 UNION SELECT i.uid FROM mx_drive_items i JOIN children c ON i.parent_uid=c.uid) UPDATE mx_drive_links SET revoked_at=?2 WHERE item_uid IN children AND revoked_at IS NULL",params![uid,now()])?;
        }
        "restore" => {
            if let Some(parent_uid) = item.parent_uid.as_deref() {
                parent(&tx, Some(parent_uid), actor, None)?;
            }
            unique_name(&tx, actor, item.parent_uid.as_deref(), &item.name, uid)?;
            tx.execute("UPDATE mx_drive_items SET trashed_at=NULL,revision=revision+1,updated_at=?1 WHERE uid=?2",params![now(),uid])?;
        }
        _ => return Err(problem(400, "Unknown drive action.")),
    }
    activity(&tx, uid, actor, &format!("item.{action}"))?;
    let item = accessible(&tx, uid, actor, action == "trash")?;
    Ok(item)
}

pub(super) fn shared(connection: &Connection, actor: &str, uid: &str) -> rusqlite::Result<Value> {
    owner(connection, uid, actor, false)?;
    let mut users=connection.prepare("SELECT g.user_uid,u.name,u.email,g.role FROM mx_drive_grants g JOIN users u ON u.uid=g.user_uid WHERE g.item_uid=?1 ORDER BY u.name")?;
    let users=users.query_map([uid],|row|Ok(json!({"user_uid":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,"email":row.get::<_,String>(2)?,"role":row.get::<_,String>(3)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut links=connection.prepare("SELECT uid,expires_at,created_at FROM mx_drive_links WHERE item_uid=?1 AND revoked_at IS NULL ORDER BY created_at DESC")?;
    let links=links.query_map([uid],|row|Ok(json!({"uid":row.get::<_,String>(0)?,"expires_at":row.get::<_,Option<i64>>(1)?,"created_at":row.get::<_,i64>(2)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let spaces=super::sharing::details(connection,uid)?;
    let mut inherited=Vec::new();
    for ancestor in ancestors(connection,uid)?.into_iter().skip(1) {
        let mut s=connection.prepare("SELECT u.name,u.email,g.role FROM mx_drive_grants g JOIN users u ON u.uid=g.user_uid WHERE g.item_uid=?1 ORDER BY u.name")?;
        for entry in s.query_map([&ancestor.uid],|r|Ok(json!({"name":r.get::<_,String>(0)?,"detail":r.get::<_,String>(1)?,"role":r.get::<_,String>(2)?,"kind":"account","from":ancestor.name})))? { inherited.push(entry?); }
        for space in super::sharing::details(connection,&ancestor.uid)? { if space["archived"]!=true {inherited.push(json!({"name":space["name"],"detail":"Collaboration space","role":space["role"],"kind":"space","from":ancestor.name}));} }
    }
    Ok(json!({"users":users,"spaces":spaces,"inherited":inherited,"links":links}))
}

pub(super) fn grant(
    connection: &Connection,
    actor: &str,
    uid: &str,
    target: &str,
    role: Option<&str>,
) -> rusqlite::Result<()> {
    let tx = rusqlite::Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
    owner(&tx, uid, actor, false)?;
    if target == actor {
        return Err(problem(400, "The owner already has full access."));
    }
    let previous: Option<String> = tx
        .query_row(
            "SELECT role FROM mx_drive_grants WHERE item_uid=?1 AND user_uid=?2",
            params![uid, target],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(role) = role {
        if !["viewer", "editor"].contains(&role) {
            return Err(problem(400, "Choose viewer or editor access."));
        }
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM users WHERE uid=?1)",
            [target],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(problem(404, "MX account not found."));
        }
        tx.execute("INSERT INTO mx_drive_grants(item_uid,user_uid,role) VALUES(?1,?2,?3) ON CONFLICT(item_uid,user_uid) DO UPDATE SET role=excluded.role",params![uid,target,role])?;
    } else {
        tx.execute(
            "DELETE FROM mx_drive_grants WHERE item_uid=?1 AND user_uid=?2",
            params![uid, target],
        )?;
    }
    activity_detail(
        &tx,
        uid,
        actor,
        "sharing.changed",
        &json!({"user_uid":target,"previous_role":previous,"new_role":role}),
    )?;
    tx.commit()
}

pub(super) fn link(
    connection: &Connection,
    actor: &str,
    uid: &str,
    expires_at: Option<i64>,
) -> rusqlite::Result<Value> {
    let tx = rusqlite::Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
    owner(&tx, uid, actor, false)?;
    if expires_at.is_some_and(|value| value <= now()) {
        return Err(problem(400, "Link expiration must be in the future."));
    }
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let link_uid = Uuid::new_v4().to_string();
    tx.execute("INSERT INTO mx_drive_links(uid,item_uid,token_hash,expires_at,created_at) VALUES(?1,?2,?3,?4,?5)",params![link_uid,uid,digest(token.as_bytes()),expires_at,now()])?;
    activity(&tx, uid, actor, "public_link.created")?;
    tx.commit()?;
    Ok(json!({"uid":link_uid,"token":token,"expires_at":expires_at}))
}

pub(super) fn guest(
    connection: &Connection,
    token: &str,
    target: Option<&str>,
) -> rusqlite::Result<Item> {
    if token.len() != 64 || !token.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(problem(404, "Shared link is unavailable."));
    }
    let root: Option<String>=connection.query_row("SELECT item_uid FROM mx_drive_links WHERE token_hash=?1 AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>?2)",params![digest(token.as_bytes()),now()],|row|row.get(0)).optional()?;
    let root = root.ok_or_else(|| problem(404, "Shared link is unavailable or expired."))?;
    let chain = ancestors(connection, target.unwrap_or(&root))?;
    if !chain.iter().any(|item| item.uid == root)
        || chain.iter().any(|item| item.trashed_at.is_some())
    {
        return Err(problem(404, "Shared link is unavailable."));
    }
    let mut item = chain[0].clone();
    item.permission = "guest".into();
    Ok(item)
}

pub(super) fn extension(file_name: &str) -> String {
    let lower = file_name.to_ascii_lowercase();
    for suffix in [".tar.gz", ".tar.bz2", ".tar.xz"] {
        if lower.ends_with(suffix) {
            return file_name[file_name.len() - suffix.len()..].to_owned();
        }
    }
    match file_name.rsplit_once('.') {
        Some((stem, ext))
            if !stem.is_empty()
                && !ext.is_empty()
                && !ext
                    .chars()
                    .any(|ch| ch.is_control() || ch == '/' || ch == '\\') =>
        {
            format!(".{ext}")
        }
        _ => String::new(),
    }
}

pub(super) fn object_key(owner: &str, version: &str, original_name: &str) -> String {
    format!("__mx/drive/{owner}/{version}{}", extension(original_name))
}

pub(super) fn purge(
    connection: &Connection,
    actor: &str,
    uid: Option<&str>,
) -> rusqlite::Result<usize> {
    let tx = rusqlite::Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
    if let Some(uid) = uid {
        let item = owner(&tx, uid, actor, true)?;
        if item.trashed_at.is_none() {
            return Err(problem(
                409,
                "Move the item to trash before removing it permanently.",
            ));
        }
    }
    let roots = {
        let mut statement=tx.prepare("SELECT uid FROM mx_drive_items WHERE owner_uid=?1 AND trashed_at IS NOT NULL AND (?2 IS NULL OR uid=?2)")?;
        statement
            .query_map(params![actor, uid], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut removed = 0;
    for root in roots {
        // Queue all immutable versions atomically BEFORE deleting metadata.
        // No N1 request is made on the client's empty-trash path.
        tx.execute("WITH RECURSIVE children(uid) AS (SELECT ?1 UNION SELECT i.uid FROM mx_drive_items i JOIN children c ON i.parent_uid=c.uid) INSERT OR IGNORE INTO mx_drive_gc(object_key,next_attempt_at,created_at) SELECT object_key,?2,?2 FROM mx_drive_versions WHERE item_uid IN children",params![root,now()])?;
        removed+=tx.execute("WITH RECURSIVE children(uid) AS (SELECT ?1 UNION SELECT i.uid FROM mx_drive_items i JOIN children c ON i.parent_uid=c.uid) INSERT OR IGNORE INTO mx_drive_tombstones(uid,owner_uid,deleted_at) SELECT uid,owner_uid,?2 FROM mx_drive_items WHERE uid IN children",params![root,now()])?;
        tx.execute("DELETE FROM mx_drive_items WHERE uid=?1", [root])?;
    }
    tx.commit()?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn database() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE users(uid TEXT PRIMARY KEY,name TEXT,email TEXT); INSERT INTO users VALUES('a','Alice','a@local'),('b','Bob','b@local'),('admin','Admin','admin@local');").unwrap();
        ensure(&c).unwrap();
        c
    }
    fn make(c: &Connection, actor: &str, label: &str, parent: Option<&str>) -> Item {
        folder(c, actor, &Uuid::new_v4().to_string(), label, parent).unwrap()
    }
    #[test]
    fn private_default_and_inherited_access_do_not_grant_resharing() {
        let c = database();
        let root = make(&c, "a", "Private", None);
        let nested = make(&c, "a", "Child", Some(&root.uid));
        assert!(accessible(&c, &nested.uid, "b", false).is_err());
        assert!(accessible(&c, &root.uid, "admin", false).is_err());
        grant(&c, "a", &root.uid, "b", Some("viewer")).unwrap();
        assert_eq!(
            accessible(&c, &nested.uid, "b", false).unwrap().permission,
            "viewer"
        );
        assert!(mutate(&c, "b", &nested.uid, 1, "rename", Some("Changed"), None).is_err());
        assert!(grant(&c, "b", &nested.uid, "admin", Some("editor")).is_err());
        grant(&c, "a", &root.uid, "b", Some("editor")).unwrap();
        mutate(&c, "b", &nested.uid, 1, "rename", Some("Changed"), None).unwrap();
        grant(&c, "a", &root.uid, "b", None).unwrap();
        assert!(accessible(&c, &nested.uid, "b", false).is_err());
    }
    #[test]
    fn cycle_stale_updates_and_replayed_deleted_creates_are_rejected() {
        let c = database();
        let root = make(&c, "a", "Parent", None);
        let child = make(&c, "a", "Child", Some(&root.uid));
        assert!(mutate(&c, "a", &root.uid, 1, "move", None, Some(&child.uid)).is_err());
        mutate(&c, "a", &root.uid, 1, "rename", Some("New"), None).unwrap();
        assert!(mutate(&c, "a", &root.uid, 1, "rename", Some("Lost"), None).is_err());
        mutate(&c, "a", &root.uid, 2, "trash", None, None).unwrap();
        assert!(folder(&c, "a", &root.uid, "New", None).is_err());
        assert!(accessible(&c, &child.uid, "a", false).is_err());
    }
    #[test]
    fn public_tokens_are_hashed_scoped_expirable_and_permanently_revoked_on_trash() {
        let c = database();
        let root = make(&c, "a", "Public", None);
        let child = make(&c, "a", "Child", Some(&root.uid));
        let private = make(&c, "a", "Secret", None);
        let value = link(&c, "a", &root.uid, None).unwrap();
        let token = value["token"].as_str().unwrap();
        assert_eq!(guest(&c, token, Some(&child.uid)).unwrap().uid, child.uid);
        assert!(guest(&c, token, Some(&private.uid)).is_err());
        let stored: String = c
            .query_row("SELECT token_hash FROM mx_drive_links", [], |r| r.get(0))
            .unwrap();
        assert_ne!(stored, token);
        mutate(&c, "a", &root.uid, 1, "trash", None, None).unwrap();
        assert!(guest(&c, token, None).is_err());
        mutate(&c, "a", &root.uid, 2, "restore", None, None).unwrap();
        assert!(guest(&c, token, None).is_err());
        assert!(link(&c, "a", &root.uid, Some(now() - 1)).is_err());
    }
    #[test]
    fn private_parents_are_omitted_from_shared_breadcrumbs_and_object_keys_never_serialize() {
        let c = database();
        let private = make(&c, "a", "Confidential parent", None);
        let shared = make(&c, "a", "Shared", Some(&private.uid));
        grant(&c, "a", &shared.uid, "b", Some("viewer")).unwrap();
        let value = list(&c, "b", "shared", Some(&shared.uid), "", 0).unwrap();
        assert_eq!(value["breadcrumbs"].as_array().unwrap().len(), 1);
        assert_eq!(value["breadcrumbs"][0]["name"], "Shared");
        let mut item = shared;
        item.object_key = Some("never disclose N1 keys".into());
        assert!(
            !serde_json::to_string(&item)
                .unwrap()
                .contains("never disclose")
        );
    }
    #[test]
    fn immutable_keys_preserve_original_extensions_not_renamed_metadata() {
        for (name, suffix) in [
            ("meeting.mp4", ".mp4"),
            ("Camera.MOV", ".MOV"),
            ("annual.report.PDF", ".PDF"),
            ("backup.TAR.GZ", ".TAR.GZ"),
            ("README", ""),
            (".env", ""),
        ] {
            assert_eq!(
                object_key("owner", "version", name),
                format!("__mx/drive/owner/version{suffix}")
            );
        }
        assert_ne!(
            object_key("owner", "v1", "clip.mp4"),
            object_key("owner", "v2", "clip.mp4")
        );
    }
}
