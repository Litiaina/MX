//! Small files publish independently. Never replay N1's one-shot batch blindly.
use super::{
    Error, database, error, store,
    uploads::{Upload, commit, release, reserve},
};
use crate::{
    api::mx::handler::{n1_access_token, n1_download, n1_storage},
    config::load_config::CONFIG,
    middleware::auth::Claims,
};
use axum::{Json, extract::Multipart};
use serde_json::{Value, json};
use std::sync::LazyLock;
use tokio::sync::Semaphore;
static BATCHES: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(2));
pub(super) async fn upload(claims: Claims, mut multipart: Multipart) -> Result<Json<Value>, Error> {
    let _permit = BATCHES
        .try_acquire()
        .map_err(|_| error(429, "Other batches are busy. Retry shortly."))?;
    let field = multipart
        .next_field()
        .await
        .map_err(|_| error(400, "Invalid batch."))?
        .ok_or_else(|| error(400, "Batch metadata is required."))?;
    if field.name() != Some("metadata") {
        return Err(error(400, "Send batch metadata first."));
    }
    let metadata = field
        .text()
        .await
        .map_err(|_| error(400, "Invalid metadata."))?;
    if metadata.len() > 32 * 8192 {
        return Err(error(400, "Batch metadata is too large."));
    }
    let requests: Vec<Upload> =
        serde_json::from_str(&metadata).map_err(|_| error(400, "Invalid batch metadata."))?;
    if requests.is_empty() || requests.len() > 32 {
        return Err(error(400, "A small-file batch contains 1–32 files."));
    }
    let mut files = Vec::new();
    for request in requests {
        let mut field = multipart
            .next_field()
            .await
            .map_err(|_| error(400, "Batch transfer interrupted."))?
            .ok_or_else(|| error(400, "A batch file is missing."))?;
        if field.name() != Some("file") {
            return Err(error(400, "Unexpected batch field."));
        }
        let name = field
            .file_name()
            .ok_or_else(|| error(400, "Filename is required."))?
            .to_owned();
        let mime = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_owned();
        let mut bytes = Vec::new();
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|_| error(400, "Batch transfer interrupted."))?
        {
            if bytes.len() + chunk.len() > 1024 * 1024 {
                return Err(error(413, "Files over 1 MiB must use resumable multipart."));
            }
            bytes.extend_from_slice(&chunk);
        }
        files.push((request, name, mime, bytes));
    }
    if multipart
        .next_field()
        .await
        .map_err(|_| error(400, "Invalid batch."))?
        .is_some()
    {
        return Err(error(400, "Unexpected extra batch file."));
    }
    let mut results = Vec::new();
    let mut pending = Vec::new();
    let mut objects = Vec::new();
    for (request, name, mime, bytes) in files {
        let actor = claims.uid.clone();
        let op = request.operation_uid.clone();
        let checksum = store::digest(&bytes);
        let size = bytes.len() as i64;
        let sum = checksum.clone();
        let operation = request.clone();
        match database(move |c| {
            reserve(
                c,
                &actor,
                &operation,
                &name,
                &mime,
                size,
                &sum,
                CONFIG.drive.quota_mb as i64 * 1024 * 1024,
            )
        })
        .await
        {
            Err(failure) => results
                .push(json!({"operation_uid":op,"status":failure.0.as_u16(),"error":failure.1})),
            Ok(r) if r.completed => {
                let copy = r.clone();
                let item =
                    database(move |c| store::accessible(c, &copy.item, &copy.actor, false)).await?;
                results.push(json!({"operation_uid":op,"status":200,"item":item}));
            }
            Ok(r) => {
                let arrived = if r.retry {
                    n1_download(&r.key(), &r.name)
                        .await
                        .map(|prior| {
                            prior.len() == bytes.len() && store::digest(&prior) == checksum
                        })
                        .unwrap_or(false)
                } else {
                    false
                };
                if !arrived {
                    objects.push((r.key(), r.mime.clone(), bytes));
                }
                pending.push((r, arrived));
            }
        }
    }
    let response = if objects.is_empty() {
        None
    } else {
        match n1_access_token().await {
            Ok(token) => match n1_storage(&token) {
                Ok(storage) => storage.upload_batch(objects).await.ok(),
                Err(_) => None,
            },
            Err(_) => None,
        }
    };
    for (r, arrived) in pending {
        let acknowledged = response
            .as_ref()
            .and_then(|value| value["results"].as_array())
            .is_some_and(|entries| {
                entries.iter().any(|entry| {
                    entry["object_key"].as_str() == Some(r.key().as_str())
                        && entry["status"]
                            .as_u64()
                            .is_some_and(|status| (200..300).contains(&status))
                })
            });
        let stored = arrived
            || acknowledged
            || n1_download(&r.key(), &r.name)
                .await
                .map(|bytes| bytes.len() == r.size as usize && store::digest(&bytes) == r.checksum)
                .unwrap_or(false);
        if stored {
            let copy = r.clone();
            match database(move |c| commit(c, &copy)).await {
                Ok(item) => {
                    results.push(json!({"operation_uid":r.operation,"status":200,"item":item}))
                }
                Err(failure) => {
                    results.push(json!({"operation_uid":r.operation,"status":failure.0.as_u16(),"error":failure.1}));
                    release(r, failure.0.is_client_error()).await;
                }
            }
        } else {
            results.push(json!({"operation_uid":r.operation,"status":502,"error":"N1 has not acknowledged this file. Retry the same file; other successful files were saved."}));
            release(r, false).await;
        }
    }
    Ok(Json(json!({"results":results})))
}
