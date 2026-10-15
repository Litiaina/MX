use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UpdateConfig {
    pub format: u32,
    pub root: PathBuf,
    pub deployment_dir: PathBuf,
    pub service: String,
    pub scope: String,
    pub repository: String,
    pub channel: String,
    pub trusted_keys: BTreeMap<String, String>,
    pub check_interval_seconds: u64,
    pub drain_timeout_seconds: u64,
    pub health_timeout_seconds: u64,
    pub max_package_bytes: u64,
    pub max_unpacked_bytes: u64,
    pub auto_install: bool,
    pub notice_seconds: u64,
    pub maintenance_start_utc_hour: Option<u32>,
    pub maintenance_end_utc_hour: Option<u32>,
    // Explicit local fixture/LAN feed, never an API-controlled arbitrary URL.
    pub feed_url: Option<String>,
    pub allow_loopback_http: bool,
}
impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            format: 1,
            root: PathBuf::new(),
            deployment_dir: PathBuf::new(),
            service: "mx.service".into(),
            scope: "user".into(),
            repository: "Litiaina/MX".into(),
            channel: "rc".into(),
            trusted_keys: BTreeMap::new(),
            check_interval_seconds: 21600,
            drain_timeout_seconds: 60,
            health_timeout_seconds: 90,
            max_package_bytes: 2 * 1024 * 1024 * 1024,
            max_unpacked_bytes: 8 * 1024 * 1024 * 1024,
            auto_install: false,
            notice_seconds: 60,
            maintenance_start_utc_hour: None,
            maintenance_end_utc_hour: None,
            feed_url: None,
            allow_loopback_http: false,
        }
    }
}
impl UpdateConfig {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes =
            std::fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
        if bytes.len() > 65536 {
            return Err("Update configuration is too large.".into());
        }
        let config: Self = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Invalid update configuration: {e}"))?;
        config.validate()?;
        if config.root.exists()
            && std::fs::canonicalize(&config.root).map_err(|e| e.to_string())? != config.root
        {
            return Err("Use the physical, non-symlink update root path in configuration.".into());
        }
        Ok(config)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.format != 1
            || !self.root.is_absolute()
            || !self.deployment_dir.is_absolute()
            || self.root.parent().is_none()
            || self.deployment_dir.parent().is_none()
            || self
                .root
                .components()
                .chain(self.deployment_dir.components())
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err("Update format must be 1; root and deployment_dir must be explicit absolute directories, never filesystem root.".into());
        }
        if self.deployment_dir.starts_with(&self.root)
            || self.root.starts_with(&self.deployment_dir.join("frontend"))
        {
            return Err(
                "Update state must be separate from the frontend and deployment directory.".into(),
            );
        }
        if !matches!(self.scope.as_str(), "user" | "system")
            || !self.service.ends_with(".service")
            || self.service.starts_with('-')
            || !self
                .service
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.@".contains(c))
        {
            return Err("Invalid exact systemd service or scope.".into());
        }
        let components: Vec<_> = self.repository.split('/').collect();
        if components.len() != 2
            || components.iter().any(|s| {
                s.is_empty()
                    || !s
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
            })
        {
            return Err("Repository must be owner/name.".into());
        }
        if !matches!(self.channel.as_str(), "stable" | "rc")
            || self.trusted_keys.is_empty()
            || self
                .trusted_keys
                .values()
                .any(|key| super::package::hex_decode(key).is_err() || key.len() != 64)
        {
            return Err(
                "Configure stable or rc and at least one 32-byte Ed25519 public key (hex).".into(),
            );
        }
        if !(60..=604800).contains(&self.check_interval_seconds)
            || !(1..=3600).contains(&self.drain_timeout_seconds)
            || !(1..=600).contains(&self.health_timeout_seconds)
            || self.notice_seconds > 3600
            || self.max_package_bytes == 0
            || self.max_unpacked_bytes == 0
        {
            return Err("Invalid update interval, timeout or package safety budget.".into());
        }
        if self.auto_install
            && !matches!((self.maintenance_start_utc_hour,self.maintenance_end_utc_hour),(Some(a),Some(b)) if a<24 && b<24 && a!=b)
        {
            return Err(
                "Automatic installation requires a non-empty UTC maintenance window (hours 0–23)."
                    .into(),
            );
        }
        if let Some(feed) = &self.feed_url {
            self.validate_url(feed)?;
        }
        Ok(())
    }
    pub fn validate_url(&self, value: &str) -> Result<(), String> {
        let url = reqwest::Url::parse(value).map_err(|_| "Invalid update URL.")?;
        if !url.username().is_empty() || url.password().is_some() {
            return Err("Update URLs must not contain credentials.".into());
        }
        if url.scheme() == "https" {
            if matches!(
                url.host_str(),
                Some(
                    "api.github.com"
                        | "github.com"
                        | "release-assets.githubusercontent.com"
                        | "objects.githubusercontent.com"
                )
            ) {
                return Ok(());
            }
            if let Some(feed) = &self.feed_url {
                if reqwest::Url::parse(feed).is_ok_and(|feed| feed.origin() == url.origin()) {
                    return Ok(());
                }
            }
        }
        if self.allow_loopback_http
            && url.scheme() == "http"
            && matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "::1"))
        {
            if let Some(feed) = &self.feed_url {
                if reqwest::Url::parse(feed).is_ok_and(|feed| feed.origin() == url.origin()) {
                    return Ok(());
                }
            }
        }
        Err("Update URLs require HTTPS and a trusted release origin. HTTP is restricted to explicitly enabled loopback tests.".into())
    }
    pub fn feed(&self) -> String {
        self.feed_url.clone().unwrap_or_else(|| {
            format!(
                "https://api.github.com/repos/{}/releases?per_page=100",
                self.repository
            )
        })
    }
    pub fn live(&self) -> Result<LiveConfig, String> {
        let ini = ini::Ini::load_from_file(self.deployment_dir.join("mx.config"))
            .map_err(|e| format!("Cannot read deployment mx.config: {e}"))?;
        let resolve = |path: &str| {
            let path = PathBuf::from(path);
            if path.is_absolute() {
                path
            } else {
                self.deployment_dir.join(path)
            }
        };
        let db = resolve(
            ini.get_from(Some("database"), "db_path")
                .ok_or("Missing deployment database path.")?,
        );
        let frontend = resolve(
            ini.get_from(Some("static_site_hosting"), "static_site_path")
                .ok_or("Missing frontend path.")?,
        );
        let port = ini
            .get_from(Some("server"), "https_port")
            .unwrap_or("21001")
            .parse::<u16>()
            .map_err(|_| "Invalid HTTPS port.")?;
        let host = ini
            .get_from(Some("server"), "host")
            .unwrap_or("127.0.0.1")
            .parse::<std::net::IpAddr>()
            .map_err(|_| "Invalid deployment bind address.")?;
        let host = match host {
            std::net::IpAddr::V4(ip) if ip.is_unspecified() => {
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
            }
            std::net::IpAddr::V6(ip) if ip.is_unspecified() => {
                std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)
            }
            ip => ip,
        };
        // A specific LAN bind is valid, but never send an insecure readiness
        // probe to a configurable remote host or through an HTTP proxy.
        let _local = std::net::UdpSocket::bind(std::net::SocketAddr::new(host, 0))
            .map_err(|_| "The configured MX bind address is not local to this server.")?;
        Ok(LiveConfig {
            database: db,
            frontend,
            health_url: format!(
                "https://{}/mx/v1/system/update-health",
                std::net::SocketAddr::new(host, port)
            ),
        })
    }
}
pub struct LiveConfig {
    pub database: PathBuf,
    pub frontend: PathBuf,
    pub health_url: String,
}
pub fn server_config() -> Result<Option<UpdateConfig>, String> {
    let explicit = std::env::var_os("MX_UPDATE_CONFIG");
    let path = explicit
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("mx-updates.json"));
    if !path.exists() && explicit.is_none() {
        return Ok(None);
    }
    UpdateConfig::load(&path).map(Some)
}
