//! Atomic, payload-bound file-manager transfers; never download bytes to copy.
use super::{Error, database, store};
use crate::middleware::auth::Claims;
use axum::{Json, extract::Path};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Deserialize, Serialize)]
pub(super) struct Source {
    uid: String,
    base_revision: i64,
}
#[derive(Deserialize, Serialize)]
pub(super) struct Transfer {
    operation_uid: String,
    action: String,
    parent_uid: Option<String>,
    items: Vec<Source>,
}
pub(super) async fn transfer(
    claims: Claims,
    Json(input): Json<Transfer>,
) -> Result<Json<Value>, Error> {
    let default_quota = crate::config::load_config::CONFIG.drive.quota_mb as i64 * 1024 * 1024;
    Ok(Json(
        database(move |c| apply(c, &claims.uid, input, default_quota)).await?,
    ))
}

#[derive(Deserialize, Serialize)]
pub(super) struct Restore {
    operation_uid: String,
    base_revision: i64,
    version_uid: String,
}
pub(super) async fn restore(
    claims: Claims,
    Path(uid): Path<String>,
    Json(input): Json<Restore>,
) -> Result<Json<Value>, Error> {
    let default = crate::config::load_config::CONFIG.drive.quota_mb as i64 * 1024 * 1024;
    Ok(Json(
        database(move |c| restore_version(c, &claims.uid, &uid, input, default)).await?,
    ))
}
fn restore_version(
    c: &Connection,
    actor: &str,
    uid: &str,
    input: Restore,
    default: i64,
) -> rusqlite::Result<Value> {
    if Uuid::parse_str(&input.operation_uid).is_err() {
        return Err(store::problem(400, "operation_uid must be a UUID."));
    }
    let fingerprint = store::digest(
        json!({"restore":uid,"payload":input})
            .to_string()
            .as_bytes(),
    );
    let tx = rusqlite::Transaction::new_unchecked(c, TransactionBehavior::Immediate)?;
    let replay:Option<(String,String)>=tx.query_row("SELECT fingerprint,result FROM mx_drive_transfers WHERE actor_uid=?1 AND operation_uid=?2",params![actor,input.operation_uid],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    if let Some((prior, result)) = replay {
        if prior != fingerprint {
            return Err(store::problem(
                409,
                "This operation identifier was already used.",
            ));
        }
        return serde_json::from_str(&result)
            .map_err(|_| store::problem(500, "Invalid restore receipt."));
    }
    let item = store::accessible(&tx, uid, actor, false)?;
    store::base(&item, input.base_revision)?;
    if item.permission == "viewer" || item.kind != "file" {
        return Err(store::problem(403, "Edit access to this file is required."));
    }
    let size: i64 = tx
        .query_row(
            "SELECT size FROM mx_drive_versions WHERE uid=?1 AND item_uid=?2",
            params![input.version_uid, uid],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| store::problem(404, "File version not found."))?;
    let used:i64=tx.query_row("SELECT COALESCE(SUM(v.size),0) FROM mx_drive_versions v JOIN mx_drive_items i ON i.uid=v.item_uid WHERE i.owner_uid=?1",[&item.owner_uid],|r|r.get(0))?;
    let reserved:i64=tx.query_row("SELECT COALESCE(SUM(size),0) FROM mx_drive_uploads WHERE owner_uid=?1 AND completed=0 AND ABS(attempted_at)>?2",params![item.owner_uid,store::now()-super::uploads::LEASE_MS],|r|r.get(0))?;
    if used.saturating_add(reserved).saturating_add(size)
        > store::quota(&tx, &item.owner_uid, default)?
    {
        return Err(store::problem(
            413,
            "Not enough storage to restore this version. Versions count toward the owner's allowance.",
        ));
    }
    let version = Uuid::new_v4().to_string();
    tx.execute("INSERT INTO mx_drive_versions(uid,item_uid,object_key,file_name,mime_type,size,sha256,actor_uid,created_at) SELECT ?1,item_uid,object_key,file_name,mime_type,size,sha256,?2,?3 FROM mx_drive_versions WHERE uid=?4 AND item_uid=?5",params![version,actor,store::now(),input.version_uid,uid])?;
    tx.execute("UPDATE mx_drive_items SET current_version_uid=?1,revision=revision+1,updated_at=?2 WHERE uid=?3",params![version,store::now(),uid])?;
    store::activity(&tx, uid, actor, "file.version_restored")?;
    let result = serde_json::to_value(store::accessible(&tx, uid, actor, false)?).unwrap();
    tx.execute("INSERT INTO mx_drive_transfers(actor_uid,operation_uid,fingerprint,result,created_at) VALUES(?1,?2,?3,?4,?5)",params![actor,input.operation_uid,fingerprint,result.to_string(),store::now()])?;
    tx.commit()?;
    Ok(result)
}

fn copy_name(
    c: &Connection,
    actor: &str,
    parent: Option<&str>,
    original: &str,
) -> rusqlite::Result<String> {
    for index in 0..10_000 {
        let candidate = if index == 0 {
            original.to_string()
        } else {
            let (stem, extension) = original
                .rsplit_once('.')
                .filter(|(stem, ext)| !stem.is_empty() && ext.len() <= 20)
                .map(|(stem, ext)| (stem, format!(".{ext}")))
                .unwrap_or((original, String::new()));
            let suffix = if index == 1 {
                " (copy)".to_string()
            } else {
                format!(" (copy {index})")
            };
            let budget = 240 - suffix.len() - extension.len();
            let mut end = stem.len().min(budget);
            while !stem.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}{}{}", &stem[..end], suffix, extension)
        };
        let exists: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM mx_drive_items WHERE owner_uid=?1 AND parent_uid IS ?2 AND name=?3 COLLATE NOCASE AND trashed_at IS NULL)",params![actor,parent,candidate],|r|r.get(0))?;
        if !exists {
            return Ok(candidate);
        }
    }
    Err(store::problem(
        409,
        "Too many copies with this name. Rename the original first.",
    ))
}

fn apply(
    c: &Connection,
    actor: &str,
    input: Transfer,
    default_quota: i64,
) -> rusqlite::Result<Value> {
    if Uuid::parse_str(&input.operation_uid).is_err()
        || !["copy", "move"].contains(&input.action.as_str())
        || input.items.is_empty()
        || input.items.len() > 100
    {
        return Err(store::problem(
            400,
            "Choose 1–100 items and a valid transfer operation.",
        ));
    }
    let fingerprint = store::digest(serde_json::to_string(&input).unwrap().as_bytes());
    let tx = rusqlite::Transaction::new_unchecked(c, TransactionBehavior::Immediate)?;
    let replay: Option<(String,String)> = tx.query_row("SELECT fingerprint,result FROM mx_drive_transfers WHERE actor_uid=?1 AND operation_uid=?2",params![actor,input.operation_uid],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    if let Some((prior, result)) = replay {
        if fingerprint != prior {
            return Err(store::problem(
                409,
                "This transfer identifier was already used for different items.",
            ));
        }
        // Replays return the original receipt, never recreate deleted entries.
        return serde_json::from_str(&result)
            .map_err(|_| store::problem(500, "Invalid transfer receipt."));
    }
    store::parent(&tx, input.parent_uid.as_deref(), actor, None)?;
    let mut sources = Vec::new();
    let mut unique = std::collections::HashSet::new();
    for source in &input.items {
        if !unique.insert(source.uid.clone()) {
            return Err(store::problem(400, "An item was selected twice."));
        }
        let item = if input.action == "move" {
            store::owner(&tx, &source.uid, actor, false)?
        } else {
            store::accessible(&tx, &source.uid, actor, false)?
        };
        store::base(&item, source.base_revision)?;
        store::parent(&tx, input.parent_uid.as_deref(), actor, Some(&item.uid))?;
        sources.push(item);
    }
    let roots = sources
        .iter()
        .filter_map(|item| match store::ancestors(&tx, &item.uid) {
            Ok(chain) if !chain.iter().skip(1).any(|p| unique.contains(&p.uid)) => {
                Some(Ok(item.clone()))
            }
            Ok(_) => None,
            Err(e) => Some(Err(e)),
        })
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut result = Vec::new();
    if input.action == "move" {
        for item in roots {
            result.push(store::mutate_in_transaction(
                &tx,
                actor,
                &item.uid,
                item.revision,
                "move",
                None,
                input.parent_uid.as_deref(),
            )?);
        }
    } else {
        // Snapshot before inserts prevents recursive self-copy. Bounds protect DB latency.
        let mut tree = Vec::new();
        for item in roots {
            let mut s = tx.prepare("WITH RECURSIVE tree(uid,depth) AS (SELECT ?1,1 UNION ALL SELECT i.uid,t.depth+1 FROM mx_drive_items i JOIN tree t ON i.parent_uid=t.uid WHERE i.trashed_at IS NULL AND t.depth<=64) SELECT uid,depth FROM tree ORDER BY depth,uid LIMIT 5001")?;
            let entries = s
                .query_map([&item.uid], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as usize))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for (uid, depth) in entries {
                tree.push((store::accessible(&tx, &uid, actor, false)?, depth));
            }
            if tree.len() > 5000 {
                return Err(store::problem(
                    400,
                    "Copy up to 5,000 nested items at a time.",
                ));
            }
        }
        let destination_depth = if let Some(uid) = &input.parent_uid {
            store::ancestors(&tx, uid)?.len()
        } else {
            0
        };
        if tree
            .iter()
            .any(|(_, depth)| depth + destination_depth > store::MAX_DEPTH)
        {
            return Err(store::problem(
                409,
                "The destination would exceed the folder depth limit.",
            ));
        }
        let bytes = tree.iter().try_fold(0i64, |sum, (item, _)| {
            sum.checked_add(item.size)
                .ok_or_else(|| store::problem(413, "Storage size is too large."))
        })?;
        let used: i64 = tx.query_row("SELECT COALESCE(SUM(v.size),0) FROM mx_drive_versions v JOIN mx_drive_items i ON i.uid=v.item_uid WHERE i.owner_uid=?1",[actor],|r|r.get(0))?;
        let reserved: i64 = tx.query_row("SELECT COALESCE(SUM(size),0) FROM mx_drive_uploads WHERE owner_uid=?1 AND completed=0 AND ABS(attempted_at)>?2",params![actor,store::now()-super::uploads::LEASE_MS],|r|r.get(0))?;
        if used.saturating_add(reserved).saturating_add(bytes)
            > store::quota(&tx, actor, default_quota)?
        {
            return Err(store::problem(
                413,
                "Not enough Drive storage for this copy. Free space or ask your administrator for more storage.",
            ));
        }
        let mut mapped = std::collections::HashMap::<String, String>::new();
        for (item, depth) in tree {
            let parent = if depth == 1 {
                input.parent_uid.clone()
            } else {
                item.parent_uid
                    .as_ref()
                    .and_then(|uid| mapped.get(uid).cloned())
            };
            let uid = Uuid::new_v4().to_string();
            let version = (item.kind == "file").then(|| Uuid::new_v4().to_string());
            let name = copy_name(&tx, actor, parent.as_deref(), &item.name)?;
            tx.execute("INSERT INTO mx_drive_items(uid,owner_uid,parent_uid,name,kind,current_version_uid,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?7)",params![uid,actor,parent,name,item.kind,version,store::now()])?;
            if let Some(version) = version {
                let inserted = tx.execute("INSERT INTO mx_drive_versions(uid,item_uid,object_key,file_name,mime_type,size,sha256,actor_uid,created_at) SELECT ?1,?2,v.object_key,v.file_name,v.mime_type,v.size,v.sha256,?3,?4 FROM mx_drive_versions v JOIN mx_drive_items i ON i.current_version_uid=v.uid WHERE i.uid=?5",params![version,uid,actor,store::now(),item.uid])?;
                if inserted != 1 {
                    return Err(store::problem(
                        409,
                        "A source file has no committed version.",
                    ));
                }
            }
            store::activity_detail(
                &tx,
                &uid,
                actor,
                "item.copied",
                &json!({"source_uid":item.uid}),
            )?;
            mapped.insert(item.uid.clone(), uid.clone());
            if depth == 1 {
                result.push(store::accessible(&tx, &uid, actor, false)?);
            }
        }
    }
    let result = json!({"items":result,"action":input.action});
    tx.execute("INSERT INTO mx_drive_transfers(actor_uid,operation_uid,fingerprint,result,created_at) VALUES(?1,?2,?3,?4,?5)",params![actor,input.operation_uid,fingerprint,result.to_string(),store::now()])?;
    tx.commit()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE users(uid TEXT PRIMARY KEY); INSERT INTO users VALUES('a'),('b'),('c');").unwrap();
        store::ensure(&c).unwrap();
        c
    }
    fn folder(c: &Connection, actor: &str, name: &str, parent: Option<&str>) -> store::Item {
        store::folder(c, actor, &Uuid::new_v4().to_string(), name, parent).unwrap()
    }
    fn file(c: &Connection, parent: Option<&str>) -> store::Item {
        let uid = Uuid::new_v4().to_string();
        let version = Uuid::new_v4().to_string();
        c.execute("INSERT INTO mx_drive_items(uid,owner_uid,parent_uid,name,kind,current_version_uid,created_at,updated_at) VALUES(?1,'a',?2,'video.mp4','file',?3,1,1)",params![uid,parent,version]).unwrap();
        c.execute("INSERT INTO mx_drive_versions VALUES(?1,?2,'unique-original.mp4','video.mp4','video/mp4',100,'digest','a',1)",params![version,uid]).unwrap();
        store::accessible(c, &uid, "a", false).unwrap()
    }
    fn request(
        items: &[&store::Item],
        action: &str,
        parent: Option<&str>,
        operation: &str,
    ) -> Transfer {
        Transfer {
            operation_uid: operation.into(),
            action: action.into(),
            parent_uid: parent.map(str::to_string),
            items: items
                .iter()
                .map(|i| Source {
                    uid: i.uid.clone(),
                    base_revision: i.revision,
                })
                .collect(),
        }
    }
    #[test]
    fn copy_is_quota_checked_acl_checked_idempotent_and_reference_safe() {
        let c = setup();
        let source = file(&c, None);
        let op = Uuid::new_v4().to_string();
        assert!(apply(&c, "b", request(&[&source], "copy", None, &op), 1000).is_err());
        c.execute(
            "INSERT INTO mx_drive_grants VALUES(?1,'b','viewer')",
            [&source.uid],
        )
        .unwrap();
        assert!(apply(&c, "b", request(&[&source], "copy", None, &op), 99).is_err());
        let copied = apply(&c, "b", request(&[&source], "copy", None, &op), 100).unwrap();
        assert_eq!(
            copied,
            apply(&c, "b", request(&[&source], "copy", None, &op), 100).unwrap()
        );
        assert_eq!(copied["items"][0]["owner_uid"], "b");
        let copy_uid = copied["items"][0]["uid"].as_str().unwrap();
        assert!(store::accessible(&c, copy_uid, "a", false).is_err());
        c.execute("DELETE FROM mx_drive_items WHERE uid=?1", [&source.uid])
            .unwrap();
        assert_eq!(
            store::accessible(&c, copy_uid, "b", false)
                .unwrap()
                .object_key,
            Some("unique-original.mp4".into())
        );
        let ready = || {
            c.query_row("SELECT COUNT(*) FROM mx_drive_gc WHERE object_key NOT IN (SELECT object_key FROM mx_drive_versions)",[],|r|r.get::<_,i64>(0)).unwrap()
        };
        assert_eq!(ready(), 0);
        c.execute("DELETE FROM mx_drive_items WHERE uid=?1", [copy_uid])
            .unwrap();
        assert_eq!(ready(), 1);
        apply(&c, "b", request(&[&source], "copy", None, &op), 100).unwrap();
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM mx_drive_items", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0,
            "replay must not resurrect a purged copy"
        );
    }
    #[test]
    fn recursive_copy_deduplicates_selection_preserves_extension_and_not_sharing() {
        let c = setup();
        let root = folder(&c, "a", "References", None);
        let child = file(&c, Some(&root.uid));
        let op = Uuid::new_v4().to_string();
        let receipt = apply(&c, "a", request(&[&root, &child], "copy", None, &op), 1000).unwrap();
        assert_eq!(receipt["items"].as_array().unwrap().len(), 1);
        assert_eq!(receipt["items"][0]["name"], "References (copy)");
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM mx_drive_versions", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
        let receipt = apply(
            &c,
            "a",
            request(
                &[&child],
                "copy",
                Some(&root.uid),
                &Uuid::new_v4().to_string(),
            ),
            1000,
        )
        .unwrap();
        assert_eq!(receipt["items"][0]["name"], "video (copy).mp4");
        assert!(
            apply(
                &c,
                "a",
                request(
                    &[&root],
                    "copy",
                    Some(&root.uid),
                    &Uuid::new_v4().to_string()
                ),
                1000
            )
            .is_err()
        );
        assert!(
            apply(&c, "a", request(&[&root], "move", None, &op), 1000).is_err(),
            "idempotency payload is bound"
        );
    }
    #[test]
    fn multi_move_rolls_back_on_collision_or_stale_revision() {
        let c = setup();
        let destination = folder(&c, "a", "Destination", None);
        let first = folder(&c, "a", "First", None);
        let second = folder(&c, "a", "Second", None);
        folder(&c, "a", "Second", Some(&destination.uid));
        assert!(
            apply(
                &c,
                "a",
                request(
                    &[&first, &second],
                    "move",
                    Some(&destination.uid),
                    &Uuid::new_v4().to_string()
                ),
                1000
            )
            .is_err()
        );
        assert_eq!(store::raw(&c, &first.uid).unwrap().parent_uid, None);
        c.execute(
            "UPDATE mx_drive_items SET revision=revision+1 WHERE uid=?1",
            [&first.uid],
        )
        .unwrap();
        assert!(
            apply(
                &c,
                "a",
                request(
                    &[&first],
                    "move",
                    Some(&destination.uid),
                    &Uuid::new_v4().to_string()
                ),
                1000
            )
            .is_err()
        );
        assert_eq!(store::raw(&c, &first.uid).unwrap().parent_uid, None);
    }
    #[test]
    fn legacy_unique_blob_schema_migrates_without_data_loss_or_cleanup() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE users(uid TEXT PRIMARY KEY); INSERT INTO users VALUES('a');
          CREATE TABLE mx_drive_versions(uid TEXT PRIMARY KEY,item_uid TEXT NOT NULL REFERENCES mx_drive_items(uid) ON DELETE CASCADE,object_key TEXT NOT NULL UNIQUE,file_name TEXT NOT NULL,mime_type TEXT NOT NULL,size INTEGER NOT NULL CHECK(size>=0),sha256 TEXT NOT NULL,actor_uid TEXT NOT NULL,created_at INTEGER NOT NULL);").unwrap();
        store::ensure(&c).unwrap();
        let source = file(&c, None);
        // Recreate a populated pre-copy schema, then exercise the real migration.
        c.execute_batch("DROP TRIGGER mx_drive_version_cleanup;
          ALTER TABLE mx_drive_versions RENAME TO old_versions;
          CREATE TABLE mx_drive_versions(uid TEXT PRIMARY KEY,item_uid TEXT NOT NULL REFERENCES mx_drive_items(uid) ON DELETE CASCADE,object_key TEXT NOT NULL UNIQUE,file_name TEXT NOT NULL,mime_type TEXT NOT NULL,size INTEGER NOT NULL CHECK(size>=0),sha256 TEXT NOT NULL,actor_uid TEXT NOT NULL,created_at INTEGER NOT NULL);
          INSERT INTO mx_drive_versions SELECT * FROM old_versions; DROP TABLE old_versions;").unwrap();
        store::ensure(&c).unwrap();
        apply(
            &c,
            "a",
            request(&[&source], "copy", None, &Uuid::new_v4().to_string()),
            1000,
        )
        .unwrap();
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM mx_drive_versions", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM mx_drive_gc", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            0
        );
    }
    #[test]
    fn restore_is_metadata_only_revisioned_quota_checked_and_replay_safe() {
        let c = setup();
        let source = file(&c, None);
        let version: String = c
            .query_row(
                "SELECT uid FROM mx_drive_versions WHERE item_uid=?1",
                [&source.uid],
                |r| r.get(0),
            )
            .unwrap();
        let operation = Uuid::new_v4().to_string();
        let request = || Restore {
            operation_uid: operation.clone(),
            base_revision: 1,
            version_uid: version.clone(),
        };
        assert!(restore_version(&c, "a", &source.uid, request(), 199).is_err());
        c.execute(
            "INSERT INTO mx_drive_grants VALUES(?1,'b','viewer')",
            [&source.uid],
        )
        .unwrap();
        assert!(restore_version(&c, "b", &source.uid, request(), 1000).is_err());
        let restored = restore_version(&c, "a", &source.uid, request(), 200).unwrap();
        assert_eq!(restored["revision"], 2);
        assert_eq!(
            restored,
            restore_version(&c, "a", &source.uid, request(), 200).unwrap()
        );
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM mx_drive_versions", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            c.query_row(
                "SELECT COUNT(DISTINCT object_key) FROM mx_drive_versions",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }
}
