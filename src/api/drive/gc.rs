use super::{database, store};
use crate::api::mx::handler::{n1_access_token, n1_soft_delete, n1_storage};
use rusqlite::params;
use std::{sync::LazyLock, time::Duration};
use tokio::sync::Notify;
static WAKE: LazyLock<Notify> = LazyLock::new(Notify::new);
pub(super) fn wake() {
    WAKE.notify_one();
}

pub(crate) async fn run() {
    loop {
        if let Err(failure) = batch().await {
            tracing::warn!(status=%failure.0,"Drive cleanup deferred; durable jobs retained");
        }
        tokio::select! {_=WAKE.notified()=>{},_=tokio::time::sleep(Duration::from_secs(10))=>{}}
    }
}
async fn batch() -> Result<(), super::Error> {
    let cancellations=database(|c|{
      let mut s=c.prepare("SELECT object_key,n1_version,coordinator,attempts FROM mx_drive_cancel WHERE next_attempt_at<=?1 ORDER BY next_attempt_at LIMIT 8")?;
      s.query_map([store::now()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?,r.get::<_,u32>(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()
    }).await?;
    for (key, version, coordinator, attempts) in cancellations {
        let success = match n1_access_token().await {
            Ok(token) => match n1_storage(&token) {
                Ok(storage) => storage
                    .durable_request(
                        reqwest::Method::DELETE,
                        &key,
                        &version,
                        "",
                        coordinator.as_deref(),
                        None,
                        None,
                    )
                    .await
                    .is_ok(),
                Err(_) => false,
            },
            Err(_) => false,
        };
        database(move|c|{if success {c.execute("DELETE FROM mx_drive_cancel WHERE object_key=?1 AND n1_version=?2",params![key,version])?;}else{c.execute("UPDATE mx_drive_cancel SET attempts=attempts+1,next_attempt_at=?1 WHERE object_key=?2 AND n1_version=?3",params![store::now()+(5_000i64*2i64.pow(attempts.min(10))).min(3_600_000),key,version])?;}Ok(())}).await?;
    }
    let jobs=database(|c|{
        let tx=rusqlite::Transaction::new_unchecked(c,rusqlite::TransactionBehavior::Immediate)?;
        // Abandoned reservations are not permanent storage leaks after process crashes.
        // A negative attempted_at marks a retryable failure, retaining its quota/identity for one hour.
        let mut s=tx.prepare("SELECT actor_uid,operation_uid,owner_uid,version_uid,name FROM mx_drive_uploads WHERE completed=0 AND ABS(attempted_at)<?1 LIMIT 8")?;
        let abandoned=s.query_map([store::now()-super::uploads::LEASE_MS],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;drop(s);
        for (actor,operation,owner,version,name) in abandoned {
            tx.execute("INSERT OR IGNORE INTO mx_drive_gc(object_key,next_attempt_at,created_at) VALUES(?1,?2,?2)",params![store::object_key(&owner,&version,&name),store::now()])?;
            tx.execute("DELETE FROM mx_drive_uploads WHERE actor_uid=?1 AND operation_uid=?2",params![actor,operation])?;
        }
        let mut s=tx.prepare("SELECT object_key,attempts FROM mx_drive_gc WHERE next_attempt_at<=?1 AND object_key NOT IN (SELECT object_key FROM mx_drive_versions) ORDER BY next_attempt_at LIMIT 8")?;
        let jobs=s.query_map([store::now()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,u32>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;drop(s);tx.commit()?;Ok(jobs)
    }).await?;
    for (key, attempts) in jobs {
        // N1 DELETE is soft deletion. N1 retention/TTL, not MX, reclaims bytes.
        let ok = n1_soft_delete(&key).await.is_ok();
        database(move|c|{
            if ok {c.execute("DELETE FROM mx_drive_gc WHERE object_key=?1",[key])?;}else{
                let delay=(5_000i64*2i64.pow(attempts.min(10))).min(3_600_000);
                c.execute("UPDATE mx_drive_gc SET attempts=attempts+1,next_attempt_at=?1 WHERE object_key=?2",params![store::now()+delay,key])?;
            }Ok(())
        }).await?;
    }
    Ok(())
}
