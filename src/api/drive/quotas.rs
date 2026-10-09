use super::{Error, database, error, store};
use crate::{config::load_config::CONFIG, middleware::auth::Claims};
use axum::{Json, extract::Path};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

pub(super) fn snapshot(c: &Connection, uid: &str) -> rusqlite::Result<Value> {
    let exists: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM users WHERE uid=?1)",
        [uid],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(store::problem(404, "Account not found."));
    }
    let row = c
        .query_row(
            "SELECT quota_bytes,revision FROM mx_drive_quotas WHERE user_uid=?1",
            [uid],
            |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;
    let default = CONFIG.drive.quota_mb as i64 * 1024 * 1024;
    let used: i64 = c.query_row("SELECT COALESCE(SUM(v.size),0) FROM mx_drive_versions v JOIN mx_drive_items i ON i.uid=v.item_uid WHERE i.owner_uid=?1", [uid], |row| row.get(0))?;
    let reserved: i64 = c.query_row("SELECT COALESCE(SUM(size),0) FROM mx_drive_uploads WHERE owner_uid=?1 AND completed=0 AND ABS(attempted_at)>?2", params![uid,store::now()-super::uploads::LEASE_MS], |row| row.get(0))?;
    let assigned = row.and_then(|r| r.0);
    Ok(
        json!({"user_uid":uid,"quota_bytes":assigned.unwrap_or(default),"default_quota_bytes":default,"assigned":assigned.is_some(),"revision":row.map(|r|r.1).unwrap_or(0),"used_bytes":used,"reserved_bytes":reserved}),
    )
}

pub(super) async fn get(claims: Claims, Path(uid): Path<String>) -> Result<Json<Value>, Error> {
    if claims.access_level != 0 {
        return Err(error(403, "Only administrators can manage Drive quotas."));
    }
    Ok(Json(database(move |c| snapshot(c, &uid)).await?))
}

#[derive(Deserialize)]
pub(super) struct Change {
    base_revision: i64,
    quota_bytes: Option<i64>,
}

fn change(
    c: &Connection,
    actor: &str,
    uid: &str,
    base: i64,
    quota: Option<i64>,
) -> rusqlite::Result<()> {
    // JSON/browser byte counts must remain exact integers; this is not a per-file limit.
    if base < 0 || quota.is_some_and(|bytes| !(0..=9_007_199_254_740_991).contains(&bytes)) {
        return Err(store::problem(
            400,
            "Quota must be a non-negative, exactly representable byte count.",
        ));
    }
    let tx = rusqlite::Transaction::new_unchecked(c, TransactionBehavior::Immediate)?;
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM users WHERE uid=?1)",
        [uid],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(store::problem(404, "Account not found."));
    }
    let prior = tx
        .query_row(
            "SELECT quota_bytes,revision FROM mx_drive_quotas WHERE user_uid=?1",
            [uid],
            |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;
    if prior.map(|r| r.1).unwrap_or(0) != base {
        return Err(store::problem(
            409,
            "Another administrator changed this quota. Reload storage settings before saving.",
        ));
    }
    tx.execute("INSERT INTO mx_drive_quotas(user_uid,quota_bytes,revision) VALUES(?1,?2,1) ON CONFLICT(user_uid) DO UPDATE SET quota_bytes=excluded.quota_bytes,revision=mx_drive_quotas.revision+1", params![uid,quota])?;
    tx.execute("INSERT INTO mx_drive_quota_activity(uid,user_uid,actor_uid,previous_bytes,quota_bytes,created_at) VALUES(?1,?2,?3,?4,?5,?6)", params![Uuid::new_v4().to_string(),uid,actor,prior.and_then(|r|r.0),quota,store::now()])?;
    tx.commit()
}

pub(super) async fn set(
    claims: Claims,
    Path(uid): Path<String>,
    Json(value): Json<Change>,
) -> Result<Json<Value>, Error> {
    if claims.access_level != 0 {
        return Err(error(403, "Only administrators can manage Drive quotas."));
    }
    Ok(Json(
        database(move |c| {
            change(c, &claims.uid, &uid, value.base_revision, value.quota_bytes)?;
            snapshot(c, &uid)
        })
        .await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn assigned_quota_is_per_owner_revisioned_and_audited_without_removing_files() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE users(uid TEXT PRIMARY KEY); INSERT INTO users VALUES('a'),('b');").unwrap();
        store::ensure(&c).unwrap();
        assert_eq!(store::quota(&c, "a", 10_000).unwrap(), 10_000);
        change(&c, "admin", "a", 0, Some(500)).unwrap();
        assert_eq!(store::quota(&c, "a", 10_000).unwrap(), 500);
        assert_eq!(store::quota(&c, "b", 10_000).unwrap(), 10_000);
        assert!(change(&c, "admin", "a", 0, Some(900)).is_err());
        assert!(change(&c, "admin", "a", 1, Some(-1)).is_err());
        change(&c, "admin", "a", 1, Some(0)).unwrap();
        assert_eq!(store::quota(&c, "a", 10_000).unwrap(), 0);
        change(&c, "admin", "a", 2, None).unwrap();
        assert_eq!(store::quota(&c, "a", 10_000).unwrap(), 10_000);
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM mx_drive_quota_activity", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            3
        );
    }
}
