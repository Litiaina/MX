use super::{Error, database, store};
use crate::{
    api::{
        live::publish_user_event,
        notifications::{NewNotification, Notification, deliver_notification, insert_notification},
    },
    middleware::auth::Claims,
};
use axum::{
    Json,
    extract::{Path, Query},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Default)]
pub(super) struct ShareDelivery {
    pub changed: bool,
    pub channel_uid: Option<String>,
    pub message_uid: Option<String>,
    recipients: Vec<String>,
    notifications: Vec<(String, Notification)>,
}
impl ShareDelivery {
    pub fn publish(&self) {
        for recipient in &self.recipients {
            // Only identifiers travel over live events. Cards are resolved with
            // each viewer's current ACL, not the owner's permissions.
            publish_user_event(
                recipient,
                "file.sharing.changed",
                None,
                json!({"channel_uid":self.channel_uid}),
            );
            if let Some(uid) = &self.message_uid {
                publish_user_event(
                    recipient,
                    "message.created",
                    None,
                    json!({"uid":uid,"channel_uid":self.channel_uid}),
                );
            }
        }
        for (recipient, notification) in &self.notifications {
            deliver_notification(recipient, notification);
        }
    }
}

/// The caller owns an IMMEDIATE transaction: access, message, activity and
/// durable notifications either all commit or all roll back together.
pub(super) fn deliver(
    c: &Connection,
    actor: &str,
    item_uid: &str,
    user: Option<&str>,
    space: Option<&str>,
    role: Option<&str>,
    added: bool,
) -> rusqlite::Result<ShareDelivery> {
    let channel_uid = if let Some(space) = space {
        space.to_owned()
    } else {
        let user = user.ok_or_else(|| store::problem(400, "Choose a recipient."))?;
        let mut pair = [actor, user];
        pair.sort();
        let key = format!("{}:{}", pair[0], pair[1]);
        let existing: Option<String> = c
            .query_row(
                "SELECT uid FROM mx_channels WHERE direct_key=?1",
                [&key],
                |r| r.get(0),
            )
            .optional()?;
        match existing {
            Some(uid) => uid,
            None if role.is_some() => {
                let uid = Uuid::new_v4().to_string();
                c.execute("INSERT INTO mx_channels(uid,kind,name,direct_key,created_by,created_at) VALUES(?1,'direct','Direct message',?2,?3,?4)",params![uid,key,actor,store::now()])?;
                for member in pair {
                    c.execute("INSERT INTO mx_channel_members(channel_uid,user_uid,role,joined_at) VALUES(?1,?2,'member',?3)",params![uid,member,store::now()])?;
                }
                uid
            }
            None => {
                return Ok(ShareDelivery {
                    changed: true,
                    recipients: vec![actor.into(), user.into()],
                    ..Default::default()
                });
            }
        }
    };
    let members = c
        .prepare("SELECT user_uid,notification_level FROM mx_channel_members WHERE channel_uid=?1")?
        .query_map([&channel_uid], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut delivery = ShareDelivery {
        changed: true,
        channel_uid: Some(channel_uid.clone()),
        recipients: members.iter().map(|m| m.0.clone()).collect(),
        ..Default::default()
    };
    if !delivery.recipients.iter().any(|uid| uid == actor) {
        delivery.recipients.push(actor.into());
    }
    let Some(role) = role else {
        return Ok(delivery);
    };
    let item = store::owner(c, item_uid, actor, false)?;
    let message_uid = Uuid::new_v4().to_string();
    let body = if added {
        format!("Shared an MX Drive {} · {role} access", item.kind)
    } else {
        format!("Updated MX Drive {} access to {role}", item.kind)
    };
    c.execute("INSERT INTO mx_messages(uid,channel_uid,sender_uid,body,created_at) VALUES(?1,?2,?3,?4,?5)",params![message_uid,channel_uid,actor,body,store::now()])?;
    c.execute(
        "INSERT INTO mx_message_drive_links(message_uid,item_uid) VALUES(?1,?2)",
        params![message_uid, item_uid],
    )?;
    let actor_name: String =
        c.query_row("SELECT name FROM users WHERE uid=?1", [actor], |r| r.get(0))?;
    for (recipient, level) in members {
        // A direct recipient is explicitly addressed. Group shares follow the
        // group's all/mentions/muted settings and the user's file preferences.
        if recipient == actor || level == "muted" || (space.is_some() && level == "mentions") {
            continue;
        }
        if let Some(notification) = insert_notification(
            c,
            &NewNotification {
                recipient_uid: recipient.clone(),
                kind: "file.shared".into(),
                title: format!(
                    "{actor_name} {} a Drive {}",
                    if added { "shared" } else { "updated" },
                    item.kind
                ),
                body: format!("{} · {role} access", item.name),
                actor_uid: Some(actor.into()),
                target_type: Some("channel".into()),
                target_uid: Some(channel_uid.clone()),
                module_uid: None,
                data: json!({"message_uid":message_uid,"channel_uid":channel_uid,"drive_item_uid":item_uid}),
                dedup_key: Some(format!("drive-share:{message_uid}")),
                mandatory: false,
            },
        )? {
            delivery.notifications.push((recipient, notification));
        }
    }
    delivery.message_uid = Some(message_uid);
    Ok(delivery)
}

pub(crate) fn message_links(
    c: &Connection,
    message: &str,
    actor: &str,
) -> rusqlite::Result<Vec<Value>> {
    let uids = c
        .prepare(
            "SELECT item_uid FROM mx_message_drive_links WHERE message_uid=?1 ORDER BY item_uid",
        )?
        .query_map([message], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    uids.into_iter()
        .map(|uid| match store::accessible(c, &uid, actor, false) {
            Ok(item) => Ok(json!({"uid":uid,"item":item,"unavailable":false})),
            Err(rusqlite::Error::InvalidParameterName(error))
                if error.starts_with("DRIVE:404:") =>
            {
                Ok(json!({"uid":uid,"unavailable":true}))
            }
            Err(error) => Err(error),
        })
        .collect()
}
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
) -> rusqlite::Result<ShareDelivery> {
    crate::api::collaboration::ensure_collaboration_schema(c)?;
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
        if previous.as_deref() == Some(role) {
            return Ok(ShareDelivery::default());
        }
        tx.execute("INSERT INTO mx_drive_space_grants(item_uid,space_uid,role) VALUES(?1,?2,?3) ON CONFLICT(item_uid,space_uid) DO UPDATE SET role=excluded.role",params![uid,space,role])?;
    } else {
        if previous.is_none() {
            return Ok(ShareDelivery::default());
        }
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
    let delivery = deliver(&tx, actor, uid, None, Some(space), role, previous.is_none())?;
    tx.commit()?;
    Ok(delivery)
}

#[derive(Deserialize)]
pub(super) struct SharedPage {
    #[serde(default)]
    q: String,
    #[serde(default)]
    offset: usize,
}
/// Includes old grants too: opening a conversation's files does not depend on
/// whether a share message was sent by an older MX release.
pub(super) async fn conversation_items(
    claims: Claims,
    Path(channel): Path<String>,
    Query(q): Query<SharedPage>,
) -> Result<Json<Value>, Error> {
    if q.q.len() > 240 {
        return Err(super::error(400, "Search must be at most 240 bytes."));
    }
    Ok(Json(database(move|c|{
        let tx=rusqlite::Transaction::new_unchecked(c,TransactionBehavior::Deferred)?;
        let member:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM mx_channel_members m JOIN mx_channels s ON s.uid=m.channel_uid WHERE m.channel_uid=?1 AND m.user_uid=?2 AND s.archived_at IS NULL)",params![channel,claims.uid],|r|r.get(0))?;
        if !member {return Err(store::problem(403,"You are not a member of this conversation."));}
        let cte=r#"WITH RECURSIVE roots(uid) AS (
            SELECT g.item_uid FROM mx_drive_space_grants g WHERE g.space_uid=?1
            UNION SELECT g.item_uid FROM mx_drive_grants g JOIN mx_drive_items i ON i.uid=g.item_uid
            JOIN mx_channels s ON s.uid=?1 AND s.kind='direct'
            JOIN mx_channel_members owner ON owner.channel_uid=s.uid AND owner.user_uid=i.owner_uid
            JOIN mx_channel_members recipient ON recipient.channel_uid=s.uid AND recipient.user_uid=g.user_uid
            WHERE owner.user_uid<>recipient.user_uid
        ), hidden(uid) AS (
            SELECT uid FROM mx_drive_items WHERE trashed_at IS NOT NULL AND owner_uid IN (SELECT i.owner_uid FROM mx_drive_items i JOIN roots r ON r.uid=i.uid)
            UNION SELECT i.uid FROM mx_drive_items i JOIN hidden h ON i.parent_uid=h.uid
        ) "#;
        let filter="i.uid IN roots AND i.uid NOT IN hidden AND (?2='' OR INSTR(LOWER(i.name),LOWER(?2))>0)";
        let total:i64=tx.query_row(&format!("{cte} SELECT COUNT(*) FROM mx_drive_items i WHERE {filter}"),params![channel,q.q],|r|r.get(0))?;
        let offset=q.offset.min((total as usize).saturating_sub(1)/50*50);
        let uids=tx.prepare(&format!("{cte} SELECT i.uid FROM mx_drive_items i WHERE {filter} ORDER BY i.kind DESC,i.updated_at DESC,i.uid LIMIT 50 OFFSET ?3"))?
            .query_map(params![channel,q.q,offset as i64],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let items=uids.iter().map(|uid|store::accessible(&tx,uid,&claims.uid,false)).collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(json!({"items":items,"total":total,"offset":offset,"limit":50,"has_more":offset+items.len()<total as usize}))
    }).await?))
}
pub(super) fn details(c: &Connection, uid: &str) -> rusqlite::Result<Vec<Value>> {
    let mut s=c.prepare("SELECT g.space_uid,s.name,s.kind,g.role,s.archived_at,(SELECT COUNT(*) FROM mx_channel_members m WHERE m.channel_uid=s.uid) FROM mx_drive_space_grants g JOIN mx_channels s ON s.uid=g.space_uid WHERE g.item_uid=?1 ORDER BY s.name COLLATE NOCASE")?;
    s.query_map([uid],|r|Ok(json!({"space_uid":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"kind":r.get::<_,String>(2)?,"role":r.get::<_,String>(3)?,"archived":r.get::<_,Option<i64>>(4)?.is_some(),"members":r.get::<_,i64>(5)?})))?.collect()
}
