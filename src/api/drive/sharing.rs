use super::{Error, database, store};
use crate::middleware::auth::Claims;
use axum::{Json, extract::Query};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
pub(super) struct Search {
    #[serde(default)]
    q: String,
}
pub(super) async fn spaces(claims: Claims, Query(q): Query<Search>) -> Result<Json<Value>, Error> {
    if q.q.len() > 240 {
        return Err(super::error(400, "Search must be at most 240 bytes."));
    }
    Ok(Json(database(move|c|{
        let mut s=c.prepare("SELECT s.uid,s.name,s.kind,(SELECT COUNT(*) FROM mx_channel_members x WHERE x.channel_uid=s.uid) FROM mx_channel_members m JOIN mx_channels s ON s.uid=m.channel_uid WHERE m.user_uid=?1 AND s.kind<>'direct' AND s.archived_at IS NULL AND INSTR(LOWER(s.name),LOWER(?2))>0 ORDER BY s.name COLLATE NOCASE,s.uid LIMIT 30")?;
        let spaces=s.query_map(params![claims.uid,q.q],|r|Ok(json!({"uid":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"kind":r.get::<_,String>(2)?,"members":r.get::<_,i64>(3)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(json!({"spaces":spaces}))
    }).await?))
}
pub(super) fn grant(
    c: &Connection,
    actor: &str,
    uid: &str,
    space: &str,
    role: Option<&str>,
) -> rusqlite::Result<()> {
    let tx = rusqlite::Transaction::new_unchecked(c, TransactionBehavior::Immediate)?;
    store::owner(&tx, uid, actor, false)?;
    let previous: Option<String> = tx
        .query_row(
            "SELECT role FROM mx_drive_space_grants WHERE item_uid=?1 AND space_uid=?2",
            params![uid, space],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(role) = role {
        if !["viewer", "editor"].contains(&role) {
            return Err(store::problem(400, "Choose viewer or editor access."));
        }
        let member:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM mx_channel_members m JOIN mx_channels s ON s.uid=m.channel_uid WHERE m.user_uid=?1 AND s.uid=?2 AND s.kind<>'direct' AND s.archived_at IS NULL)",params![actor,space],|r|r.get(0))?;
        if !member {
            return Err(store::problem(
                404,
                "Choose an active collaboration space you belong to.",
            ));
        }
        tx.execute("INSERT INTO mx_drive_space_grants(item_uid,space_uid,role) VALUES(?1,?2,?3) ON CONFLICT(item_uid,space_uid) DO UPDATE SET role=excluded.role",params![uid,space,role])?;
    } else {
        tx.execute(
            "DELETE FROM mx_drive_space_grants WHERE item_uid=?1 AND space_uid=?2",
            params![uid, space],
        )?;
    }
    store::activity_detail(
        &tx,
        uid,
        actor,
        "sharing.changed",
        &json!({"space_uid":space,"previous_role":previous,"new_role":role}),
    )?;
    tx.commit()
}
pub(super) fn details(c: &Connection, uid: &str) -> rusqlite::Result<Vec<Value>> {
    let mut s=c.prepare("SELECT g.space_uid,s.name,s.kind,g.role,s.archived_at,(SELECT COUNT(*) FROM mx_channel_members m WHERE m.channel_uid=s.uid) FROM mx_drive_space_grants g JOIN mx_channels s ON s.uid=g.space_uid WHERE g.item_uid=?1 ORDER BY s.name COLLATE NOCASE")?;
    s.query_map([uid],|r|Ok(json!({"space_uid":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"kind":r.get::<_,String>(2)?,"role":r.get::<_,String>(3)?,"archived":r.get::<_,Option<i64>>(4)?.is_some(),"members":r.get::<_,i64>(5)?})))?.collect()
}
