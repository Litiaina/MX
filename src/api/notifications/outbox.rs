//! Durable notification work. Foreground transactions write a small job rather
//! than doing N recipients' index writes or spawning N blocking writer tasks.
use super::*;

#[derive(Serialize, Deserialize)]
enum Job {
    Module(NewNotification),
    Direct(Vec<NewNotification>),
}

pub(super) fn ensure(c: &rusqlite::Connection) -> rusqlite::Result<()> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS mx_notification_outbox(
        id INTEGER PRIMARY KEY, actor_uid TEXT NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
        job_json TEXT NOT NULL);",
    )
}

fn enqueue(c: &rusqlite::Connection, actor: &str, job: &Job) -> rusqlite::Result<()> {
    ensure_notification_schema(c)?;
    let text = serde_json::to_string(job)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    c.execute(
        "INSERT INTO mx_notification_outbox(actor_uid,job_json) VALUES(?1,?2)",
        params![actor, text],
    )?;
    Ok(())
}

pub(super) fn enqueue_module(
    c: &rusqlite::Connection,
    request: NewNotification,
) -> rusqlite::Result<()> {
    let actor = request.actor_uid.clone().unwrap_or_default();
    enqueue(c, &actor, &Job::Module(request))
}

pub(crate) fn queue_notifications(
    c: &rusqlite::Connection,
    requests: Vec<NewNotification>,
) -> rusqlite::Result<()> {
    if requests.is_empty() {
        return Ok(());
    }
    let actor = requests[0].actor_uid.clone().unwrap_or_default();
    enqueue(c, &actor, &Job::Direct(requests))
}

pub(crate) fn queue_record_activity(
    c: &rusqlite::Connection,
    module: &str,
    actor: &str,
    record: &str,
    kind: &str,
) -> rusqlite::Result<()> {
    let created = kind == "record.created";
    enqueue_module(
        c,
        NewNotification {
            recipient_uid: String::new(),
            kind: kind.into(),
            title: if created {
                "Record created"
            } else {
                "Record updated"
            }
            .into(),
            body: if created {
                "A record was added to a module you can access."
            } else {
                "Fields changed on a record in a module you can access."
            }
            .into(),
            actor_uid: Some(actor.into()),
            target_type: Some("record".into()),
            target_uid: Some(record.into()),
            module_uid: Some(module.into()),
            data: json!({"record_uid":record,"module_uid":module}),
            dedup_key: Some(if created {
                format!("record-created:{module}:{record}")
            } else {
                format!(
                    "record-activity:{kind}:{module}:{record}::{actor}:{}",
                    chrono::Utc::now().timestamp() / 300
                )
            }),
            mandatory: false,
        },
    )
}

// The notification rows and removal of their jobs commit together. A crash at
// ANY earlier point leaves the entire batch pending for startup recovery.
pub(super) fn drain(c: &rusqlite::Connection) -> rusqlite::Result<Vec<(String, Notification)>> {
    let tx = rusqlite::Transaction::new_unchecked(c, rusqlite::TransactionBehavior::Immediate)?;
    let jobs = tx
        .prepare("SELECT id,job_json FROM mx_notification_outbox ORDER BY id LIMIT 8")?
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut deliveries = Vec::new();
    let mut batch = NotificationBatch::default();
    for (id, text) in jobs {
        let job: Job = serde_json::from_str(&text).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
        })?;
        match job {
            Job::Module(request) => deliveries.extend(insert_module_notifications_in_transaction(
                &tx, &request, &mut batch,
            )?),
            Job::Direct(requests) => {
                for request in requests {
                    // A user may be removed/deleted or mute a conversation between
                    // commit and delivery. Do not leak queued message notifications.
                    let exists: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM users WHERE uid=?1)",
                        [&request.recipient_uid],
                        |r| r.get(0),
                    )?;
                    if !exists {
                        continue;
                    }
                    if let Some(module) = request.module_uid.as_deref() {
                        let level=crate::api::mx::privacy::level(&tx,&request.recipient_uid)?;
                        if !crate::api::modules::module_can_for_user_initialized(&tx,module,&request.recipient_uid,level,"read")? {continue;}
                        if request.target_type.as_deref()==Some("record") && !crate::api::mx::privacy::field_record_allowed(&tx,request.target_uid.as_deref().unwrap_or(""),request.data["attachment_field_uid"].as_str().unwrap_or(""),&request.recipient_uid,level)? {continue;}
                    }
                    if request.target_type.as_deref() == Some("channel")
                        && (request.kind == "message.created" || request.kind == "mention.created")
                    {
                        let level:Option<String> = tx.query_row("SELECT notification_level FROM mx_channel_members WHERE channel_uid=?1 AND user_uid=?2",params![request.target_uid,request.recipient_uid],|r|r.get(0)).optional()?;
                        if level.as_deref().is_none_or(|v| {
                            v == "muted" || (v == "mentions" && request.kind != "mention.created")
                        }) {
                            continue;
                        }
                    }
                    if let Some(notification) = batch.insert(&tx, &request)? {
                        deliveries.push((request.recipient_uid, notification));
                    }
                }
            }
        }
        tx.execute("DELETE FROM mx_notification_outbox WHERE id=?1", [id])?;
    }
    tx.commit()?;
    Ok(deliveries)
}

pub(crate) fn start_delivery_worker() {
    tokio::spawn(async {
        loop {
            let work = tokio::task::spawn_blocking(|| {
                with_sql_connection(|c| {
                    c.query_row(
                        "SELECT EXISTS(SELECT 1 FROM mx_notification_outbox)",
                        [],
                        |r| r.get::<_, bool>(0),
                    )
                })
            })
            .await;
            match work {
                Ok(Ok(true)) => match tokio::task::spawn_blocking(|| with_sql_write(drain)).await {
                    Ok(Ok(deliveries)) => {
                        for (uid, notification) in deliveries {
                            deliver_notification(&uid, &notification);
                        }
                    }
                    other => {
                        tracing::error!(
                            ?other,
                            "Notification outbox delivery failed; jobs retained for retry"
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                },
                Ok(Ok(false)) => tokio::time::sleep(std::time::Duration::from_millis(250)).await,
                other => {
                    tracing::error!(?other, "Notification outbox lookup failed");
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn outbox_is_atomic_recoverable_and_rechecks_permissions() {
        let c = super::super::tests::fanout_fixture();
        {
            let tx = c.unchecked_transaction().unwrap();
            queue_record_activity(&tx, "capacity", "actor", "record", "record.created").unwrap();
            // Simulates a record transaction that never committed.
        }
        assert_eq!(
            c.query_row("SELECT count(*) FROM mx_notification_outbox", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        queue_record_activity(&c, "capacity", "actor", "record", "record.created").unwrap();
        c.execute_batch("CREATE TRIGGER fail_outbox_delivery BEFORE INSERT ON mx_notifications BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
        assert!(drain(&c).is_err());
        assert_eq!(
            c.query_row("SELECT count(*) FROM mx_notification_outbox", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        c.execute_batch("DROP TRIGGER fail_outbox_delivery; UPDATE mx_module_account_permissions SET can_read=0 WHERE user_uid='reader';").unwrap();
        let sent = drain(&c).unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, "admin");
        assert!(drain(&c).unwrap().is_empty());
        assert_eq!(
            c.query_row("SELECT count(*) FROM mx_notifications", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}
