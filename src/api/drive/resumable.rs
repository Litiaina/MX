//! Browser-to-MX parts are forwarded to restart-persistent N1 multipart sessions.
//! Metadata/ACL/quota/CAS remain authoritative in MX. No N1 credential is exposed.
use super::{
    Error, database, error, store,
    uploads::{Reservation, Upload, commit, reserve_inner},
};
use crate::{
    api::mx::handler::{n1_access_token, n1_storage},
    config::load_config::CONFIG,
    middleware::auth::Claims,
};
use axum::{Json, extract::Path};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::LazyLock;
use tokio::sync::Semaphore;
static TRANSFERS: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(8));
pub(super) const MAX_PART_BYTES: usize = 50 * 1024 * 1024;
// Match console's default policy, including the strict >1 GiB boundary.
fn multipart_policy(size: u64) -> (u64, usize) {
    const MIB: u64 = 1024 * 1024;
    if size > 1024 * MIB {
        (50 * MIB, 4)
    } else if size < 16 * MIB {
        (size.max(1), 1)
    } else if size < 256 * MIB {
        (4 * MIB, 2)
    } else {
        (8 * MIB, 3)
    }
}
#[derive(Deserialize)]
pub(super) struct Init {
    #[serde(flatten)]
    upload: Upload,
    file_name: String,
    mime_type: String,
    size: i64,
    #[serde(default)]
    sha256: String,
}
#[derive(Clone)]
struct Session {
    reservation: Reservation,
    version: String,
    session: String,
    coordinator: Option<String>,
    part_size: i64,
}
fn load(c: &Connection, actor: &str, operation: &str) -> rusqlite::Result<Session> {
    let r=c.query_row("SELECT item_uid,version_uid,parent_uid,name,mime_type,size,sha256,base_revision,attempt_uid,completed,owner_uid FROM mx_drive_uploads WHERE actor_uid=?1 AND operation_uid=?2",params![actor,operation],|row|Ok(Reservation {actor:actor.into(),operation:operation.into(),item:row.get(0)?,version:row.get(1)?,parent:row.get(2)?,name:row.get(3)?,mime:row.get(4)?,size:row.get(5)?,checksum:row.get(6)?,base:row.get(7)?,attempt:row.get(8)?,completed:row.get(9)?,retry:true,owner:row.get(10)?,quota:CONFIG.drive.quota_mb as i64*1024*1024})).optional()?.ok_or_else(||store::problem(404,"Upload not found. Start or reselect the file."))?;
    if r.completed {
        store::accessible(c, &r.item, actor, false)?;
    } else if let Some(base) = r.base {
        let item = store::accessible(c, &r.item, actor, false)?;
        store::base(&item, base)?;
        if item.permission == "viewer" {
            return Err(store::problem(403, "Your edit access was removed."));
        }
    } else {
        store::parent(c, r.parent.as_deref(), actor, None)?;
    }
    let session=c.query_row("SELECT n1_version,n1_session,coordinator,part_size FROM mx_drive_sessions WHERE actor_uid=?1 AND operation_uid=?2",params![actor,operation],|row|Ok(Session {reservation:r.clone(),version:row.get(0)?,session:row.get(1)?,coordinator:row.get(2)?,part_size:row.get(3)?})).optional()?.ok_or_else(||store::problem(409,"Upload initialization is still pending. Retry the same file."))?;
    c.execute("UPDATE mx_drive_uploads SET attempted_at=?3 WHERE actor_uid=?1 AND operation_uid=?2 AND completed=0",params![actor,operation,store::now()])?;
    Ok(session)
}
pub(super) async fn init(claims: Claims, Json(value): Json<Init>) -> Result<Json<Value>, Error> {
    let _permit = TRANSFERS
        .try_acquire()
        .map_err(|_| error(429, "Other transfers are busy. Retry shortly."))?;
    if value.size < 0
        || (!value.sha256.is_empty()
            && (value.sha256.len() != 64 || !value.sha256.bytes().all(|b| b.is_ascii_hexdigit())))
    {
        return Err(error(400, "Invalid file size or SHA-256 checksum."));
    }
    if value.mime_type.len() > 200 || value.mime_type.chars().any(char::is_control) {
        return Err(error(400, "Invalid content type."));
    }
    let actor = claims.uid.clone();
    let r = database(move |c| {
        reserve_inner(
            c,
            &actor,
            &value.upload,
            &value.file_name,
            &value.mime_type,
            value.size,
            &value.sha256.to_ascii_lowercase(),
            CONFIG.drive.quota_mb as i64 * 1024 * 1024,
            true,
        )
    })
    .await?;
    let operation = r.operation.clone();
    if r.completed {
        return Ok(Json(
            json!({"completed":true,"item":database(move|c|store::accessible(c,&r.item,&r.actor,false)).await?}),
        ));
    }
    let actor = claims.uid.clone();
    let op = operation.clone();
    let exists=database(move|c|c.query_row("SELECT EXISTS(SELECT 1 FROM mx_drive_sessions WHERE actor_uid=?1 AND operation_uid=?2)",params![actor,op],|row|row.get::<_,bool>(0))).await?;
    if !exists {
        let token = n1_access_token()
            .await
            .map_err(|_| error(502, "N1 authentication is temporarily unavailable."))?;
        let (part_size, _) = multipart_policy(r.size as u64);
        // N1 init is idempotent for the same unique key and upload shape, including lost responses.
        let session = n1_storage(&token)
            .map_err(|_| error(502, "N1 is unavailable."))?
            .durable_init(&r.key(), &r.mime, r.size as u64, part_size)
            .await
            .map_err(|_| {
                error(
                    502,
                    "N1 could not initialize the resumable upload. Retry the same file.",
                )
            })?;
        let version = session["version_id"]
            .as_str()
            .ok_or_else(|| error(502, "N1 returned an invalid session."))?
            .to_owned();
        let session_id = session["session_id"]
            .as_str()
            .ok_or_else(|| error(502, "N1 returned an invalid session."))?
            .to_owned();
        let coordinator = session
            .get("coordinator_node_id")
            .or_else(|| {
                session
                    .get("gateway")
                    .and_then(|gateway| gateway.get("coordinator_node_id"))
            })
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let reservation = r.clone();
        database(move|c| {
        let tx=rusqlite::Transaction::new_unchecked(c,rusqlite::TransactionBehavior::Immediate)?;
        let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM mx_drive_uploads WHERE actor_uid=?1 AND operation_uid=?2 AND attempt_uid=?3 AND completed=0)",params![reservation.actor,reservation.operation,reservation.attempt],|row|row.get(0))?;
        if !valid {
            // Cancellation can arrive while N1 init is in flight, before our session row exists.
            tx.execute("INSERT OR IGNORE INTO mx_drive_cancel(object_key,n1_version,coordinator,next_attempt_at) VALUES(?1,?2,?3,0)",params![reservation.key(),version,coordinator])?;
            tx.commit()?;super::gc::wake();
            return Err(store::problem(409,"This upload was canceled or superseded."));
        }
        tx.execute("INSERT OR IGNORE INTO mx_drive_sessions(actor_uid,operation_uid,n1_version,n1_session,coordinator,part_size) VALUES(?1,?2,?3,?4,?5,?6)",params![reservation.actor,reservation.operation,version,session_id,coordinator,part_size as i64])?;tx.commit()
      }).await?;
    }
    status(claims, Path(operation)).await
}
pub(super) async fn status(
    claims: Claims,
    Path(operation): Path<String>,
) -> Result<Json<Value>, Error> {
    let session = database(move |c| load(c, &claims.uid, &operation)).await?;
    let r = &session.reservation;
    if r.completed {
        let r = r.clone();
        return Ok(Json(
            json!({"completed":true,"item":database(move|c|store::accessible(c,&r.item,&r.actor,false)).await?}),
        ));
    }
    let token = n1_access_token()
        .await
        .map_err(|_| error(502, "N1 is unavailable. Your upload remains resumable."))?;
    let storage = n1_storage(&token).map_err(|_| error(502, "N1 is unavailable."))?;
    if storage.is_finalized(&r.key(), &session.version).await {
        return Ok(Json(
            json!({"operation_uid":r.operation,"completed":false,"ready_to_finalize":true,"part_size":session.part_size,"workers":multipart_policy(r.size as u64).1,"uploaded_parts":[]}),
        ));
    }
    let result = storage
        .durable_request(
            reqwest::Method::GET,
            &r.key(),
            &session.version,
            "",
            session.coordinator.as_deref(),
            None,
            None,
        )
        .await
        .map_err(|_| {
            error(
                502,
                "Cannot reconcile stored parts. Retry shortly; MX has kept the upload identity.",
            )
        })?;
    let indices: Vec<u64> = result["uploaded_parts"]
        .as_array()
        .map(|parts| parts.iter().filter_map(|v| v.as_u64()).collect())
        .unwrap_or_default();
    Ok(Json(
        json!({"operation_uid":r.operation,"completed":false,"part_size":session.part_size,"workers":multipart_policy(r.size as u64).1,"uploaded_parts":indices}),
    ))
}
pub(super) async fn part(
    claims: Claims,
    Path((operation, index)): Path<(String, u64)>,
    request: axum::extract::Request,
) -> Result<Json<Value>, Error> {
    let _permit = TRANSFERS
        .try_acquire()
        .map_err(|_| error(429, "Other parts are busy. Retry this part shortly."))?;
    // Admit before reading the body, not after Bytes has allocated it.
    let bytes = axum::body::to_bytes(request.into_body(), MAX_PART_BYTES)
        .await
        .map_err(|_| error(413, "Upload part exceeds 50 MiB or was interrupted. Resume the file using the negotiated part size."))?;
    let actor = claims.uid.clone();
    let op = operation.clone();
    let hashing = bytes.clone();
    let checksum = tokio::task::spawn_blocking(move || store::digest(&hashing))
        .await
        .map_err(|_| error(500, "Could not validate this upload part. Retry it."))?;
    let size = bytes.len() as i64;
    let session=database(move|c|{
      let tx=rusqlite::Transaction::new_unchecked(c,rusqlite::TransactionBehavior::Immediate)?;let s=load(&tx,&actor,&op)?;let r=&s.reservation;
      if r.completed {return Err(store::problem(409,"This file is already uploaded."));}
      let parts=(r.size as u64).div_ceil(s.part_size as u64).max(1);
      if index>=parts || size!=(r.size-index as i64*s.part_size).min(s.part_size) {return Err(store::problem(400,"Incorrect part index or length."));}
      let prior:Option<String>=tx.query_row("SELECT sha256 FROM mx_drive_parts WHERE actor_uid=?1 AND operation_uid=?2 AND part_index=?3",params![actor,op,index as i64],|row|row.get(0)).optional()?;
      if prior.is_some_and(|value|value!=checksum) {return Err(store::problem(409,"A different file was supplied for this upload part. Choose the original file."));}
      tx.execute("INSERT OR IGNORE INTO mx_drive_parts(actor_uid,operation_uid,part_index,sha256) VALUES(?1,?2,?3,?4)",params![actor,op,index as i64,checksum])?;tx.commit()?;Ok(s)
    }).await?;
    let token = n1_access_token()
        .await
        .map_err(|_| error(502, "N1 is unavailable. Retry this part."))?;
    n1_storage(&token)
        .map_err(|_| error(502, "N1 is unavailable."))?
        .durable_request(
            reqwest::Method::PUT,
            &session.reservation.key(),
            &session.version,
            &format!("/{index}"),
            session.coordinator.as_deref(),
            Some(bytes),
            None,
        )
        .await
        .map_err(|_| {
            error(
                502,
                "This part was not acknowledged. Retry the identical part.",
            )
        })?;
    Ok(Json(json!({"stored":true,"index":index})))
}
pub(super) async fn finish(
    claims: Claims,
    Path(operation): Path<String>,
) -> Result<Json<store::Item>, Error> {
    let _permit = TRANSFERS
        .try_acquire()
        .map_err(|_| error(429, "Other transfers are busy. Retry shortly."))?;
    let s = database(move |c| load(c, &claims.uid, &operation)).await?;
    let r = s.reservation;
    if r.completed {
        return Ok(Json(
            database(move |c| store::accessible(c, &r.item, &r.actor, false)).await?,
        ));
    }
    let token = n1_access_token()
        .await
        .map_err(|_| error(502, "N1 is unavailable. Retry finalization."))?;
    let storage = n1_storage(&token).map_err(|_| error(502, "N1 is unavailable."))?;
    if !storage.is_finalized(&r.key(), &s.version).await {
        let result=storage.durable_request(reqwest::Method::POST,&r.key(),&s.version,&format!("/{}/finalize",s.session),s.coordinator.as_deref(),None,Some(json!({"content_type":r.mime,"total_size_bytes":r.size,"expected_parts":(r.size as u64).div_ceil(s.part_size as u64).max(1),"part_size_bytes":s.part_size}))).await;
        if result.is_err() && !storage.is_finalized(&r.key(), &s.version).await {
            return Err(error(
                502,
                "N1 has not acknowledged finalization. Resume this upload; do not start a new file.",
            ));
        }
    }
    // N1 validates/publishes its parts. Confirm exact immutable version and size via
    // metadata, NOT a second download of the entire file before it appears in Drive.
    if !storage
        .verify_finalized_size(&r.key(), &s.version, r.size as u64)
        .await
    {
        return Err(error(
            502,
            "N1 has not confirmed this file's version and size. Retry finalization.",
        ));
    }
    let mut copy = r.clone();
    // No whole-file checksum is computed for multipart. Never record an unverified
    // legacy client declaration as a verified content digest. Per-part receipts remain.
    copy.checksum.clear();
    match database(move |c| commit(c, &copy)).await {
        Ok(item) => Ok(Json(item)),
        Err(failure) => {
            super::uploads::release(r, failure.0.is_client_error()).await;
            Err(failure)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn console_part_sizes_and_workers_at_every_boundary() {
        const MIB: u64 = 1024 * 1024;
        for (size, part, workers) in [
            (0, 1, 1),
            (1, 1, 1),
            (16 * MIB - 1, 16 * MIB - 1, 1),
            (16 * MIB, 4 * MIB, 2),
            (100 * MIB + 1, 4 * MIB, 2),
            (256 * MIB - 1, 4 * MIB, 2),
            (256 * MIB, 8 * MIB, 3),
            (1024 * MIB, 8 * MIB, 3),
            (1024 * MIB + 1, 50 * MIB, 4),
            (10_000 * MIB, 50 * MIB, 4),
            (i64::MAX as u64, 50 * MIB, 4),
        ] {
            assert_eq!(multipart_policy(size), (part, workers));
            assert!(part <= MAX_PART_BYTES as u64);
        }
    }
}
pub(super) async fn cancel(
    claims: Claims,
    Path(operation): Path<String>,
) -> Result<Json<Value>, Error> {
    // Cancellation must still work if a destination was trashed or access was revoked.
    let r=database(move|c| {
        let result=c.query_row("SELECT item_uid,version_uid,parent_uid,name,mime_type,size,sha256,base_revision,attempt_uid,completed,owner_uid FROM mx_drive_uploads WHERE actor_uid=?1 AND operation_uid=?2",params![claims.uid,operation],|row|Ok(Reservation {actor:claims.uid.clone(),operation:operation.clone(),item:row.get(0)?,version:row.get(1)?,parent:row.get(2)?,name:row.get(3)?,mime:row.get(4)?,size:row.get(5)?,checksum:row.get(6)?,base:row.get(7)?,attempt:row.get(8)?,completed:row.get(9)?,retry:true,owner:row.get(10)?,quota:CONFIG.drive.quota_mb as i64*1024*1024})).optional()?;Ok(result)
    }).await?;
    if let Some(r) = r {
        if !r.completed {
            super::uploads::release(r, true).await;
        }
    }
    Ok(Json(json!({"ok":true,"storage_cleanup":"queued"})))
}
