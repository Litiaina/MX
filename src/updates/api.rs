use super::{
    UpdateConfig, configuration, in_maintenance, package,
    store::{self, Policy},
};
use crate::{
    config::load_config::CONFIG, db::connector::with_sql_connection, middleware::auth::Claims,
};
use axum::{
    Json,
    extract::{Path, Query, Request},
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
static INFLIGHT: AtomicUsize = AtomicUsize::new(0);
struct Inflight;
impl Drop for Inflight {
    fn drop(&mut self) {
        INFLIGHT.fetch_sub(1, Ordering::SeqCst);
    }
}
pub async fn maintenance_gate(request: Request, next: Next) -> Response {
    let path = request.uri().path();
    if path == "/mx/v1/system/update-health"
        || (path == "/mx/v1/admin/updates" && request.method() == Method::GET)
    {
        return next.run(request).await;
    }
    INFLIGHT.fetch_add(1, Ordering::SeqCst);
    let _request = Inflight;
    if path.starts_with("/mx/") && in_maintenance() {
        return (StatusCode::SERVICE_UNAVAILABLE,[("Retry-After","5")],Json(json!({"response":"MX is applying an update. Your local entries are retained; wait for the server to return before retrying.","maintenance":true}))).into_response();
    }
    next.run(request).await
}
fn managed(config: &UpdateConfig) -> bool {
    let current = fs::canonicalize(config.root.join("current/litiaina-mx"));
    let running = std::env::current_exe().and_then(fs::canonicalize);
    let frontend = fs::canonicalize(&CONFIG.static_hosting.static_web_path);
    let expected = fs::canonicalize(config.root.join("current/frontend/dist"));
    matches!((current,running,frontend,expected),(Ok(a),Ok(b),Ok(c),Ok(d)) if a==b && c==d)
}
pub async fn health() -> Response {
    let config = configuration().ok().flatten();
    let maintenance = config
        .and_then(|c| store::read::<serde_json::Value>(&c.root.join("maintenance.json")).ok());
    let announcement = config
        .and_then(|c| store::read::<serde_json::Value>(&c.root.join("announcement.json")).ok());
    let frontend = PathBuf::from(&CONFIG.static_hosting.static_web_path).join("mx-release.json");
    let marker = store::read::<package::FrontendIdentity>(&frontend).ok();
    let ready =
        with_sql_connection(|c| c.query_row("SELECT 1", [], |r| r.get::<_, i64>(0))).is_ok();
    let identity = super::health::Identity {
        version: env!("CARGO_PKG_VERSION").into(),
        pid: std::process::id(),
        database_id: super::health::database_id(std::path::Path::new(&CONFIG.database.db_path))
            .unwrap_or_default(),
        ready,
        managed: config.is_some_and(managed),
        frontend_version: marker.map(|m| m.version),
        update_id: maintenance
            .as_ref()
            .and_then(|m| m["uid"].as_str().map(String::from)),
    };
    let proof = maintenance
        .as_ref()
        .and_then(|m| m["health_secret"].as_str())
        .and_then(|secret| super::health::proof(secret, &identity).ok());
    let mut value = serde_json::to_value(&identity).unwrap();
    value["proof"] = json!(proof);
    value["configured"] = json!(config.is_some());
    value["maintenance"] = json!(in_maintenance());
    value["scheduled_at"] = json!(announcement.and_then(|a| a["scheduled_at"].as_i64()));
    value["inflight"] = json!(INFLIGHT.load(Ordering::SeqCst));
    value["active_call_participants"] = json!(crate::api::calls::active_participant_count());
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        [("Cache-Control", "no-store")],
        Json(value),
    )
        .into_response()
}
fn failure(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({"response":message.into()}))).into_response()
}
fn config_for(claims: &Claims) -> Result<&'static UpdateConfig, Response> {
    if !claims.can_manage_accounts() {
        return Err(failure(
            StatusCode::FORBIDDEN,
            "Only an MX administrator may manage server updates.",
        ));
    }
    configuration().map_err(|e|failure(StatusCode::SERVICE_UNAVAILABLE,e))?.ok_or_else(||failure(StatusCode::SERVICE_UNAVAILABLE,"Updates are not configured. The deployment operator must provision mx-updates.json, trusted signing keys and the independent worker."))
}
pub async fn status(claims: Claims) -> Response {
    if !claims.can_manage_accounts() {
        return failure(
            StatusCode::FORBIDDEN,
            "Only an MX administrator may inspect update management.",
        );
    }
    let config=match configuration(){Ok(Some(c))=>c,Ok(None)=>return Json(json!({"configured":false,"current_version":env!("CARGO_PKG_VERSION"),"message":"Updates are not configured. Ask your deployment operator to provision mx-updates.json and the independent worker. MX continues to work without internet access."})).into_response(),Err(e)=>return failure(StatusCode::SERVICE_UNAVAILABLE,e)};
    let state = match store::state(config) {
        Ok(state) => state,
        Err(e) => return failure(StatusCode::SERVICE_UNAVAILABLE, e),
    };
    let policy = match store::policy(config) {
        Ok(policy) => policy,
        Err(e) => return failure(StatusCode::SERVICE_UNAVAILABLE, e),
    };
    let worker = store::read::<serde_json::Value>(&config.root.join("worker.json")).ok();
    Json(json!({"configured":true,"current_version":env!("CARGO_PKG_VERSION"),"channel":config.channel,"repository":config.repository,"managed":managed(config),"policy":policy,"notice_seconds":config.notice_seconds,"busy":config.root.join("request.json").exists()||in_maintenance(),"maintenance":in_maintenance(),"last_checked_at":state.last_checked_at,"worker_last_started_at":worker.and_then(|w|w["last_started_at"].as_i64()),"available":state.available.map(|o|json!({"version":o.version,"tag":o.tag,"notes":o.notes})),"staged":state.staged.map(|s|json!({"version":s.version})),"operation":state.operation,"history":state.history})).into_response()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionRequest {
    pub operation_uid: String,
    pub version: Option<String>,
}
pub async fn action(
    claims: Claims,
    Path(action): Path<String>,
    Json(body): Json<ActionRequest>,
) -> Response {
    let config = match config_for(&claims) {
        Ok(c) => c,
        Err(e) => return e,
    };
    if !matches!(action.as_str(), "check" | "stage" | "install") {
        return failure(StatusCode::NOT_FOUND, "Unknown update action.");
    }
    if action == "install" && !managed(config) {
        return failure(
            StatusCode::CONFLICT,
            "This server is not using managed release slots yet. Follow the deployment setup before activation.",
        );
    }
    let config = config.clone();
    let request = store::Request {
        uid: body.operation_uid,
        action,
        version: body.version,
        actor_uid: claims.uid,
        created_at: store::now(),
    };
    let uid = request.uid.clone();
    match tokio::task::spawn_blocking(move||{store::initialize(&config)?;let _submit=store::Lock::acquire(&config.root.join("submit.lock"),false)?;store::queue(&config,request)}).await {
        Ok(Ok(()))=>(StatusCode::ACCEPTED,Json(json!({"response":"Update job queued. Progress will continue if you leave this page.","operation_uid":uid}))).into_response(),
        Ok(Err(e))=>failure(StatusCode::CONFLICT,e),Err(e)=>failure(StatusCode::INTERNAL_SERVER_ERROR,e.to_string()),
    }
}
pub async fn save_policy(claims: Claims, Json(policy): Json<Policy>) -> Response {
    let config = match config_for(&claims) {
        Ok(c) => c,
        Err(e) => return e,
    };
    if let Err(e) = policy.validate() {
        return failure(StatusCode::UNPROCESSABLE_ENTITY, e);
    }
    let config = config.clone();
    match tokio::task::spawn_blocking(move||{store::initialize(&config)?;let _submit=store::Lock::acquire(&config.root.join("submit.lock"),false)?;if config.root.join("request.json").exists()||config.root.join("maintenance.json").exists(){return Err("Wait for the active update job before changing installation policy.".into());}store::atomic(&config.root.join("policy.json"),&policy)}).await {
        Ok(Ok(()))=>Json(json!({"response":"Update policy saved. Trusted keys and upstream remain controlled by the deployment operator."})).into_response(),Ok(Err(e))=>failure(StatusCode::CONFLICT,e),Err(e)=>failure(StatusCode::INTERNAL_SERVER_ERROR,e.to_string()),
    }
}
#[derive(Deserialize)]
pub struct UploadQuery {
    pub operation_uid: String,
}
pub async fn upload(
    claims: Claims,
    Query(query): Query<UploadQuery>,
    request: Request,
) -> Response {
    let config = match config_for(&claims) {
        Ok(c) => c,
        Err(e) => return e,
    };
    if uuid::Uuid::parse_str(&query.operation_uid).is_err() {
        return failure(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Invalid upload operation ID.",
        );
    }
    if let Err(e) = store::initialize(config) {
        return failure(StatusCode::SERVICE_UNAVAILABLE, e);
    }
    let _submit = match store::Lock::acquire(&config.root.join("submit.lock"), false) {
        Ok(lock) => lock,
        Err(e) => return failure(StatusCode::CONFLICT, e),
    };
    let _worker = match store::Lock::acquire(&config.root.join("worker.lock"), false) {
        Ok(lock) => lock,
        Err(e) => return failure(StatusCode::CONFLICT, e),
    };
    if config.root.join("request.json").exists() || in_maintenance() {
        return failure(
            StatusCode::CONFLICT,
            "Wait for the current update job before uploading another package.",
        );
    }
    let length = request
        .headers()
        .get("content-length")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.parse::<u64>().ok());
    if length.is_some_and(|n| n > config.max_package_bytes) {
        return failure(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Package exceeds the configured update safety budget.",
        );
    }
    if let Err(e) = package::check_space(config, length.unwrap_or(64 * 1024 * 1024)) {
        return failure(StatusCode::INSUFFICIENT_STORAGE, e);
    }
    let path = config
        .root
        .join("incoming")
        .join(format!("{}.mxupdate", query.operation_uid));
    let result: store::Result<()> = async {
        use futures::StreamExt;
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .await
            .map_err(|e| {
                format!(
                    "Cannot create a new package upload: {e}. Check update status before retrying."
                )
            })?;
        let mut stream = request.into_body().into_data_stream();
        let mut bytes = 0u64;
        while let Some(chunk) =
            tokio::time::timeout(std::time::Duration::from_secs(60), stream.next())
                .await
                .map_err(|_| "Package upload stopped making progress.")?
        {
            let chunk = chunk.map_err(|e| e.to_string())?;
            bytes = bytes
                .checked_add(chunk.len() as u64)
                .ok_or("Upload length overflow.")?;
            if bytes > config.max_package_bytes {
                return Err("Package exceeds the update safety budget.".into());
            }
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        }
        if bytes == 0 || length.is_some_and(|n| n != bytes) {
            return Err("Package upload was empty or incomplete.".into());
        }
        file.sync_all().await.map_err(|e| e.to_string())?;
        drop(file);
        // Publish the request only after the independent worker lock is free.
        drop(_worker);
        store::queue(
            config,
            store::Request {
                uid: query.operation_uid.clone(),
                action: "import".into(),
                version: None,
                actor_uid: claims.uid,
                created_at: store::now(),
            },
        )
    }
    .await;
    match result {Ok(())=>(StatusCode::ACCEPTED,Json(json!({"response":"Package received. Signature verification and staging continue in the independent worker.","operation_uid":query.operation_uid}))).into_response(),Err(e)=>failure(StatusCode::UNPROCESSABLE_ENTITY,e)}
}
