use super::{database, store};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Deserialize;
use uuid::Uuid;

pub(super) const LEASE_MS: i64 = 60 * 60 * 1000;
#[derive(Clone, Deserialize)]
pub(super) struct Upload {
    pub operation_uid: String,
    pub parent_uid: Option<String>,
    pub item_uid: Option<String>,
    pub base_revision: Option<i64>,
    #[serde(default)]
    pub last_modified: Option<i64>,
}
#[derive(Clone)]
pub(super) struct Reservation {
    pub actor: String,
    pub operation: String,
    pub item: String,
    pub version: String,
    pub parent: Option<String>,
    pub name: String,
    pub mime: String,
    pub size: i64,
    pub checksum: String,
    pub base: Option<i64>,
    pub attempt: String,
    pub completed: bool,
    pub retry: bool,
    pub owner: String,
    pub quota: i64,
}
impl Reservation {
    pub fn key(&self) -> String {
        store::object_key(&self.owner, &self.version, &self.name)
    }
}

pub(super) fn reserve(
    c: &Connection,
    actor: &str,
    upload: &Upload,
    file_name: &str,
    mime: &str,
    size: i64,
    checksum: &str,
    quota: i64,
) -> rusqlite::Result<Reservation> {
    reserve_inner(
        c, actor, upload, file_name, mime, size, checksum, quota, false,
    )
}
pub(super) fn reserve_inner(
    c: &Connection,
    actor: &str,
    upload: &Upload,
    file_name: &str,
    mime: &str,
    size: i64,
    checksum: &str,
    quota: i64,
    resumable: bool,
) -> rusqlite::Result<Reservation> {
    if Uuid::parse_str(&upload.operation_uid).is_err() {
        return Err(store::problem(400, "operation_uid must be a UUID."));
    }
    let name = store::name(file_name)?;
    if upload.item_uid.is_some() != upload.base_revision.is_some() {
        return Err(store::problem(
            400,
            "A new version requires item_uid and base_revision together.",
        ));
    }
    if upload.last_modified.is_some_and(|value| value < 0) {
        return Err(store::problem(400, "Invalid file modification time."));
    }
    let identity = serde_json::to_string(&(
        upload.parent_uid.clone(),
        upload.item_uid.clone(),
        upload.base_revision,
        &name,
        mime,
        size,
        checksum,
    ))
    .unwrap();
    // Like console, identify resumable selections by metadata, never a full-file scan.
    // Keep legacy operation fingerprints compatible when no modification time is sent.
    let fingerprint = store::digest(
        if let Some(modified) = upload.last_modified {
            format!("{identity}:{modified}")
        } else {
            identity
        }
        .as_bytes(),
    );
    let tx = rusqlite::Transaction::new_unchecked(c, TransactionBehavior::Immediate)?;
    let mut operation = upload.operation_uid.clone();
    let supplied: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM mx_drive_uploads WHERE actor_uid=?1 AND operation_uid=?2)",
        params![actor, operation],
        |r| r.get(0),
    )?;
    if !supplied && resumable && checksum.is_empty() && upload.last_modified.is_some() {
        // Server-owned recovery also works after a page reload with a new operation UUID.
        if let Some(existing) = tx.query_row("SELECT operation_uid FROM mx_drive_uploads WHERE actor_uid=?1 AND fingerprint=?2 AND completed=0 AND attempted_at>?3 ORDER BY attempted_at DESC LIMIT 1",params![actor,fingerprint,store::now()-LEASE_MS],|r|r.get::<_,String>(0)).optional()? { operation=existing; }
    }
    let prior:Option<(String,String,String,String,i64,bool)>=tx.query_row("SELECT fingerprint,item_uid,version_uid,attempt_uid,attempted_at,completed FROM mx_drive_uploads WHERE actor_uid=?1 AND operation_uid=?2",params![actor,operation],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
    let retry = prior.is_some();
    let mut existing_attempt = None;
    let (item_uid, version_uid) =
        if let Some((stored, item, version, attempt, attempted, completed)) = prior {
            if stored != fingerprint {
                return Err(store::problem(
                    409,
                    "This upload identifier belongs to different content. Start a new upload.",
                ));
            }
            if completed {
                let item_value = store::accessible(&tx, &item, actor, false)?;
                return Ok(Reservation {
                    actor: actor.into(),
                    operation: operation.clone(),
                    item,
                    version,
                    parent: upload.parent_uid.clone(),
                    name,
                    mime: mime.into(),
                    size,
                    checksum: checksum.into(),
                    base: upload.base_revision,
                    attempt: String::new(),
                    completed: true,
                    retry: true,
                    owner: item_value.owner_uid,
                    quota,
                });
            }
            if attempted > 0 && attempted > store::now() - LEASE_MS && !resumable {
                return Err(store::problem(
                    409,
                    "This upload is still processing. Keep the same file and retry shortly.",
                ));
            }
            if resumable {
                existing_attempt = Some(attempt);
            }
            (item, version)
        } else {
            (
                upload
                    .item_uid
                    .clone()
                    .unwrap_or_else(|| Uuid::new_v4().to_string()),
                Uuid::new_v4().to_string(),
            )
        };
    let deleted: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM mx_drive_tombstones WHERE uid=?1)",
        [&item_uid],
        |r| r.get(0),
    )?;
    if deleted {
        return Err(store::problem(
            410,
            "The destination file was permanently removed.",
        ));
    }
    let owner = if let Some(base) = upload.base_revision {
        let item = store::accessible(&tx, &item_uid, actor, false)?;
        if item.kind != "file" || item.permission == "viewer" {
            return Err(store::problem(
                403,
                "You cannot upload a version of this item.",
            ));
        }
        store::base(&item, base)?;
        item.owner_uid
    } else {
        store::parent(&tx, upload.parent_uid.as_deref(), actor, None)?;
        store::unique_name(&tx, actor, upload.parent_uid.as_deref(), &name, &item_uid)?;
        actor.into()
    };
    let used:i64=tx.query_row("SELECT COALESCE(SUM(v.size),0) FROM mx_drive_versions v JOIN mx_drive_items i ON i.uid=v.item_uid WHERE i.owner_uid=?1",[&owner],|r|r.get(0))?;
    let reserved:i64=tx.query_row("SELECT COALESCE(SUM(u.size),0) FROM mx_drive_uploads u WHERE u.completed=0 AND ABS(u.attempted_at)>?1 AND u.owner_uid=?2 AND NOT(u.actor_uid=?3 AND u.operation_uid=?4)",params![store::now()-LEASE_MS,owner,actor,operation],|r|r.get(0))?;
    if used.saturating_add(reserved).saturating_add(size) > store::quota(&tx, &owner, quota)? {
        return Err(store::problem(
            413,
            "The owner's MX Drive quota would be exceeded. Empty trash or contact your administrator.",
        ));
    }
    let attempt = existing_attempt.unwrap_or_else(|| Uuid::new_v4().to_string());
    tx.execute("INSERT INTO mx_drive_uploads(actor_uid,operation_uid,fingerprint,item_uid,version_uid,parent_uid,name,mime_type,size,sha256,base_revision,attempt_uid,attempted_at,owner_uid,object_key) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15) ON CONFLICT(actor_uid,operation_uid) DO UPDATE SET attempt_uid=excluded.attempt_uid,attempted_at=excluded.attempted_at",params![actor,operation,fingerprint,item_uid,version_uid,upload.parent_uid,name,mime,size,checksum,upload.base_revision,attempt,store::now(),owner,store::object_key(&owner,&version_uid,&name)])?;
    tx.commit()?;
    Ok(Reservation {
        actor: actor.into(),
        operation,
        item: item_uid,
        version: version_uid,
        parent: upload.parent_uid.clone(),
        name,
        mime: mime.into(),
        size,
        checksum: checksum.into(),
        base: upload.base_revision,
        attempt,
        completed: false,
        retry,
        owner,
        quota,
    })
}

pub(super) fn commit(c: &Connection, r: &Reservation) -> rusqlite::Result<store::Item> {
    let tx = rusqlite::Transaction::new_unchecked(c, TransactionBehavior::Immediate)?;
    let owned:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM mx_drive_uploads WHERE actor_uid=?1 AND operation_uid=?2 AND attempt_uid=?3 AND completed=0)",params![r.actor,r.operation,r.attempt],|row|row.get(0))?;
    if !owned {
        let completed:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM mx_drive_uploads WHERE actor_uid=?1 AND operation_uid=?2 AND completed=1)",params![r.actor,r.operation],|row|row.get(0))?;
        if completed {
            return store::accessible(&tx, &r.item, &r.actor, false);
        }
        return Err(store::problem(
            409,
            "The upload reservation changed. Retry the same operation to check its status.",
        ));
    }
    let deleted: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM mx_drive_tombstones WHERE uid=?1)",
        [&r.item],
        |row| row.get(0),
    )?;
    if deleted {
        return Err(store::problem(
            410,
            "The file was permanently removed while uploading.",
        ));
    }
    // A paused reservation can outlive its quota lease. Recheck committed usage
    // under this write transaction before allowing it to consume storage again.
    let used: i64 = tx.query_row("SELECT COALESCE(SUM(v.size),0) FROM mx_drive_versions v JOIN mx_drive_items i ON i.uid=v.item_uid WHERE i.owner_uid=?1", [&r.owner], |row| row.get(0))?;
    if used.saturating_add(r.size) > store::quota(&tx, &r.owner, r.quota)? {
        return Err(store::problem(
            413,
            "The owner's Drive quota was consumed while this upload was paused.",
        ));
    }
    if let Some(base) = r.base {
        let item = store::accessible(&tx, &r.item, &r.actor, false)?;
        store::base(&item, base)?;
        if item.permission == "viewer" {
            return Err(store::problem(
                403,
                "Your edit access was removed while uploading.",
            ));
        }
    } else {
        store::parent(&tx, r.parent.as_deref(), &r.actor, None)?;
        store::unique_name(&tx, &r.owner, r.parent.as_deref(), &r.name, &r.item)?;
        tx.execute("INSERT INTO mx_drive_items(uid,owner_uid,parent_uid,name,kind,created_at,updated_at) VALUES(?1,?2,?3,?4,'file',?5,?5)",params![r.item,r.owner,r.parent,r.name,store::now()])?;
    }
    tx.execute("INSERT INTO mx_drive_versions(uid,item_uid,object_key,file_name,mime_type,size,sha256,actor_uid,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![r.version,r.item,r.key(),r.name,r.mime,r.size,r.checksum,r.actor,store::now()])?;
    tx.execute("UPDATE mx_drive_items SET current_version_uid=?1,revision=revision+?2,updated_at=?3 WHERE uid=?4",params![r.version,i64::from(r.base.is_some()),store::now(),r.item])?;
    store::activity(
        &tx,
        &r.item,
        &r.actor,
        if r.base.is_some() {
            "version.uploaded"
        } else {
            "file.uploaded"
        },
    )?;
    tx.execute("UPDATE mx_drive_uploads SET completed=1 WHERE actor_uid=?1 AND operation_uid=?2 AND attempt_uid=?3",params![r.actor,r.operation,r.attempt])?;
    let item = store::accessible(&tx, &r.item, &r.actor, false)?;
    tx.commit()?;
    Ok(item)
}

pub(super) async fn release(r: Reservation, discard: bool) {
    let _=database(move|c|{
        let tx=rusqlite::Transaction::new_unchecked(c,TransactionBehavior::Immediate)?;
        if discard {
            // Superseded/denied destination: queue bytes only if THIS attempt still owns the reservation.
            let changed=tx.execute("DELETE FROM mx_drive_uploads WHERE actor_uid=?1 AND operation_uid=?2 AND attempt_uid=?3 AND completed=0",params![r.actor,r.operation,r.attempt])?;
            if changed>0 {tx.execute("INSERT OR IGNORE INTO mx_drive_gc(object_key,next_attempt_at,created_at) VALUES(?1,?2,?2)",params![r.key(),store::now()])?;}
        }else {tx.execute("UPDATE mx_drive_uploads SET attempted_at=?4 WHERE actor_uid=?1 AND operation_uid=?2 AND attempt_uid=?3 AND completed=0",params![r.actor,r.operation,r.attempt,-store::now()])?;}
        tx.commit()
    }).await;
    if discard {
        super::gc::wake();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys=ON;CREATE TABLE users(uid TEXT PRIMARY KEY);INSERT INTO users VALUES('a'),('b');").unwrap();
        store::ensure(&c).unwrap();
        c
    }
    fn upload() -> Upload {
        Upload {
            operation_uid: Uuid::new_v4().to_string(),
            parent_uid: None,
            item_uid: None,
            base_revision: None,
            last_modified: None,
        }
    }
    #[test]
    fn metadata_reselection_recovers_only_same_actor_file_and_destination() {
        let c = db();
        let mut first = upload();
        first.last_modified = Some(123);
        let reserve_file = |actor: &str, op: &Upload| {
            reserve_inner(&c, actor, op, "large.mp4", "video/mp4", 60, "", 1000, true).unwrap()
        };
        let original = reserve_file("a", &first);
        let mut retry = first.clone();
        retry.operation_uid = Uuid::new_v4().to_string();
        let recovered = reserve_file("a", &retry);
        assert_eq!(recovered.operation, original.operation);
        assert_eq!(recovered.attempt, original.attempt);
        assert_eq!(recovered.key(), original.key());
        assert_ne!(reserve_file("b", &retry).operation, original.operation);
        retry.last_modified = Some(124);
        assert_ne!(reserve_file("a", &retry).operation, original.operation);
        retry.operation_uid = Uuid::new_v4().to_string();
        retry.last_modified = Some(123);
        commit(&c, &original).unwrap();
        assert!(
            reserve_inner(
                &c,
                "a",
                &retry,
                "large.mp4",
                "video/mp4",
                60,
                "",
                1000,
                true
            )
            .is_err(),
            "completed files are not silently overwritten on reselection"
        );
    }
    #[test]
    fn quota_reservations_idempotency_and_atomic_revisions() {
        let c = db();
        let op = upload();
        let r = reserve(&c, "a", &op, "capture.MP4", "video/mp4", 60, "sum", 100).unwrap();
        assert!(reserve(&c, "a", &op, "capture.MP4", "video/mp4", 60, "sum", 100).is_err());
        assert!(reserve(&c, "a", &upload(), "other.mp4", "video/mp4", 60, "sum", 100).is_err());
        let item = commit(&c, &r).unwrap();
        assert!(r.key().ends_with(".MP4"));
        assert_eq!(item.revision, 1);
        assert!(reserve(&c, "a", &op, "different.mp4", "video/mp4", 60, "sum", 100).is_err());
        assert!(
            reserve(&c, "a", &op, "capture.MP4", "video/mp4", 60, "sum", 100)
                .unwrap()
                .completed
        );
        let next = Upload {
            item_uid: Some(item.uid.clone()),
            base_revision: Some(1),
            ..upload()
        };
        let r2 = reserve(&c, "a", &next, "second.webm", "video/webm", 20, "sum2", 100).unwrap();
        store::mutate(&c, "a", &item.uid, 1, "rename", Some("renamed.mov"), None).unwrap();
        assert!(commit(&c, &r2).is_err());
        assert_eq!(
            store::raw(&c, &item.uid).unwrap().object_key.unwrap(),
            r.key()
        );
    }
    #[test]
    fn current_owner_assignment_is_rechecked_for_shared_editor_commits() {
        let c = db();
        let first = reserve(
            &c,
            "a",
            &upload(),
            "Original.pdf",
            "application/pdf",
            60,
            "one",
            100,
        )
        .unwrap();
        let item = commit(&c, &first).unwrap();
        store::grant(&c, "a", &item.uid, "b", Some("editor")).unwrap();
        c.execute("INSERT INTO mx_drive_quotas(user_uid,quota_bytes,revision) VALUES('a',100,1),('b',1,1)",[]).unwrap();
        let operation = Upload {
            item_uid: Some(item.uid.clone()),
            base_revision: Some(1),
            ..upload()
        };
        let pending = reserve_inner(
            &c,
            "b",
            &operation,
            "New.pdf",
            "application/pdf",
            30,
            "two",
            100,
            true,
        )
        .unwrap();
        c.execute(
            "UPDATE mx_drive_quotas SET quota_bytes=80,revision=2 WHERE user_uid='a'",
            [],
        )
        .unwrap();
        assert!(commit(&c, &pending).is_err());
        assert_eq!(store::raw(&c, &item.uid).unwrap().revision, 1);
        c.execute(
            "UPDATE mx_drive_quotas SET quota_bytes=110,revision=3 WHERE user_uid='a'",
            [],
        )
        .unwrap();
        assert_eq!(commit(&c, &pending).unwrap().revision, 2);
    }
    #[test]
    fn empty_trash_queues_versions_and_replay_never_resurrects() {
        let c = db();
        let op = upload();
        let r = reserve(
            &c,
            "a",
            &op,
            "evidence.pdf",
            "application/pdf",
            5,
            "sum",
            100,
        )
        .unwrap();
        let item = commit(&c, &r).unwrap();
        store::mutate(&c, "a", &item.uid, 1, "trash", None, None).unwrap();
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM mx_drive_gc", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(store::purge(&c, "a", None).unwrap(), 1);
        assert_eq!(
            c.query_row("SELECT object_key FROM mx_drive_gc", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            r.key()
        );
        assert!(
            reserve(
                &c,
                "a",
                &op,
                "evidence.pdf",
                "application/pdf",
                5,
                "sum",
                100
            )
            .is_err()
        );
    }

    #[test]
    fn shared_editor_versions_use_the_owner_quota_and_original_extension() {
        let c = db();
        let first = reserve(
            &c,
            "a",
            &upload(),
            "Original.mp4",
            "video/mp4",
            60,
            "one",
            100,
        )
        .unwrap();
        let item = commit(&c, &first).unwrap();
        store::grant(&c, "a", &item.uid, "b", Some("editor")).unwrap();
        let operation = Upload {
            item_uid: Some(item.uid.clone()),
            base_revision: Some(1),
            ..upload()
        };
        assert!(
            reserve(
                &c,
                "b",
                &operation,
                "Camera.MOV",
                "video/quicktime",
                50,
                "two",
                100
            )
            .is_err()
        );
        let r = reserve(
            &c,
            "b",
            &operation,
            "Camera.MOV",
            "video/quicktime",
            30,
            "two",
            100,
        )
        .unwrap();
        assert_eq!(r.owner, "a");
        assert!(r.key().starts_with("__mx/drive/a/"));
        assert!(r.key().ends_with(".MOV"));
        store::grant(&c, "a", &item.uid, "b", None).unwrap();
        assert!(commit(&c, &r).is_err());
        assert_eq!(store::raw(&c, &item.uid).unwrap().revision, 1);
    }

    #[test]
    fn deleting_an_account_queues_its_immutable_versions() {
        let c = db();
        let r = reserve(
            &c,
            "a",
            &upload(),
            "Account.pdf",
            "application/pdf",
            5,
            "sum",
            100,
        )
        .unwrap();
        commit(&c, &r).unwrap();
        c.execute("DELETE FROM users WHERE uid='a'", []).unwrap();
        assert_eq!(
            c.query_row("SELECT object_key FROM mx_drive_gc", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            r.key()
        );
        assert!(store::raw(&c, &r.item).is_err());
    }

    #[test]
    fn canceled_durable_sessions_are_queued_before_the_reservation_disappears() {
        let c = db();
        let r = reserve(
            &c,
            "a",
            &upload(),
            "Pending.mp4",
            "video/mp4",
            5,
            "sum",
            100,
        )
        .unwrap();
        c.execute("INSERT INTO mx_drive_sessions(actor_uid,operation_uid,n1_version,n1_session,part_size) VALUES(?1,?2,'n1-version','n1-session',16777216)",params![r.actor,r.operation]).unwrap();
        c.execute(
            "DELETE FROM mx_drive_uploads WHERE actor_uid=?1 AND operation_uid=?2",
            params![r.actor, r.operation],
        )
        .unwrap();
        assert_eq!(
            c.query_row("SELECT object_key FROM mx_drive_cancel", [], |row| row
                .get::<_, String>(
                0
            ))
            .unwrap(),
            r.key()
        );
    }

    #[test]
    fn an_expired_reservation_cannot_overrun_quota_at_commit() {
        let c = db();
        let paused = reserve(
            &c,
            "a",
            &upload(),
            "paused.bin",
            "application/octet-stream",
            60,
            "first",
            100,
        )
        .unwrap();
        c.execute("UPDATE mx_drive_uploads SET attempted_at=1", [])
            .unwrap();
        let next = reserve(
            &c,
            "a",
            &upload(),
            "next.bin",
            "application/octet-stream",
            60,
            "second",
            100,
        )
        .unwrap();
        commit(&c, &next).unwrap();
        assert!(commit(&c, &paused).is_err());
        assert_eq!(
            c.query_row("SELECT SUM(size) FROM mx_drive_versions", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            60
        );
    }
}
