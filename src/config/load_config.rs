use std::{path::Path, sync::LazyLock};

use ini::Ini;

use crate::CONFIG_FILE;

#[derive(Debug, Clone)]
pub struct ServerConfigValues {
    pub host: String,
    pub http_port: u16,
    pub https_port: u16,
    pub cert_path: String,
    pub key_path: String,
}

#[derive(Debug, Clone)]
pub struct DatabaseConfigValues {
    pub db_path: String,
    pub pool_max_size: u32,
    pub pool_min_idle: u32,
    pub pool_connection_timeout_seconds: u64,
    pub busy_timeout_milliseconds: u64,
}

#[derive(Debug, Clone)]
pub struct JwtTokenConfigValues {
    pub login_token_expiration: u64,
    pub refresh_token_expiration: u64,
}

#[derive(Debug, Clone)]
pub struct StaticHosting {
    pub static_web_path: String,
}

#[derive(Debug, Clone)]
pub struct N1Config {
    pub base_url: String,
    pub fragment: String,
    pub insecure_tls: bool,
    pub attachment_max_size_mb: usize,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub server: ServerConfigValues,
    pub database: DatabaseConfigValues,
    pub jwt_token_config: JwtTokenConfigValues,
    pub static_hosting: StaticHosting,
    pub n1: N1Config,
}

pub static CONFIG: LazyLock<Config> = LazyLock::new(|| {
    let configuration = Ini::load_from_file(CONFIG_FILE).unwrap_or_else(|error| {
        crate::fatal_error!(
            format!(
                "failed to load configuration file '{}': {}",
                CONFIG_FILE, error
            ),
            "static",
            "CONFIG"
        )
    });

    /*
     * Server
     */

    let server_section = configuration.section(Some("server")).unwrap_or_else(|| {
        crate::fatal_error!(
            "missing required configuration section '[server]'",
            "static",
            "CONFIG"
        )
    });

    /*
     * Database
     */

    let database_section = configuration.section(Some("database")).unwrap_or_else(|| {
        crate::fatal_error!(
            "missing required configuration section '[database]'",
            "static",
            "CONFIG"
        )
    });

    /*
     * JWT
     */

    let jwt_token_section = configuration
        .section(Some("jwt_token_config"))
        .unwrap_or_else(|| {
            crate::fatal_error!(
                "missing required configuration section '[jwt_token_config]'",
                "static",
                "CONFIG"
            )
        });

    /*
     * Static site hosting
     */

    let static_site_hosting = configuration
        .section(Some("static_site_hosting"))
        .unwrap_or_else(|| {
            crate::fatal_error!(
                "missing required configuration section '[static_site_hosting]'",
                "static",
                "CONFIG"
            )
        });

    /*
     * N1
     */

    let n1_section = configuration.section(Some("n1")).unwrap_or_else(|| {
        crate::fatal_error!(
            "missing required configuration section '[n1]'",
            "static",
            "CONFIG"
        )
    });

    /*
     * Server values
     */

    let host = server_section
        .get("host")
        .map(ToString::to_string)
        .unwrap_or_else(|| "127.0.0.1".to_string());

    let http_port_value = server_section.get("http_port").unwrap_or("20001");

    let http_port = http_port_value.parse::<u16>().unwrap_or_else(|error| {
        crate::fatal_error!(
            format!(
                "invalid value '{}' for 'http_port': {}",
                http_port_value, error
            ),
            "static",
            "CONFIG"
        )
    });

    let https_port_value = server_section.get("https_port").unwrap_or("21001");

    let https_port = https_port_value.parse::<u16>().unwrap_or_else(|error| {
        crate::fatal_error!(
            format!(
                "invalid value '{}' for 'https_port': {}",
                https_port_value, error
            ),
            "static",
            "CONFIG"
        )
    });

    let cert_path = server_section
        .get("cert_path")
        .map(ToString::to_string)
        .unwrap_or_else(|| "cert.pem".to_string());

    let key_path = server_section
        .get("key_path")
        .map(ToString::to_string)
        .unwrap_or_else(|| "key.pem".to_string());

    /*
     * Database values
     */

    let db_path = database_section
        .get("db_path")
        .map(ToString::to_string)
        .unwrap_or_else(|| "lux.db".to_string());

    let pool_max_size_value = database_section.get("pool_max_size").unwrap_or("16");

    let pool_max_size = pool_max_size_value.parse::<u32>().unwrap_or_else(|error| {
        crate::fatal_error!(
            format!(
                "invalid value '{}' for 'pool_max_size': {}",
                pool_max_size_value, error
            ),
            "static",
            "CONFIG"
        )
    });

    let pool_min_idle_value = database_section.get("pool_min_idle").unwrap_or("4");

    let pool_min_idle = pool_min_idle_value.parse::<u32>().unwrap_or_else(|error| {
        crate::fatal_error!(
            format!(
                "invalid value '{}' for 'pool_min_idle': {}",
                pool_min_idle_value, error
            ),
            "static",
            "CONFIG"
        )
    });

    if pool_min_idle > pool_max_size {
        crate::fatal_error!(
            format!(
                "'pool_min_idle' ({}) cannot be greater than 'pool_max_size' ({})",
                pool_min_idle, pool_max_size
            ),
            "static",
            "CONFIG"
        );
    }

    let pool_connection_timeout_seconds_value = database_section
        .get("pool_connection_timeout_seconds")
        .unwrap_or("10");

    let pool_connection_timeout_seconds = pool_connection_timeout_seconds_value
        .parse::<u64>()
        .unwrap_or_else(|error| {
            crate::fatal_error!(
                format!(
                    "invalid value '{}' for 'pool_connection_timeout_seconds': {}",
                    pool_connection_timeout_seconds_value, error
                ),
                "static",
                "CONFIG"
            )
        });

    let busy_timeout_milliseconds_value = database_section
        .get("busy_timeout_milliseconds")
        .unwrap_or("10000");

    let busy_timeout_milliseconds = busy_timeout_milliseconds_value
        .parse::<u64>()
        .unwrap_or_else(|error| {
            crate::fatal_error!(
                format!(
                    "invalid value '{}' for 'busy_timeout_milliseconds': {}",
                    busy_timeout_milliseconds_value, error
                ),
                "static",
                "CONFIG"
            )
        });

    /*
     * JWT values
     */

    let login_token_expiration_value = jwt_token_section
        .get("login_token_expiration")
        .unwrap_or_else(|| {
            crate::fatal_error!(
                "missing required value 'login_token_expiration' in section '[jwt_token_config]'",
                "static",
                "CONFIG"
            )
        });

    let login_token_expiration =
        login_token_expiration_value
            .parse::<u64>()
            .unwrap_or_else(|error| {
                crate::fatal_error!(
                    format!(
                        "invalid value '{}' for 'login_token_expiration': {}",
                        login_token_expiration_value, error
                    ),
                    "static",
                    "CONFIG"
                )
            });

    let refresh_token_expiration_value = jwt_token_section
        .get("refresh_token_expiration")
        .unwrap_or_else(|| {
            crate::fatal_error!(
                "missing required value 'refresh_token_expiration' in section '[jwt_token_config]'",
                "static",
                "CONFIG"
            )
        });

    let refresh_token_expiration = refresh_token_expiration_value
        .parse::<u64>()
        .unwrap_or_else(|error| {
            crate::fatal_error!(
                format!(
                    "invalid value '{}' for 'refresh_token_expiration': {}",
                    refresh_token_expiration_value, error
                ),
                "static",
                "CONFIG"
            )
        });

    /*
     * Static site hosting
     */

    let configured_static_site_path = static_site_hosting
        .get("static_site_path")
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            crate::fatal_error!(
                "missing required value 'static_site_path' in section '[static_site_hosting]'",
                "static",
                "CONFIG"
            )
        });

    // MX 1.0 initially called the browser asset directory `web`. Keep existing
    // deployment configurations working after the Svelte frontend replacement.
    let static_site_path_value = if matches!(
        configured_static_site_path.as_str(),
        "web" | "frontend/legacy"
    ) && !Path::new(&configured_static_site_path).exists()
        && Path::new("frontend/dist").is_dir()
    {
        "frontend/dist".to_string()
    } else {
        configured_static_site_path
    };

    /*
     * N1 values
     */

    let n1_base_url = n1_section
        .get("base_url")
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            crate::fatal_error!(
                "missing required value 'base_url' in section '[n1]'",
                "static",
                "CONFIG"
            )
        });

    if n1_base_url.trim().is_empty() {
        crate::fatal_error!(
            "'base_url' in section '[n1]' cannot be empty",
            "static",
            "CONFIG"
        );
    }

    let n1_fragment = n1_section
        .get("fragment")
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            crate::fatal_error!(
                "missing required value 'fragment' in section '[n1]'",
                "static",
                "CONFIG"
            )
        });

    /*
     * We intentionally allow fragment="" here.
     *
     * init_config.rs creates an empty fragment value on first run.
     * The MX/N1 handler will report that N1 has not yet been configured
     * when an attachment operation is attempted.
     */

    let n1_insecure_tls_value = n1_section.get("insecure_tls").unwrap_or("false");

    let n1_insecure_tls = n1_insecure_tls_value
        .parse::<bool>()
        .unwrap_or_else(|error| {
            crate::fatal_error!(
                format!(
                    "invalid value '{}' for 'insecure_tls' in section '[n1]': {}",
                    n1_insecure_tls_value, error
                ),
                "static",
                "CONFIG"
            )
        });

    let attachment_max_size_mb_value = n1_section.get("attachment_max_size_mb").unwrap_or("50");

    let attachment_max_size_mb = attachment_max_size_mb_value
        .parse::<usize>()
        .unwrap_or_else(|error| {
            crate::fatal_error!(
                format!(
                    "invalid value '{}' for 'attachment_max_size_mb' in section '[n1]': {}",
                    attachment_max_size_mb_value, error
                ),
                "static",
                "CONFIG"
            )
        });

    if attachment_max_size_mb == 0 {
        crate::fatal_error!(
            "'attachment_max_size_mb' in section '[n1]' must be greater than 0",
            "static",
            "CONFIG"
        );
    }

    /*
     * Final configuration
     */

    Config {
        server: ServerConfigValues {
            host,
            http_port,
            https_port,
            cert_path,
            key_path,
        },

        database: DatabaseConfigValues {
            db_path,
            pool_max_size,
            pool_min_idle,
            pool_connection_timeout_seconds,
            busy_timeout_milliseconds,
        },

        jwt_token_config: JwtTokenConfigValues {
            login_token_expiration,
            refresh_token_expiration,
        },

        static_hosting: StaticHosting {
            static_web_path: static_site_path_value,
        },

        n1: N1Config {
            base_url: n1_base_url,
            fragment: n1_fragment,
            insecure_tls: n1_insecure_tls,
            attachment_max_size_mb,
        },
    }
});
