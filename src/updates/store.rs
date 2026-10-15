use super::{UpdateConfig, package::SignedRelease};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
pub type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Offer {
    pub version: String,
    pub tag: String,
    pub notes: String,
    pub url: String,
    pub signed: SignedRelease,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Staged {
    pub version: String,
    pub slot: String,
    pub signed: SignedRelease,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub uid: String,
    pub action: String,
    #[serde(default)]
    pub actor_uid: String,
    pub phase: String,
    pub message: String,
    pub version: Option<String>,
    pub bytes: u64,
    pub total: u64,
    pub started_at: i64,
    pub updated_at: i64,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct State {
    pub last_checked_at: Option<i64>,
    pub available: Option<Offer>,
    pub staged: Option<Staged>,
    pub operation: Option<Operation>,
    pub history: Vec<Operation>,
    // Independent of the bounded UI history: trimming history must not cause
    // a previously failed activation to be installed automatically again.
    #[serde(default)]
    pub blocked_versions: Vec<String>,
    #[serde(default)]
    pub transfer_failures: std::collections::BTreeMap<String, TransferFailure>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferFailure {
    pub attempts: u32,
    pub next_retry_at: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub uid: String,
    pub action: String,
    pub version: Option<String>,
    pub actor_uid: String,
    pub created_at: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activation {
    pub uid: String,
    pub phase: String,
    pub previous_slot: String,
    pub candidate_slot: String,
    pub version: String,
    pub previous_version: String,
    pub snapshot: Option<String>,
    pub snapshot_sha256: Option<String>,
    pub started_at: i64,
    pub health_secret: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub mode: String,
    pub start_hour: u32,
    pub end_hour: u32,
    pub utc_offset_minutes: i32,
}
impl Policy {
    pub fn validate(&self) -> Result<()> {
        if !matches!(self.mode.as_str(), "manual" | "automatic")
            || self.start_hour > 23
            || self.end_hour > 23
            || self.start_hour == self.end_hour
            || !(-720..=840).contains(&self.utc_offset_minutes)
        {
            return Err("Choose manual/automatic, different maintenance start/end hours, and a valid UTC offset.".into());
        }
        Ok(())
    }
    pub fn contains(&self, utc_hour: u32, utc_minute: u32) -> bool {
        let minute =
            ((utc_hour * 60 + utc_minute) as i32 + self.utc_offset_minutes).rem_euclid(1440) as u32;
        let start = self.start_hour * 60;
        let end = self.end_hour * 60;
        if start < end {
            (start..end).contains(&minute)
        } else {
            minute >= start || minute < end
        }
    }
}
pub fn policy(config: &UpdateConfig) -> Result<Policy> {
    let path = config.root.join("policy.json");
    let policy = if path.exists() {
        read(&path)?
    } else {
        Policy {
            mode: if config.auto_install {
                "automatic"
            } else {
                "manual"
            }
            .into(),
            start_hour: config.maintenance_start_utc_hour.unwrap_or(2),
            end_hour: config.maintenance_end_utc_hour.unwrap_or(3),
            utc_offset_minutes: if config.maintenance_start_utc_hour.is_some() {
                0
            } else {
                480
            },
        }
    };
    policy.validate()?;
    Ok(policy)
}
pub fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
pub fn initialize(config: &UpdateConfig) -> Result<()> {
    fs::create_dir_all(&config.root).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&config.root, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    for name in ["slots", "incoming", "backups", "recovery"] {
        fs::create_dir_all(config.root.join(name)).map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let file = File::open(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let mut data = Vec::new();
    file.take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() > 2 * 1024 * 1024 {
        return Err("Update metadata exceeds its safety budget.".into());
    }
    serde_json::from_slice(&data)
        .map_err(|e| format!("Invalid update metadata {}: {e}", path.display()))
}
pub fn sync_dir(path: &Path) -> Result<()> {
    File::open(if path.as_os_str().is_empty() {
        Path::new(".")
    } else {
        path
    })
    .and_then(|f| f.sync_all())
    .map_err(|e| format!("Cannot sync {}: {e}", path.display()))
}
pub fn atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp).map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    fs::rename(&temp, path).map_err(|e| e.to_string())?;
    sync_dir(path.parent().ok_or("No parent directory.")?)
}
pub struct Lock {
    file: File,
}
impl Lock {
    pub fn acquire(path: &Path, wait: bool) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let flag = libc::LOCK_EX | if wait { 0 } else { libc::LOCK_NB };
            if unsafe { libc::flock(file.as_raw_fd(), flag) } != 0 {
                return Err("Another update operation is already running.".into());
            }
        }
        #[cfg(not(unix))]
        return Err("Update installation currently requires Linux/systemd.".into());
        Ok(Self { file })
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
        }
    }
}
pub fn state(config: &UpdateConfig) -> Result<State> {
    let path = config.root.join("state.json");
    if path.exists() {
        read(&path)
    } else {
        Ok(State::default())
    }
}
pub fn automatic_blocked(state: &State, version: &str, at: i64) -> bool {
    if state
        .blocked_versions
        .iter()
        .any(|blocked| blocked == version)
    {
        return true;
    }
    if state
        .staged
        .as_ref()
        .is_some_and(|slot| slot.version == version)
    {
        return false;
    }
    // A transient transfer is not a bad activation. Permit at most three
    // attempts, with a 15-minute backoff, before requiring operator review.
    state
        .transfer_failures
        .get(version)
        .is_some_and(|failure| failure.attempts >= 3 || at < failure.next_retry_at)
}
pub fn mutate(
    config: &UpdateConfig,
    operation: impl FnOnce(&mut State) -> Result<()>,
) -> Result<State> {
    let _lock = Lock::acquire(&config.root.join("state.lock"), true)?;
    let mut state = state(config)?;
    operation(&mut state)?;
    atomic(&config.root.join("state.json"), &state)?;
    Ok(state)
}
pub fn progress(
    config: &UpdateConfig,
    phase: &str,
    message: &str,
    bytes: u64,
    total: u64,
) -> Result<()> {
    mutate(config, |state| {
        if let Some(op) = &mut state.operation {
            op.phase = phase.into();
            op.message = message.into();
            op.bytes = bytes;
            op.total = total;
            op.updated_at = now();
        }
        Ok(())
    })
    .map(|_| ())
}
pub fn terminal(
    config: &UpdateConfig,
    phase: &str,
    message: &str,
    error: Option<String>,
) -> Result<()> {
    mutate(config, |state| {
        if let Some(op) = &mut state.operation {
            op.phase = phase.into();
            op.message = message.into();
            op.error = error;
            op.updated_at = now();
            let entry = op.clone();
            if op.action == "stage" {
                if let Some(version) = &op.version {
                    if phase == "failed" {
                        let failure = state.transfer_failures.entry(version.clone()).or_insert(
                            TransferFailure {
                                attempts: 0,
                                next_retry_at: 0,
                            },
                        );
                        failure.attempts = failure.attempts.saturating_add(1);
                        failure.next_retry_at = now() + 15 * 60 * 1000;
                    } else if phase == "completed" {
                        state.transfer_failures.remove(version);
                    }
                }
            }
            if op.action == "install" {
                if let Some(version) = &op.version {
                    if matches!(phase, "failed" | "rolled_back")
                        && !state.blocked_versions.contains(version)
                    {
                        state.blocked_versions.push(version.clone());
                    } else if phase == "completed" {
                        state.blocked_versions.retain(|blocked| blocked != version);
                    }
                }
            }
            if let Some(previous) = state.history.iter_mut().find(|old| old.uid == entry.uid) {
                *previous = entry;
            } else {
                state.history.push(entry);
            }
            if state.history.len() > 30 {
                state.history.remove(0);
            }
        }
        Ok(())
    })
    .map(|_| ())
}
pub fn queue(config: &UpdateConfig, request: Request) -> Result<()> {
    initialize(config)?;
    if uuid::Uuid::parse_str(&request.uid).is_err()
        || !matches!(
            request.action.as_str(),
            "check" | "stage" | "install" | "import"
        )
    {
        return Err("Invalid update request.".into());
    }
    let _lock = Lock::acquire(&config.root.join("state.lock"), true)?;
    let mut state = state(config)?;
    if let Some(op) = state
        .operation
        .as_ref()
        .filter(|op| op.uid == request.uid)
        .or_else(|| state.history.iter().find(|op| op.uid == request.uid))
    {
        return if op.action == request.action && op.version == request.version {
            Ok(())
        } else {
            Err("This update operation ID was already used for a different request.".into())
        };
    }
    if config.root.join("request.json").exists() || config.root.join("maintenance.json").exists() {
        return Err(
            "An update is already queued or maintenance needs recovery. Check update history."
                .into(),
        );
    }
    if matches!(request.action.as_str(), "stage" | "install") && request.version.is_none() {
        return Err("Choose the exact release version first.".into());
    }
    state.operation = Some(Operation {
        uid: request.uid.clone(),
        action: request.action.clone(),
        actor_uid: request.actor_uid.clone(),
        phase: "queued".into(),
        message: "Waiting for the independent update worker.".into(),
        version: request.version.clone(),
        bytes: 0,
        total: 0,
        started_at: request.created_at,
        updated_at: now(),
        error: None,
    });
    atomic(&config.root.join("state.json"), &state)?;
    atomic(&config.root.join("request.json"), &request)
}
pub fn remove(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => sync_dir(path.parent().ok_or("No parent.")?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
pub fn slot(config: &UpdateConfig, name: &str) -> Result<PathBuf> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        || name == "."
        || name == ".."
    {
        return Err("Invalid immutable release slot.".into());
    }
    Ok(config.root.join("slots").join(name))
}
