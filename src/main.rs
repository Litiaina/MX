use axum::handler::HandlerWithoutStateExt;
use axum::http::uri::Authority;
use axum::http::{StatusCode, Uri};
use axum::response::Redirect;
use axum::{BoxError, Router};

use axum_extra::extract::Host;
use axum_server::tls_rustls::RustlsConfig;
use chrono::Local;

use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::signal;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::api::hosting::handler::serve_static_files;
use crate::api::route::{api_route, handler_404};
use crate::config::init_config::init_default_config;
use crate::config::init_env::create_env_file;
use crate::config::load_config::CONFIG;
use crate::db::init_db::initialize_sql_db;

mod api;
mod config;
mod db;
mod macros;
mod middleware;
mod updates;
mod util;

pub const ENV_FILE: &str = "mx.env";
pub const CONFIG_FILE: &str = "mx.config";

#[derive(Debug, Clone, Copy)]
pub struct Ports {
    http: u16,
    https: u16,
}

#[tokio::main]
async fn main() {
    // Release identification must not initialize TLS, create configuration,
    // open the database or contact N1.
    if std::env::args_os()
        .skip(1)
        .any(|arg| arg == "--version" || arg == "-V")
    {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return;
    }

    let arguments = std::env::args_os()
        .skip(1)
        .map(|argument| argument.into_string())
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|_| {
            eprintln!(
                "MX command-line arguments must be UTF-8. Use --help for supported commands."
            );
            std::process::exit(2);
        });
    if arguments.len() == 1 && matches!(arguments[0].as_str(), "--help" | "-h") {
        println!(
            "Litiaina MX {}\n\nUsage:\n  litiaina-mx                 Start the configured server\n  litiaina-mx --version | -V  Print the built-in version\n  litiaina-mx --help | -h     Show this help\n  litiaina-mx --update-worker --config FILE\n  litiaina-mx --update-keygen PRIVATE_KEY_FILE\n  litiaina-mx --update-package RELEASE_DIRECTORY PRIVATE_KEY_FILE OUTPUT.mxupdate CHANNEL\n\nUpdate setup and safety: docs/updates.md",
            env!("CARGO_PKG_VERSION")
        );
        return;
    }
    if let Some(result) = updates::cli(arguments.clone()).await {
        if let Err(error) = result {
            eprintln!("MX updater: {error}");
            std::process::exit(1);
        }
        return;
    }
    if !arguments.is_empty() {
        eprintln!("Unknown MX command. Use --help; the server was not started.");
        std::process::exit(2);
    }

    rustls::crypto::ring::default_provider()
        .install_default()
        .unwrap_or_else(|_| {
            crate::fatal_error!(
                "failed to install 'ring' as the default TLS provider",
                "function",
                "main()"
            )
        });

    jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER
        .install_default()
        .expect("failed to install jsonwebtoken crypto provider");

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("{}=trace", env!("CARGO_CRATE_NAME")).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    if !Path::new(ENV_FILE).exists() {
        match create_env_file(ENV_FILE) {
            Ok(_) => {
                tracing::info!("Created environment file '{}'.", ENV_FILE);
            }

            Err(error) => {
                crate::report_error!(
                    format!(
                        "failed to create environment file '{}': {}",
                        ENV_FILE, error
                    ),
                    "function",
                    "main()"
                );

                return;
            }
        }
    } else {
        match dotenv::from_filename(ENV_FILE) {
            Ok(_) => {
                tracing::info!("Loaded environment variables from '{}'.", ENV_FILE);
            }

            Err(error) => {
                crate::report_error!(
                    format!("failed to load environment file '{}': {}", ENV_FILE, error),
                    "function",
                    "main()"
                );

                return;
            }
        }
    }

    if !Path::new(CONFIG_FILE).exists() {
        match init_default_config(CONFIG_FILE) {
            Ok(_) => {
                tracing::info!("Created default configuration file '{}'.", CONFIG_FILE);
            }

            Err(error) => {
                crate::report_error!(
                    format!(
                        "failed to create configuration file '{}': {}",
                        CONFIG_FILE, error
                    ),
                    "function",
                    "main()"
                );

                return;
            }
        }
    } else {
        tracing::info!("Configuration file '{}' exists.", CONFIG_FILE);
    }

    server().await;
}

async fn server() {
    if let Err(error) = updates::validate_startup() {
        eprintln!("MX updater configuration: {error}");
        std::process::exit(1);
    }
    let host = &CONFIG.server.host;

    let ip = host.parse::<IpAddr>().unwrap_or_else(|error| {
        crate::fatal_error!(
            format!("invalid host IP address '{}': {}", host, error),
            "function",
            "server()"
        )
    });

    initialize_sql_db().await;
    // Do not let background storage cleanup run against a candidate database
    // until the independent updater has accepted activation.
    tokio::spawn(async {
        while updates::in_maintenance() {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        tokio::spawn(crate::api::preferences::warm_notification_sound_cache());
        crate::api::drive::run_cleanup().await;
    });

    tracing::info!(
        "SQLite database initialized from '{}'.",
        CONFIG.database.db_path
    );

    let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    tracing::info!("╔════════════════════════════════════╗");
    tracing::info!("║        L I T I A I N A  M X        ║");
    tracing::info!("╚════════════════════════════════════╝");
    tracing::info!("{} - Status: ONLINE", current_time);
    tracing::info!("Press CTRL+C to terminate");
    tracing::info!("══════════════ ACTIVE CONNECTIONS ══════════════");

    let ports = Ports {
        http: CONFIG.server.http_port,
        https: CONFIG.server.https_port,
    };

    let certificate =
        RustlsConfig::from_pem_file(&CONFIG.server.cert_path, &CONFIG.server.key_path)
            .await
            .unwrap_or_else(|error| {
                crate::fatal_error!(
                    format!(
                        "failed to load TLS certificate '{}' or key '{}': {}",
                        CONFIG.server.cert_path, CONFIG.server.key_path, error
                    ),
                    "function",
                    "server()"
                )
            });

    let api_socket = SocketAddr::from((ip, ports.https));

    let application = Router::new()
        .merge(api_route())
        .route(
            "/mx/v1/system/update-health",
            axum::routing::get(updates::api::health),
        )
        .fallback_service(serve_static_files().not_found_service(handler_404.into_service()))
        .layer(axum::middleware::from_fn(
            crate::api::hosting::handler::office_isolation,
        ))
        .layer(axum::middleware::from_fn(updates::api::maintenance_gate));

    let api_task = tokio::spawn(serve(
        ip,
        api_socket,
        application,
        Arc::new("api".to_string()),
        ports,
        certificate,
    ));

    if let Err(error) = api_task.await {
        crate::report_error!(
            format!("API server task failed: {}", error),
            "function",
            "server()"
        );
    }
}

async fn serve(
    ip: IpAddr,
    socket: SocketAddr,
    app: Router,
    name: Arc<String>,
    ports: Ports,
    certificate: RustlsConfig,
) {
    tracing::info!("{} on port => {} type: https", name, socket);

    let handle = axum_server::Handle::new();

    let redirect_server_name = Arc::clone(&name);

    let shutdown_future = shutdown_signal(Arc::clone(&redirect_server_name), handle.clone());

    tokio::spawn(redirect_http_to_https(
        ip,
        redirect_server_name,
        ports,
        shutdown_future,
    ));

    let mut server = axum_server::bind_rustls(socket, certificate).handle(handle);

    server.http_builder().http2().enable_connect_protocol();

    if let Err(error) = server.serve(app.into_make_service()).await {
        crate::report_error!(
            format!("HTTPS server failed: {}", error),
            "function",
            "serve()"
        );
    }
}

async fn redirect_http_to_https<F>(ip: IpAddr, name: Arc<String>, ports: Ports, signal: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    fn make_https(host: &str, uri: Uri, https_port: u16) -> Result<Uri, BoxError> {
        let mut parts = uri.into_parts();

        parts.scheme = Some(axum::http::uri::Scheme::HTTPS);

        if parts.path_and_query.is_none() {
            parts.path_and_query = Some("/".parse().expect("failed to parse root URI"));
        }

        let authority: Authority = host.parse()?;

        let bare_host = match authority.port() {
            Some(port) => authority
                .as_str()
                .strip_suffix(port.as_str())
                .and_then(|value| value.strip_suffix(':'))
                .unwrap_or(authority.as_str()),

            None => authority.as_str(),
        };

        parts.authority = Some(format!("{}:{}", bare_host, https_port).parse()?);

        Ok(Uri::from_parts(parts)?)
    }

    let http_port = ports.http;
    let https_port = ports.https;

    let redirect = move |Host(host): Host, uri: Uri| async move {
        match make_https(&host, uri, https_port) {
            Ok(uri) => Ok(Redirect::permanent(&uri.to_string())),

            Err(error) => {
                crate::report_error!(
                    format!("failed to construct HTTPS redirect URI: {}", error),
                    "function",
                    "redirect_http_to_https()"
                );

                Err(StatusCode::BAD_REQUEST)
            }
        }
    };

    let address = SocketAddr::from((ip, http_port));

    let listener = tokio::net::TcpListener::bind(address)
        .await
        .unwrap_or_else(|error| {
            crate::fatal_error!(
                format!(
                    "failed to bind HTTP redirect listener to '{}': {}",
                    address, error
                ),
                "function",
                "redirect_http_to_https()"
            )
        });

    tracing::info!("{} on port => {} type: http", name, address);

    if let Err(error) = axum::serve(listener, redirect.into_make_service())
        .with_graceful_shutdown(signal)
        .await
    {
        crate::report_error!(
            format!("HTTP redirect server failed: {}", error),
            "function",
            "redirect_http_to_https()"
        );
    }
}

async fn shutdown_signal(name: Arc<String>, handle: axum_server::Handle) {
    let ctrl_c = async {
        signal::ctrl_c().await.unwrap_or_else(|error| {
            crate::fatal_error!(
                format!("failed to install Ctrl+C signal handler: {}", error),
                "function",
                "shutdown_signal()"
            )
        });
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .unwrap_or_else(|error| {
                crate::fatal_error!(
                    format!("failed to install termination signal handler: {}", error),
                    "function",
                    "shutdown_signal()"
                )
            })
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("{} => shutdown received", name);

    handle.graceful_shutdown(Some(Duration::from_secs(10)));
}
