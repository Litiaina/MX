//! MX owns Drive metadata; N1 owns immutable bytes and TTL-based reclamation.
mod batch;
mod gc;
mod listing;
mod quotas;
mod resumable;
mod sharing;
mod store;
mod transfers;
mod uploads;

use crate::{
    api::mx::{
        handler::{generate_office_pdf_preview, n1_download, n1_stream, office_preview_supported},
        model::FileAttachment,
    },
    config::load_config::CONFIG,
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::{Claims, auth},
};
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Path, Query},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

pub(crate) use gc::run as run_cleanup;
pub(crate) use store::ensure as ensure_drive_schema;

#[derive(Debug)]
pub(super) struct Error(pub StatusCode, pub String);
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"response":self.1}))).into_response()
    }
}
pub(super) fn error(code: u16, message: &str) -> Error {
    Error(
        StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        message.into(),
    )
}
pub(super) async fn database<T: Send + 'static>(
    operation: impl FnOnce(&Connection) -> rusqlite::Result<T> + Send + 'static,
) -> Result<T, Error> {
    tokio::task::spawn_blocking(move || with_sql_connection(operation))
        .await
        .map_err(|_| error(500, "Drive database task failed."))?
        .map_err(|failure| {
            if let SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(message)) =
                &failure
            {
                if let Some(message) = message.strip_prefix("DRIVE:") {
                    if let Some((code, message)) = message.split_once(':') {
                        return error(code.parse().unwrap_or(500), message);
                    }
                }
            }
            tracing::error!(error=%failure,"Drive database operation failed");
            error(500, "Drive could not complete this operation. Try again.")
        })
}

pub fn routes() -> Router {
    Router::new()
        .route("/mx/v1/drive", get(list))
        .route("/mx/v1/drive/people", get(people))
        .route("/mx/v1/drive/spaces", get(sharing::spaces))
        .route("/mx/v1/drive/folders", post(create_folder))
        .route("/mx/v1/drive/transfers", post(transfers::transfer))
        .route("/mx/v1/drive/admin/quotas/{uid}", get(quotas::get).put(quotas::set))
        .route(
            "/mx/v1/drive/upload",
            post(|| async { error(410, "Use /mx/v1/drive/uploads and its resumable part endpoints. Whole-file buffered uploads are retired.") }),
        )
        .route("/mx/v1/drive/uploads", post(resumable::init))
        .route(
            "/mx/v1/drive/batch",
            post(batch::upload).layer(DefaultBodyLimit::max(33 * 1024 * 1024)),
        )
        .route(
            "/mx/v1/drive/uploads/{operation}",
            get(resumable::status).delete(resumable::cancel),
        )
        .route(
            "/mx/v1/drive/uploads/{operation}/parts/{index}",
            axum::routing::put(resumable::part).layer(DefaultBodyLimit::max(resumable::MAX_PART_BYTES)),
        )
        .route(
            "/mx/v1/drive/uploads/{operation}/finish",
            post(resumable::finish),
        )
        .route("/mx/v1/drive/trash/empty", post(empty_trash))
        .route(
            "/mx/v1/drive/items/{uid}",
            get(item).patch(change).delete(purge),
        )
        .route("/mx/v1/drive/items/{uid}/star", post(star))
        .route("/mx/v1/drive/items/{uid}/sharing", get(sharing).post(grant))
        .route("/mx/v1/drive/items/{uid}/neighbors", get(neighbors))
        .route("/mx/v1/drive/items/{uid}/links", post(create_link))
        .route("/mx/v1/drive/items/{uid}/links/{link}", delete(revoke_link))
        .route("/mx/v1/drive/items/{uid}/history", get(history))
        .route("/mx/v1/drive/items/{uid}/restore-version", post(transfers::restore))
        .route("/mx/v1/drive/items/{uid}/ticket", post(ticket))
        .route("/mx/v1/drive/items/{uid}/download", get(download))
        .layer(axum::middleware::from_fn(auth))
}
pub fn public_routes() -> Router {
    Router::new()
        .route("/mx/v1/drive/media/{ticket}", get(media))
        .route("/mx/v1/drive/guest/{token}", get(guest))
        .route("/mx/v1/drive/guest/{token}/neighbors", get(guest_neighbors))
        .route("/mx/v1/drive/guest/{token}/media/{uid}", get(guest_media))
}

async fn list(claims: Claims, Query(query): Query<listing::Listing>) -> Result<Json<Value>, Error> {
    let mut result = database(move |c| {
        let mut result = listing::list(c, &claims.uid, &query)?;
        let storage = quotas::snapshot(c, &claims.uid)?;
        result["used_bytes"] = storage["used_bytes"].clone();
        result["quota_bytes"] = storage["quota_bytes"].clone();
        result["reserved_bytes"] = storage["reserved_bytes"].clone();
        result["quota_assigned"] = storage["assigned"].clone();
        Ok(result)
    })
    .await?;
    // Explicitly unlimited per file; account quota is enforced separately.
    result["max_file_size_bytes"] = Value::Null;
    result["public_links"] = json!(CONFIG.drive.public_links);
    Ok(Json(result))
}
async fn neighbors(
    claims: Claims,
    Path(uid): Path<String>,
    Query(query): Query<listing::Listing>,
) -> Result<Json<Value>, Error> {
    Ok(Json(
        database(move |c| listing::neighbors(c, &claims.uid, &uid, &query)).await?,
    ))
}
#[derive(Deserialize)]
struct Folder {
    operation_uid: String,
    name: String,
    parent_uid: Option<String>,
}
async fn create_folder(
    claims: Claims,
    Json(value): Json<Folder>,
) -> Result<Json<store::Item>, Error> {
    Ok(Json(
        database(move |c| {
            store::folder(
                c,
                &claims.uid,
                &value.operation_uid,
                &value.name,
                value.parent_uid.as_deref(),
            )
        })
        .await?,
    ))
}
async fn item(claims: Claims, Path(uid): Path<String>) -> Result<Json<store::Item>, Error> {
    Ok(Json(
        database(move |c| store::accessible(c, &uid, &claims.uid, false)).await?,
    ))
}
#[derive(Deserialize)]
struct Change {
    base_revision: i64,
    action: String,
    name: Option<String>,
    parent_uid: Option<String>,
}
async fn change(
    claims: Claims,
    Path(uid): Path<String>,
    Json(value): Json<Change>,
) -> Result<Json<store::Item>, Error> {
    Ok(Json(
        database(move |c| {
            store::mutate(
                c,
                &claims.uid,
                &uid,
                value.base_revision,
                &value.action,
                value.name.as_deref(),
                value.parent_uid.as_deref(),
            )
        })
        .await?,
    ))
}
#[derive(Deserialize)]
struct Star {
    starred: bool,
}
async fn star(
    claims: Claims,
    Path(uid): Path<String>,
    Json(value): Json<Star>,
) -> Result<Json<Value>, Error> {
    database(move |c| {
        let tx = rusqlite::Transaction::new_unchecked(c, rusqlite::TransactionBehavior::Immediate)?;
        store::accessible(&tx, &uid, &claims.uid, false)?;
        if value.starred {
            tx.execute(
                "INSERT OR IGNORE INTO mx_drive_stars(item_uid,user_uid) VALUES(?1,?2)",
                params![uid, claims.uid],
            )?;
        } else {
            tx.execute(
                "DELETE FROM mx_drive_stars WHERE item_uid=?1 AND user_uid=?2",
                params![uid, claims.uid],
            )?;
        }
        tx.commit()
    })
    .await?;
    Ok(Json(json!({"ok":true})))
}
async fn sharing(claims: Claims, Path(uid): Path<String>) -> Result<Json<Value>, Error> {
    Ok(Json(
        database(move |c| store::shared(c, &claims.uid, &uid)).await?,
    ))
}
#[derive(Deserialize)]
struct Grant {
    user_uid: Option<String>,
    space_uid: Option<String>,
    role: Option<String>,
}
async fn grant(
    claims: Claims,
    Path(uid): Path<String>,
    Json(value): Json<Grant>,
) -> Result<Json<Value>, Error> {
    database(
        move |c| match (value.user_uid.as_deref(), value.space_uid.as_deref()) {
            (Some(user), None) => store::grant(c, &claims.uid, &uid, user, value.role.as_deref()),
            (None, Some(space)) => {
                sharing::grant(c, &claims.uid, &uid, space, value.role.as_deref())
            }
            _ => Err(store::problem(
                400,
                "Choose exactly one MX account or collaboration space.",
            )),
        },
    )
    .await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
struct Link {
    expires_at: Option<i64>,
}
async fn create_link(
    claims: Claims,
    Path(uid): Path<String>,
    Json(value): Json<Link>,
) -> Result<Json<Value>, Error> {
    if !CONFIG.drive.public_links {
        return Err(error(
            403,
            "Public links are disabled by your administrator.",
        ));
    }
    Ok(Json(
        database(move |c| store::link(c, &claims.uid, &uid, value.expires_at)).await?,
    ))
}
async fn revoke_link(
    claims: Claims,
    Path((uid, link)): Path<(String, String)>,
) -> Result<Json<Value>, Error> {
    database(move |c| {
        let tx = rusqlite::Transaction::new_unchecked(c, rusqlite::TransactionBehavior::Immediate)?;
        store::owner(&tx, &uid, &claims.uid, false)?;
        tx.execute(
            "UPDATE mx_drive_links SET revoked_at=?1 WHERE uid=?2 AND item_uid=?3",
            params![store::now(), link, uid],
        )?;
        store::activity(&tx, &uid, &claims.uid, "public_link.revoked")?;
        tx.commit()
    })
    .await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
struct Search {
    #[serde(default)]
    q: String,
}
async fn people(_claims: Claims, Query(query): Query<Search>) -> Result<Json<Value>, Error> {
    // Authenticated account directory only; no email directory on guest routes.
    Ok(Json(database(move|c|{
      let mut s=c.prepare("SELECT uid,name,email FROM users WHERE INSTR(LOWER(name),LOWER(?1))>0 OR INSTR(LOWER(email),LOWER(?1))>0 ORDER BY name LIMIT 30")?;
      s.query_map([query.q],|r|Ok(json!({"uid":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"email":r.get::<_,String>(2)?})))?.collect::<rusqlite::Result<Vec<_>>>().map(|v|json!({"people":v}))
    }).await?))
}
async fn history(claims: Claims, Path(uid): Path<String>) -> Result<Json<Value>, Error> {
    Ok(Json(database(move|c| {store::accessible(c,&uid,&claims.uid,false)?;
        let mut s=c.prepare("SELECT uid,file_name,mime_type,size,created_at FROM mx_drive_versions WHERE item_uid=?1 ORDER BY created_at DESC,uid LIMIT 100")?;
        let versions=s.query_map([&uid],|r|Ok(json!({"uid":r.get::<_,String>(0)?,"file_name":r.get::<_,String>(1)?,"mime_type":r.get::<_,String>(2)?,"size":r.get::<_,i64>(3)?,"created_at":r.get::<_,i64>(4)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut s=c.prepare("SELECT a.action,a.created_at,COALESCE(u.name,'Deleted account') FROM mx_drive_activity a LEFT JOIN users u ON u.uid=a.actor_uid WHERE a.item_uid=?1 ORDER BY a.created_at DESC LIMIT 100")?;
        let activity=s.query_map([uid],|r|Ok(json!({"action":r.get::<_,String>(0)?,"created_at":r.get::<_,i64>(1)?,"actor":r.get::<_,String>(2)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(json!({"versions":versions,"activity":activity}))
    }).await?))
}
async fn purge(claims: Claims, Path(uid): Path<String>) -> Result<Json<Value>, Error> {
    purge_for(claims.uid, Some(uid)).await
}
async fn empty_trash(claims: Claims) -> Result<Json<Value>, Error> {
    purge_for(claims.uid, None).await
}
async fn purge_for(actor: String, uid: Option<String>) -> Result<Json<Value>, Error> {
    let removed = database(move |c| store::purge(c, &actor, uid.as_deref())).await?;
    gc::wake();
    Ok(Json(
        json!({"removed":removed,"storage_cleanup":"queued","response":"Removed from MX. N1 soft deletion is queued; physical reclamation follows N1 retention."}),
    ))
}

#[derive(Deserialize, Default)]
struct Content {
    version_uid: Option<String>,
    revision: Option<i64>,
    #[serde(default)]
    download: bool,
    #[serde(default)]
    office: bool,
}
fn version(
    c: &Connection,
    mut item: store::Item,
    uid: Option<&str>,
) -> rusqlite::Result<store::Item> {
    if let Some(uid) = uid {
        let value:Option<(String,String,i64,String)>=c.query_row("SELECT object_key,mime_type,size,file_name FROM mx_drive_versions WHERE uid=?1 AND item_uid=?2",params![uid,item.uid],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        let (key, mime, size, name) =
            value.ok_or_else(|| store::problem(404, "File version not found."))?;
        item.object_key = Some(key);
        item.mime_type = mime;
        item.size = size;
        item.original_file_name = name;
    }
    Ok(item)
}
#[derive(Deserialize, Default)]
struct Ticket {
    version_uid: Option<String>,
}
async fn ticket(
    claims: Claims,
    Path(uid): Path<String>,
    Json(value): Json<Ticket>,
) -> Result<Json<Value>, Error> {
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let hash = store::digest(token.as_bytes());
    let metadata=database(move|c| {
      let tx=rusqlite::Transaction::new_unchecked(c,rusqlite::TransactionBehavior::Immediate)?;
      let item=version(&tx,store::accessible(&tx,&uid,&claims.uid,false)?,value.version_uid.as_deref())?;
      if item.kind!="file" {return Err(store::problem(400,"Select a file to preview."));}
      // Several independent Drive versions can reference the same immutable bytes.
      // Pin this item's exact version, never an arbitrary content-key match.
      let version_uid:String=tx.query_row("SELECT v.uid FROM mx_drive_versions v JOIN mx_drive_items i ON i.uid=v.item_uid WHERE i.uid=?1 AND v.uid=COALESCE(?2,i.current_version_uid)",params![uid,value.version_uid],|r|r.get(0))?;
      tx.execute("DELETE FROM mx_drive_tickets WHERE expires_at<=?1",[store::now()])?;
      tx.execute("INSERT INTO mx_drive_tickets(token_hash,item_uid,actor_uid,auth_version,version_uid,expires_at) VALUES(?1,?2,?3,?4,?5,?6)",params![hash,uid,claims.uid,claims.auth_version,version_uid,store::now()+30*60*1000])?;tx.commit()?;Ok((item.mime_type,item.original_file_name))
    }).await?;
    Ok(Json(
        json!({"url":format!("/mx/v1/drive/media/{token}"),"expires_in":1800,"mime_type":metadata.0,"original_file_name":metadata.1}),
    ))
}
async fn media(
    Path(token): Path<String>,
    Query(query): Query<Content>,
    headers: HeaderMap,
) -> Result<Response, Error> {
    let item=database(move|c|{
      if token.len()!=64 {return Err(store::problem(404,"Preview expired. Reopen the file."));}
      let ticket:Option<(String,String,i64,Option<String>)>=c.query_row("SELECT item_uid,actor_uid,auth_version,version_uid FROM mx_drive_tickets WHERE token_hash=?1 AND expires_at>?2",params![store::digest(token.as_bytes()),store::now()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
      let (uid,actor,auth,version_uid)=ticket.ok_or_else(||store::problem(404,"Preview expired. Reopen the file."))?;
      let valid:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM users WHERE uid=?1 AND auth_version=?2)",params![actor,auth],|r|r.get(0))?;
      if !valid {return Err(store::problem(401,"Your account session changed. Reopen the file."));}
      version(c,store::accessible(c,&uid,&actor,false)?,version_uid.as_deref())
    }).await?;
    serve(item, query, headers).await
}
async fn download(
    claims: Claims,
    Path(uid): Path<String>,
    Query(mut query): Query<Content>,
    headers: HeaderMap,
) -> Result<Response, Error> {
    let version_uid = query.version_uid.clone();
    let item = database(move |c| {
        version(
            c,
            store::accessible(c, &uid, &claims.uid, false)?,
            version_uid.as_deref(),
        )
    })
    .await?;
    query.download = true;
    serve(item, query, headers).await
}
fn guest_item(item: &store::Item) -> Value {
    json!({"uid":item.uid,"name":item.name,"kind":item.kind,"mime_type":item.mime_type,"size":item.size,"revision":item.revision})
}
fn guest_limit() -> usize {
    50
}
#[derive(Deserialize)]
struct Guest {
    item_uid: Option<String>,
    #[serde(default)]
    offset: usize,
    #[serde(default = "guest_limit")]
    limit: usize,
}
async fn guest(Path(token): Path<String>, Query(query): Query<Guest>) -> Result<Response, Error> {
    if !CONFIG.drive.public_links {
        return Err(error(404, "Public sharing is unavailable."));
    }
    let result=database(move|c| {
      if !(1..=100).contains(&query.limit) {return Err(store::problem(400,"Choose a page size from 1 to 100."));}
      let tx=rusqlite::Transaction::new_unchecked(c,rusqlite::TransactionBehavior::Deferred)?;let c=&tx;
      let root=store::guest(c,&token,None)?; let item=store::guest(c,&token,query.item_uid.as_deref())?;
      let mut children=Vec::new();let mut total=0;let mut offset=0;
      if item.kind=="folder" {
        total=c.query_row("SELECT COUNT(*) FROM mx_drive_items WHERE parent_uid=?1 AND trashed_at IS NULL",[&item.uid],|r|r.get::<_,i64>(0))? as usize;
        offset=query.offset.min(total.saturating_sub(1)/query.limit*query.limit);
        let mut s=c.prepare("SELECT uid FROM mx_drive_items WHERE parent_uid=?1 AND trashed_at IS NULL ORDER BY kind DESC,name COLLATE NOCASE,uid LIMIT ?2 OFFSET ?3")?;
        let ids=s.query_map(params![item.uid,query.limit as i64,offset as i64],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        for uid in ids {children.push(guest_item(&store::guest(c,&token,Some(&uid))?));}
      }
      let mut breadcrumbs=Vec::new();for entry in store::ancestors(c,&item.uid)? {let stop=entry.uid==root.uid;breadcrumbs.push(guest_item(&entry));if stop {break;}} breadcrumbs.reverse();
      tx.commit()?;
      Ok(json!({"root":guest_item(&root),"item":guest_item(&item),"items":children,"breadcrumbs":breadcrumbs,"offset":offset,"limit":query.limit,"has_more":offset+children.len()<total,"total":total}))
    }).await?;
    let mut response = Json(result).into_response();
    private_headers(response.headers_mut());
    Ok(response)
}
async fn guest_neighbors(
    Path(token): Path<String>,
    Query(query): Query<Guest>,
) -> Result<Response, Error> {
    if !CONFIG.drive.public_links {
        return Err(error(404, "Public sharing is unavailable."));
    }
    let result=database(move|c| {
        let tx=rusqlite::Transaction::new_unchecked(c,rusqlite::TransactionBehavior::Deferred)?;
        let root=store::guest(&tx,&token,None)?;
        let uid=query.item_uid.ok_or_else(||store::problem(400,"Select a file."))?;
        let item=store::guest(&tx,&token,Some(&uid))?;
        if item.kind!="file" {return Err(store::problem(400,"Select a file."));}
        if root.kind=="file" {return Ok(json!({"previous":null,"next":null,"position":1,"total":1}));}
        let seek=|operator:&str,direction:&str|->rusqlite::Result<Option<Value>> {
            let sql=format!("SELECT uid FROM mx_drive_items WHERE parent_uid=?1 AND kind='file' AND trashed_at IS NULL AND (name COLLATE NOCASE {operator} ?2 OR (name COLLATE NOCASE=?2 AND uid {operator} ?3)) ORDER BY name COLLATE NOCASE {direction},uid {direction} LIMIT 1");
            let id=tx.query_row(&sql,params![item.parent_uid,item.name,item.uid],|r|r.get::<_,String>(0)).optional()?;
            id.map(|id|store::guest(&tx,&token,Some(&id)).map(|item|guest_item(&item))).transpose()
        };
        let previous=seek("<","DESC")?;let next=seek(">","ASC")?;
        let total:i64=tx.query_row("SELECT COUNT(*) FROM mx_drive_items WHERE parent_uid=?1 AND kind='file' AND trashed_at IS NULL",[&item.parent_uid],|r|r.get(0))?;
        let before:i64=tx.query_row("SELECT COUNT(*) FROM mx_drive_items WHERE parent_uid=?1 AND kind='file' AND trashed_at IS NULL AND (name COLLATE NOCASE<?2 OR (name COLLATE NOCASE=?2 AND uid<?3))",params![item.parent_uid,item.name,item.uid],|r|r.get(0))?;
        tx.commit()?;Ok(json!({"previous":previous,"next":next,"position":before+1,"total":total}))
    }).await?;
    let mut response = Json(result).into_response();
    private_headers(response.headers_mut());
    Ok(response)
}
async fn guest_media(
    Path((token, uid)): Path<(String, String)>,
    Query(mut query): Query<Content>,
    headers: HeaderMap,
) -> Result<Response, Error> {
    if !CONFIG.drive.public_links {
        return Err(error(404, "Public sharing is unavailable."));
    }
    // Guests only see the current shared version, never private version history.
    query.version_uid = None;
    let item = database(move |c| store::guest(c, &token, Some(&uid))).await?;
    // Never mix byte ranges from different current versions during playback.
    // Guests cannot select an old/private version; refresh the listing after a change.
    if query.revision != Some(item.revision) {
        return Err(error(
            409,
            "This shared file changed. Refresh the link before opening it again.",
        ));
    }
    serve(item, query, headers).await
}
fn private_headers(headers: &mut HeaderMap) {
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("content-security-policy",HeaderValue::from_static("sandbox; default-src 'none'; media-src 'self'; img-src 'self' data:; style-src 'unsafe-inline'"));
}
fn disposition(name: &str, download: bool) -> HeaderValue {
    let ascii: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || " ._-".contains(ch) {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let encoded: String = name
        .as_bytes()
        .iter()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"._-".contains(byte) {
                (*byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect();
    HeaderValue::from_str(&format!(
        "{}; filename=\"{}\"; filename*=UTF-8''{}",
        if download { "attachment" } else { "inline" },
        ascii,
        encoded
    ))
    .unwrap_or_else(|_| HeaderValue::from_static("attachment"))
}
async fn serve(item: store::Item, query: Content, headers: HeaderMap) -> Result<Response, Error> {
    let key = item.object_key.as_deref().ok_or_else(|| {
        error(
            400,
            "Folders cannot be downloaded as files. Open the folder first.",
        )
    })?;
    if query.office && office_preview_supported(&item.original_file_name) {
        let bytes = n1_download(key, &item.name)
            .await
            .map_err(|_| error(502, "N1 could not read this file. Try again."))?;
        let attachment = FileAttachment {
            uid: store::digest(key.as_bytes()),
            file_name: item.original_file_name.clone(),
            mime_type: item.mime_type.clone(),
            size: item.size as u64,
            object_key: key.into(),
            version_id: None,
            attachment_field_uid: None,
            attachment_field_label: None,
            attachment_field_storage_name: None,
        };
        let preview =
            tokio::task::spawn_blocking(move || generate_office_pdf_preview(attachment, bytes))
                .await
                .map_err(|_| error(500, "Document preview task failed."))?
                .map_err(|message| error(422, &message))?;
        let mut response = Body::from(preview).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/pdf"),
        );
        private_headers(response.headers_mut());
        return Ok(response);
    }
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let upstream = n1_stream(key, range)
        .await
        .map_err(|_| error(502, "N1 could not stream this file. Try again."))?;
    let status = upstream.status();
    if status != StatusCode::OK
        && status != StatusCode::PARTIAL_CONTENT
        && status != StatusCode::RANGE_NOT_SATISFIABLE
    {
        return Err(error(502, "Storage did not return this file. Try again."));
    }
    let mut response = Response::builder().status(status);
    for key in [
        header::ACCEPT_RANGES,
        header::CONTENT_RANGE,
        header::CONTENT_LENGTH,
        header::ETAG,
        header::LAST_MODIFIED,
    ] {
        if let Some(value) = upstream.headers().get(&key) {
            response = response.header(key, value);
        }
    }
    let mime = HeaderValue::from_str(&item.mime_type)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    // Untrusted active documents are downloads, not same-origin executable pages.
    let active = item.mime_type == "text/html"
        || item.mime_type == "application/xhtml+xml"
        || item.mime_type == "image/svg+xml";
    let mut response = response
        .header(header::CONTENT_TYPE, mime)
        .header(
            header::CONTENT_DISPOSITION,
            disposition(&item.name, query.download || active),
        )
        .body(Body::from_stream(upstream.bytes_stream()))
        .map_err(|_| error(500, "Unable to create file stream."))?;
    private_headers(response.headers_mut());
    Ok(response)
}
