use super::{
    UpdateConfig,
    package::{self, SignedRelease},
    store::{self, Activation, Offer, Request, Result, Staged},
};
use chrono::Timelike;
use serde::Deserialize;
use std::{
    fs::{self, File},
    path::Path,
    time::Duration,
};

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    body: Option<String>,
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}
fn client(config: &UpdateConfig) -> Result<reqwest::Client> {
    let config = config.clone();
    reqwest::Client::builder()
        .user_agent(concat!("Litiaina-MX-Updater/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 5 || config.validate_url(attempt.url().as_str()).is_err()
            {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|e| e.to_string())
}
async fn response(config: &UpdateConfig, url: &str) -> Result<reqwest::Response> {
    config.validate_url(url)?;
    let mut request = client(config)?.get(url);
    if reqwest::Url::parse(url).is_ok_and(|u| u.host_str() == Some("api.github.com")) {
        request = request.header("Accept", "application/vnd.github+json");
        if let Ok(token) = std::env::var("MX_UPDATE_GITHUB_TOKEN") {
            request = request.bearer_auth(token);
        }
    }
    let result = request
        .send()
        .await
        .map_err(|e| format!("Release connection failed: {e}"))?;
    if !result.status().is_success() {
        return Err(format!(
            "Release server returned {}. The running MX installation is unchanged.",
            result.status()
        ));
    }
    Ok(result)
}
async fn small(config: &UpdateConfig, url: &str) -> Result<Vec<u8>> {
    let mut response = response(config, url).await?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > 1024 * 1024 {
            return Err("Release metadata response is too large.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
async fn download(config: &UpdateConfig, offer: &Offer, destination: &Path) -> Result<()> {
    let manifest = package::verify(config, &offer.signed)?;
    let signed_total = manifest.files.iter().map(|f| f.size).sum::<u64>();
    let limit = signed_total
        .saturating_add(2 * 1024 * 1024)
        .min(config.max_package_bytes);
    package::check_space(config, signed_total.saturating_add(limit))?;
    let mut response = response(config, &offer.url).await?;
    let advertised = response.content_length();
    if advertised.is_some_and(|size| size > limit) {
        return Err("Update download exceeds its signed package budget.".into());
    }
    let mut output = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .await
        .map_err(|e| e.to_string())?;
    let mut bytes = 0u64;
    let mut last = std::time::Instant::now();
    use tokio::io::AsyncWriteExt;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Download interrupted; the current release is still running: {e}"))?
    {
        bytes = bytes
            .checked_add(chunk.len() as u64)
            .ok_or("Download overflow.")?;
        if bytes > limit {
            return Err("Download exceeded its signed safety budget.".into());
        }
        output.write_all(&chunk).await.map_err(|e| e.to_string())?;
        if last.elapsed() >= Duration::from_millis(500) {
            store::progress(
                config,
                "downloading",
                "Downloading the signed release package.",
                bytes,
                advertised.unwrap_or(0),
            )?;
            last = std::time::Instant::now();
        }
    }
    if advertised.is_some_and(|size| size != bytes) {
        return Err("Update download was incomplete.".into());
    }
    output.sync_all().await.map_err(|e| e.to_string())?;
    Ok(())
}
async fn installed_version(config: &UpdateConfig) -> Result<String> {
    let binary = config.root.join("current/litiaina-mx");
    if binary.exists() {
        package::executable_version(&binary).await
    } else {
        Ok(env!("CARGO_PKG_VERSION").into())
    }
}
pub(super) async fn discover(config: &UpdateConfig) -> Result<Option<Offer>> {
    let current = installed_version(config).await?;
    let releases: Vec<Release> = serde_json::from_slice(&small(config, &config.feed()).await?)
        .map_err(|e| format!("Invalid release listing: {e}"))?;
    let mut candidates = releases
        .into_iter()
        .filter(|r| !r.draft && (config.channel == "rc" || !r.prerelease))
        .filter_map(|r| {
            semver::Version::parse(r.tag_name.strip_prefix('v').unwrap_or(&r.tag_name))
                .ok()
                .filter(|v| v.major == 1)
                .map(|v| (v, r))
        })
        .filter(|(v, _)| package::newer(&v.to_string(), &current).unwrap_or(false))
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    let metadata_name = format!("mx-{}.json", package::target());
    let package_name = format!("mx-{}.mxupdate", package::target());
    let mut failures = Vec::new();
    for (version, release) in candidates.into_iter().take(10) {
        let Some(metadata) = release.assets.iter().find(|a| a.name == metadata_name) else {
            continue;
        };
        let Some(archive) = release.assets.iter().find(|a| a.name == package_name) else {
            continue;
        };
        let result: Result<Offer> = async {
            let signed: SignedRelease =
                serde_json::from_slice(&small(config, &metadata.browser_download_url).await?)
                    .map_err(|e| e.to_string())?;
            let manifest = package::verify(config, &signed)?;
            if manifest.version != version.to_string()
                || release.prerelease != !version.pre.is_empty()
            {
                return Err("GitHub tag/channel does not match the signed release.".into());
            }
            config.validate_url(&archive.browser_download_url)?;
            if config.feed_url.is_none()
                && !archive.browser_download_url.starts_with(&format!(
                    "https://github.com/{}/releases/download/{}/",
                    config.repository, release.tag_name
                ))
            {
                return Err("Release package is outside the configured repository/tag.".into());
            }
            Ok(Offer {
                version: manifest.version,
                tag: release.tag_name.clone(),
                notes: release
                    .body
                    .clone()
                    .unwrap_or_default()
                    .chars()
                    .take(16000)
                    .collect(),
                url: archive.browser_download_url.clone(),
                signed,
            })
        }
        .await;
        match result {
            Ok(offer) => return Ok(Some(offer)),
            Err(e) => failures.push(e),
        }
    }
    if let Some(error) = failures.first() {
        return Err(format!(
            "A release was found but could not be trusted: {error}"
        ));
    }
    Ok(None)
}
async fn systemctl(config: &UpdateConfig, arguments: &[&str]) -> Result<String> {
    let mut command = tokio::process::Command::new("/usr/bin/systemctl");
    if config.scope == "user" {
        command.arg("--user");
    }
    let output = tokio::time::timeout(
        Duration::from_secs(90),
        command
            .arg("--no-ask-password")
            .args(arguments)
            .arg(&config.service)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| "systemctl timed out; maintenance is retained until recovery.")?
    .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "systemctl {} failed: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().into())
}
async fn assert_service(config: &UpdateConfig) -> Result<()> {
    let start = systemctl(config, &["show", "--value", "--property=ExecStart"]).await?;
    let expected = config
        .root
        .join("current/litiaina-mx")
        .to_string_lossy()
        .to_string();
    if !start.contains(&format!("path={expected} ;")) {
        return Err(format!(
            "Service ExecStart must use {expected}. Follow the one-time managed deployment setup."
        ));
    }
    let directory = systemctl(config, &["show", "--value", "--property=WorkingDirectory"]).await?;
    if fs::canonicalize(directory).ok() != fs::canonicalize(&config.deployment_dir).ok() {
        return Err(
            "The configured service WorkingDirectory does not match the MX deployment.".into(),
        );
    }
    let live = config.live()?;
    if fs::canonicalize(live.frontend).ok()
        != fs::canonicalize(config.root.join("current/frontend/dist")).ok()
    {
        return Err("Set static_site_path to the managed current/frontend/dist path before enabling activation.".into());
    }
    Ok(())
}
fn current_slot(config: &UpdateConfig) -> Result<String> {
    let link = fs::read_link(config.root.join("current"))
        .map_err(|_| "Managed current must be a symlink to slots/RELEASE.")?;
    let name = link
        .strip_prefix("slots")
        .ok()
        .and_then(|p| p.to_str())
        .ok_or("Current release link escapes its slots directory.")?;
    let path = store::slot(config, name)?;
    if !fs::symlink_metadata(path)
        .map_err(|e| e.to_string())?
        .is_dir()
    {
        return Err("The previous release slot is missing.".into());
    }
    Ok(name.into())
}
fn switch(config: &UpdateConfig, name: &str) -> Result<()> {
    let destination = store::slot(config, name)?;
    if !destination.is_dir() {
        return Err("Cannot activate a missing slot.".into());
    }
    #[cfg(unix)]
    {
        let temporary = config
            .root
            .join(format!("current-{}.tmp", uuid::Uuid::new_v4()));
        std::os::unix::fs::symlink(Path::new("slots").join(name), &temporary)
            .map_err(|e| e.to_string())?;
        fs::rename(temporary, config.root.join("current")).map_err(|e| e.to_string())?;
        store::sync_dir(&config.root)
    }
    #[cfg(not(unix))]
    {
        Err("Activation requires Linux/systemd.".into())
    }
}
fn integrity(path: &Path) -> Result<()> {
    let connection =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
    connection
        .busy_timeout(Duration::from_secs(30))
        .map_err(|e| e.to_string())?;
    let mut query = connection
        .prepare("PRAGMA integrity_check")
        .map_err(|e| e.to_string())?;
    let rows = query
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    if rows != ["ok"] {
        return Err(format!(
            "SQLite integrity verification failed: {}",
            rows.join("; ")
        ));
    }
    Ok(())
}
fn snapshot(config: &UpdateConfig, job: &mut Activation) -> Result<()> {
    let live = config.live()?;
    let length = fs::metadata(&live.database)
        .map_err(|e| format!("Existing SQLite database is missing: {e}"))?
        .len();
    package::check_space(config, length.saturating_mul(3))?;
    let destination = config
        .root
        .join("backups")
        .join(format!("{}.sqlite3", job.uid));
    if destination.exists() {
        return Err("Rollback snapshot already exists but its journal is incomplete; preserve it for operator inspection.".into());
    }
    let connection = rusqlite::Connection::open_with_flags(
        &live.database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .map_err(|e| e.to_string())?;
    connection
        .busy_timeout(Duration::from_secs(30))
        .map_err(|e| e.to_string())?;
    let busy: i64 = connection
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if busy != 0 {
        return Err(
            "SQLite still has another writer/reader holding its WAL. Activation is aborted.".into(),
        );
    }
    connection
        .execute_batch(&format!(
            "VACUUM INTO '{}'",
            destination.to_string_lossy().replace('\'', "''")
        ))
        .map_err(|e| format!("Final rollback snapshot failed: {e}"))?;
    drop(connection);
    integrity(&destination)?;
    File::open(&destination)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    store::sync_dir(destination.parent().ok_or("Missing backup parent.")?)?;
    job.snapshot = Some(
        destination
            .file_name()
            .ok_or("No snapshot name.")?
            .to_string_lossy()
            .into(),
    );
    job.snapshot_sha256 = Some(package::hash_file(&destination)?);
    job.phase = "backed_up".into();
    store::atomic(&config.root.join("activation.json"), job)
}
fn restore(config: &UpdateConfig, job: &Activation) -> Result<()> {
    let name = job
        .snapshot
        .as_deref()
        .ok_or("No verified rollback snapshot.")?;
    if name != format!("{}.sqlite3", job.uid) {
        return Err("Rollback snapshot path does not match its journal.".into());
    }
    let source = config.root.join("backups").join(name);
    if package::hash_file(&source)? != job.snapshot_sha256.as_deref().unwrap_or("") {
        return Err("Rollback snapshot checksum changed. Maintenance stays enabled; operator recovery is required.".into());
    }
    integrity(&source)?;
    let database = config.live()?.database;
    let recovery = config.root.join("recovery").join(&job.uid);
    fs::create_dir_all(&recovery).map_err(|e| e.to_string())?;
    // Copy a diagnostic set instead of moving across possibly different disks.
    // All MX processes are stopped before this method is called.
    for (i, suffix) in ["", "-wal", "-shm"].iter().enumerate() {
        let path = std::path::PathBuf::from(format!("{}{suffix}", database.to_string_lossy()));
        if path.exists() {
            let saved = recovery.join(format!("failed-{i}.sqlite"));
            if !saved.exists() {
                fs::copy(&path, &saved).map_err(|e| e.to_string())?;
                File::open(saved)
                    .and_then(|f| f.sync_all())
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    store::sync_dir(&recovery)?;
    let temporary =
        database.with_extension(format!("restore-{}-{}.tmp", job.uid, uuid::Uuid::new_v4()));
    fs::copy(&source, &temporary).map_err(|e| e.to_string())?;
    let permissions = fs::metadata(&database)
        .or_else(|_| fs::metadata(&source))
        .map_err(|e| e.to_string())?
        .permissions();
    fs::set_permissions(&temporary, permissions).map_err(|e| e.to_string())?;
    File::open(&temporary)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    integrity(&temporary)?;
    // Never combine a standalone restored database with candidate WAL/SHM.
    for suffix in ["-wal", "-shm"] {
        store::remove(&std::path::PathBuf::from(format!(
            "{}{suffix}",
            database.to_string_lossy()
        )))?;
    }
    fs::rename(&temporary, &database).map_err(|e| e.to_string())?;
    store::sync_dir(database.parent().ok_or("No database parent.")?)?;
    Ok(())
}
fn health_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())
}
async fn health(config: &UpdateConfig) -> Result<serde_json::Value> {
    health_client()?
        .get(config.live()?.health_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}
async fn wait_healthy(
    config: &UpdateConfig,
    version: &str,
    job: &Activation,
    managed: bool,
) -> Result<()> {
    let end = tokio::time::Instant::now() + Duration::from_secs(config.health_timeout_seconds);
    let expected_database = super::health::database_id(&config.live()?.database)?;
    let mut last_response = "No ready response was received.".to_string();
    while tokio::time::Instant::now() < end {
        if let Ok(value) = health(config).await {
            let identity = serde_json::from_value::<super::health::Identity>(value.clone()).ok();
            let expected_pid = systemctl(config, &["show", "--value", "--property=MainPID"])
                .await
                .ok()
                .and_then(|pid| pid.parse::<u32>().ok());
            let trusted = identity.as_ref().is_some_and(|identity| {
                Some(identity.pid) == expected_pid
                    && expected_pid != Some(0)
                    && identity.database_id == expected_database
                    && value["proof"].as_str().is_some_and(|proof| {
                        super::health::verify(&job.health_secret, identity, proof)
                    })
            });
            last_response = format!(
                "Last response: version {}, ready {}, managed {}, frontend {}, PID {}, authenticated {}.",
                value["version"],
                value["ready"],
                value["managed"],
                value["frontend_version"],
                value["pid"],
                trusted
            );
            if value["version"].as_str() == Some(version)
                && value["ready"] == true
                && value["update_id"].as_str() == Some(job.uid.as_str())
                && (!managed || value["frontend_version"].as_str() == Some(version))
                && value["managed"] == true
                && trusted
            {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
    Err(format!(
        "MX {version} did not pass the version, SQLite, frontend and authenticated maintenance health checks within {} seconds. {last_response}",
        config.health_timeout_seconds
    ))
}
async fn stopped(config: &UpdateConfig) -> Result<()> {
    systemctl(config, &["stop"]).await?;
    if systemctl(config, &["show", "--value", "--property=MainPID"]).await? != "0" {
        return Err("MX did not stop completely. The database will not be replaced.".into());
    }
    Ok(())
}
async fn start_previous(config: &UpdateConfig) -> Result<()> {
    // An inactive unit can be garbage-collected between stop and restart.
    // reset-failed does not load it, whereas start does. Try resetting a crash
    // limit, but let start/readiness be authoritative for recovery success.
    let reset_error = systemctl(config, &["reset-failed"]).await.err();
    systemctl(config, &["start"])
        .await
        .map(|_| ())
        .map_err(|error| match reset_error {
            Some(reset) => format!("{error} (reset-failed also reported: {reset})"),
            None => error,
        })
}
async fn finish_commit(config: &UpdateConfig, job: &Activation) -> Result<()> {
    systemctl(config, &["start"]).await?;
    wait_healthy(config, &job.version, job, true).await?;
    store::terminal(
        config,
        "completed",
        &format!(
            "MX {} is running. The verified pre-update backup and previous release are retained.",
            job.version
        ),
        None,
    )?;
    store::remove(&config.root.join("request.json"))?;
    store::remove(&config.root.join("announcement.json"))?;
    store::remove(&config.root.join("maintenance.json"))?;
    Ok(())
}
async fn rollback(config: &UpdateConfig, job: &mut Activation, reason: &str) -> Result<()> {
    // Persist intent before stopping or touching files. Repeating this path
    // after worker/power interruption can only restore the same verified backup.
    store::atomic(
        &config.root.join("maintenance.json"),
        &serde_json::json!({"uid":job.uid,"version":job.version,"started_at":job.started_at,"health_secret":job.health_secret}),
    )?;
    job.phase = "rolling_back".into();
    store::atomic(&config.root.join("activation.json"), job)?;
    store::progress(
        config,
        "rolling_back",
        "Activation failed. Restoring the previous application and verified SQLite snapshot.",
        0,
        0,
    )?;
    stopped(config).await?;
    if job.snapshot.is_some() {
        restore(config, job)?;
    }
    switch(config, &job.previous_slot)?;
    start_previous(config).await?;
    wait_healthy(config, &job.previous_version, job, false).await?;
    job.phase = "rolled_back".into();
    store::atomic(&config.root.join("activation.json"), job)?;
    store::terminal(
        config,
        "rolled_back",
        "The previous MX release and database are restored. New writes may resume.",
        Some(reason.into()),
    )?;
    store::remove(&config.root.join("request.json"))?;
    store::remove(&config.root.join("announcement.json"))?;
    store::remove(&config.root.join("maintenance.json"))?;
    Ok(())
}
pub(super) async fn install(
    config: &UpdateConfig,
    request: &Request,
    staged: &Staged,
) -> Result<()> {
    assert_service(config).await?;
    let before = health(config).await?;
    if before["database_id"].as_str()
        != Some(super::health::database_id(&config.live()?.database)?.as_str())
    {
        return Err("The running MX database path differs from mx.config. Reconcile deployment configuration and restart MX before updating.".into());
    }
    let manifest = package::verify_tree(config, staged)?;
    let current = installed_version(config).await?;
    if request.version.as_deref() != Some(&manifest.version)
        || !package::newer(&manifest.version, &current)?
    {
        return Err("Install request is stale or would downgrade/reinstall the current version. Check for updates again.".into());
    }
    if package::executable_version(&store::slot(config, &staged.slot)?.join("litiaina-mx")).await?
        != manifest.version
    {
        return Err("Candidate executable does not match its signed version.".into());
    }
    let previous = current_slot(config)?;
    if request.actor_uid == "scheduled-worker"
        && (!in_window(config)?
            || health(config)
                .await
                .map_or(true, |h| h["active_call_participants"].as_u64() != Some(0)))
    {
        store::terminal(
            config,
            "deferred",
            "Automatic installation is deferred outside its maintenance window or while a call is active.",
            None,
        )?;
        return Ok(());
    }
    if config.notice_seconds > 0 {
        let scheduled = store::now() + (config.notice_seconds * 1000) as i64;
        store::atomic(
            &config.root.join("announcement.json"),
            &serde_json::json!({"uid":request.uid,"version":manifest.version,"scheduled_at":scheduled}),
        )?;
        store::progress(
            config,
            "scheduled",
            "Update verified. Clients are being warned to save their work before the restart.",
            0,
            0,
        )?;
        tokio::time::sleep(Duration::from_secs(config.notice_seconds)).await;
        if request.actor_uid == "scheduled-worker"
            && (!in_window(config)?
                || health(config)
                    .await
                    .map_or(true, |h| h["active_call_participants"].as_u64() != Some(0)))
        {
            store::remove(&config.root.join("announcement.json"))?;
            store::terminal(
                config,
                "deferred",
                "Automatic installation deferred: the maintenance window ended or a call is active.",
                None,
            )?;
            return Ok(());
        }
    }
    let mut job = Activation {
        uid: request.uid.clone(),
        phase: "draining".into(),
        previous_slot: previous,
        candidate_slot: staged.slot.clone(),
        version: manifest.version.clone(),
        previous_version: current,
        snapshot: None,
        snapshot_sha256: None,
        started_at: store::now(),
        health_secret: format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4()),
    };
    store::atomic(&config.root.join("activation.json"), &job)?;
    store::atomic(
        &config.root.join("maintenance.json"),
        &serde_json::json!({"uid":job.uid,"version":job.version,"started_at":job.started_at,"health_secret":job.health_secret}),
    )?;
    let activation: Result<()> = async {
        store::progress(
            config,
            "draining",
            "New requests are paused. Waiting for in-flight work to finish.",
            0,
            0,
        )?;
        let end = tokio::time::Instant::now() + Duration::from_secs(config.drain_timeout_seconds);
        let mut drained = false;
        while tokio::time::Instant::now() < end {
            if health(config)
                .await
                .is_ok_and(|value| value["inflight"].as_u64() == Some(0))
            {
                drained = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        if !drained {
            return Err("Active requests did not drain. No update was installed.".into());
        }
        job.phase = "stopping".into();
        store::atomic(&config.root.join("activation.json"), &job)?;
        stopped(config).await?;
        store::progress(
            config,
            "backing_up",
            "MX is stopped. Creating and verifying the final local SQLite rollback snapshot.",
            0,
            0,
        )?;
        snapshot(config, &mut job)?;
        job.phase = "switching".into();
        store::atomic(&config.root.join("activation.json"), &job)?;
        store::progress(
            config,
            "activating",
            "Activating the matching binary and frontend.",
            0,
            0,
        )?;
        switch(config, &job.candidate_slot)?;
        job.phase = "validating".into();
        store::atomic(&config.root.join("activation.json"), &job)?;
        systemctl(config, &["start"]).await?;
        store::progress(
            config,
            "validating",
            "Checking the restarted candidate before reopening access.",
            0,
            0,
        )?;
        wait_healthy(config, &job.version, &job, true).await?;
        integrity(&config.live()?.database)?;
        let expected = manifest
            .files
            .iter()
            .find(|f| f.path == "frontend/dist/index.html")
            .ok_or("Missing frontend index.")?;
        let mut index = health_client()?
            .get(
                config
                    .live()?
                    .health_url
                    .replace("/mx/v1/system/update-health", "/"),
            )
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        use sha2::Digest;
        let mut digest = sha2::Sha256::new();
        let mut received = 0u64;
        while let Some(bytes) = index.chunk().await.map_err(|e| e.to_string())? {
            received = received
                .checked_add(bytes.len() as u64)
                .ok_or("Frontend response overflow.")?;
            if received > expected.size {
                return Err("Frontend response exceeds the signed index size.".into());
            }
            digest.update(&bytes);
        }
        if received != expected.size || package::hex(&digest.finalize()) != expected.sha256 {
            return Err("The restarted server is not serving the matching frontend.".into());
        }
        job.phase = "committed".into();
        store::atomic(&config.root.join("activation.json"), &job)?;
        Ok(())
    }
    .await;
    match activation {
        Ok(()) => finish_commit(config, &job).await,
        Err(error) => match rollback(config, &mut job, &error).await {
            Ok(()) => Ok(()),
            Err(recovery) => Err(format!(
                "{error} Automatic rollback also needs attention: {recovery}. Maintenance remains enabled; no new business writes are accepted."
            )),
        },
    }
}
fn in_window(config: &UpdateConfig) -> Result<bool> {
    let policy = store::policy(config)?;
    let now = chrono::Utc::now();
    Ok(policy.mode == "automatic" && policy.contains(now.hour(), now.minute()))
}
async fn schedule_next(config: &UpdateConfig) -> Result<()> {
    if !in_window(config)? {
        return Ok(());
    }
    let state = store::state(config)?;
    if let Some(offer) = &state.available {
        if package::newer(&offer.version, &installed_version(config).await?)?
            && !store::automatic_blocked(&state, &offer.version, store::now())
        {
            let action = if state
                .staged
                .as_ref()
                .is_some_and(|s| s.version == offer.version)
            {
                "install"
            } else {
                "stage"
            };
            store::queue(
                config,
                Request {
                    uid: uuid::Uuid::new_v4().to_string(),
                    action: action.into(),
                    version: Some(offer.version.clone()),
                    actor_uid: "scheduled-worker".into(),
                    created_at: store::now(),
                },
            )?;
        }
    }
    Ok(())
}
pub async fn run(config: &UpdateConfig) -> Result<()> {
    #[cfg(unix)]
    if unsafe { libc::geteuid() } == 0 {
        return Err("Run the update worker as the dedicated MX service user, never root. Grant only start/stop authority for its exact systemd unit.".into());
    }
    store::initialize(config)?;
    let _worker = store::Lock::acquire(&config.root.join("worker.lock"), false)?;
    store::atomic(
        &config.root.join("worker.json"),
        &serde_json::json!({"last_started_at":store::now(),"format":package::FORMAT}),
    )?;
    let journal = config.root.join("activation.json");
    if journal.exists() {
        let mut job: Activation = store::read(&journal)?;
        match job.phase.as_str() {
            "committed" if config.root.join("maintenance.json").exists() => {
                finish_commit(config, &job).await?;
                return Ok(());
            }
            "rolled_back" if config.root.join("maintenance.json").exists() => {
                start_previous(config).await?;
                wait_healthy(config, &job.previous_version, &job, false).await?;
                store::remove(&config.root.join("request.json"))?;
                store::remove(&config.root.join("announcement.json"))?;
                store::remove(&config.root.join("maintenance.json"))?;
                return Ok(());
            }
            "committed" | "rolled_back" => {}
            _ => {
                rollback(config,&mut job,"The updater was interrupted during activation; the verified pre-update state was restored.").await?;
                return Ok(());
            }
        }
    }
    let path = config.root.join("request.json");
    if !path.exists() {
        let state = store::state(config)?;
        if state.last_checked_at.is_some_and(|checked| {
            store::now() - checked < (config.check_interval_seconds * 1000) as i64
        }) {
            schedule_next(config).await?;
            return Ok(());
        }
        store::queue(
            config,
            Request {
                uid: uuid::Uuid::new_v4().to_string(),
                action: "check".into(),
                version: None,
                actor_uid: "scheduled-worker".into(),
                created_at: store::now(),
            },
        )?;
    }
    let request: Request = store::read(&path)?;
    if uuid::Uuid::parse_str(&request.uid).is_err() {
        return Err("Invalid durable request ID.".into());
    }
    if request.actor_uid == "scheduled-worker"
        && matches!(request.action.as_str(), "stage" | "install")
        && !in_window(config)?
    {
        store::terminal(
            config,
            "deferred",
            "The maintenance window ended before this automatic job started.",
            None,
        )?;
        store::remove(&path)?;
        return Ok(());
    }
    let result:Result<()>=async {
        match request.action.as_str() {
            "check"=>{
                store::progress(config,"checking","Checking the configured repository for trusted releases.",0,0)?;
                let offer=discover(config).await?;
                store::mutate(config,|s|{s.last_checked_at=Some(store::now());s.available=offer;Ok(())})?;
                store::terminal(config,"completed","Release check completed.",None)?;
            },
            "stage"|"import"=>{
                let package=config.root.join("incoming").join(format!("{}.mxupdate",request.uid));
                let expected=if request.action=="stage" {
                    let offer=store::state(config)?.available.ok_or("Check for a release first.")?;
                    if request.version.as_deref()!=Some(&offer.version){return Err("Release offer changed. Review the selected version again.".into());}
                    package::verify(config,&offer.signed)?;store::progress(config,"downloading","Starting the signed package download.",0,0)?;
                    // A prior interrupted download is never appended blindly.
                    if package.exists(){store::remove(&package)?;}
                    download(config,&offer,&package).await?;Some(offer.signed)
                }else{None};
                store::progress(config,"verifying","Verifying the package signature and file checksums.",0,0)?;
                let staged=package::stage(config,&package,expected.as_ref())?;
                if !package::newer(&staged.version,&installed_version(config).await?)? {return Err("Package is not newer than the installed release. Downgrades and historical 4.x crossgrades are rejected.".into());}
                if package::executable_version(&store::slot(config,&staged.slot)?.join("litiaina-mx")).await?!=staged.version {return Err("The package executable version differs from its signed release.".into());}
                store::mutate(config,|s|{s.staged=Some(staged);Ok(())})?;
                store::terminal(config,"completed","The signed release is verified and staged. The running installation has not changed.",None)?;
                store::remove(&package)?;
            },
            "install"=>{let staged=store::state(config)?.staged.ok_or("Stage and verify a signed release before installation.")?;install(config,&request,&staged).await?;},
            _=>return Err("Unknown update action.".into()),
        }Ok(())
    }.await;
    if let Err(error) = result {
        store::terminal(
            config,
            "failed",
            "Update failed. Inspect the details below before retrying.",
            Some(error.clone()),
        )?;
        if matches!(request.action.as_str(), "stage" | "import") {
            store::remove(
                &config
                    .root
                    .join("incoming")
                    .join(format!("{}.mxupdate", request.uid)),
            )?;
        }
        if !config.root.join("maintenance.json").exists() {
            store::remove(&path)?;
        }
        return Err(error);
    }
    store::remove(&path)?;
    // Automatic installation is opt-in, within an explicit UTC maintenance
    // window, and never loops a release that already failed or rolled back.
    schedule_next(config).await?;
    Ok(())
}
