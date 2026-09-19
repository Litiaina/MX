use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderName, Method, StatusCode};
use axum::response::{Html, IntoResponse};
use axum::routing::{delete, get, patch, post, put};
use tower_http::cors::{Any, CorsLayer};

use crate::api::admin::handler::{
    check_2fa_status, create_super_user, delete_super_user, disable_2fa_user, enable_2fa_user,
    execute_query, get_user, modify_super_user,
};

use crate::api::mx::handler::{
    delete_mx_attachment, delete_mx_record, download_mx_attachment, preview_mx_attachment,
    upload_mx_attachment_field,
};
use crate::api::mx::records::{create_mx_record, list_mx_records, update_mx_record};
use crate::api::mx::schema::{
    create_schema_field, get_admin_record_schema, get_record_schema, update_schema_field,
    update_schema_order, update_system_schema_field,
};

use crate::api::mx::storage::{get_storage_layout, update_storage_layout};

use crate::api::audit::{audit_request, get_audit_log};
use crate::api::backup::{create_backup, download_backup, list_backups, verify_backup};
use crate::api::dashboard::get_records_revision;
use crate::api::deployment::{get_deployment_config, save_deployment_config};
use crate::api::reports::{
    export_report_csv, get_action_rate_report, get_dashboard_config, get_user_performance_report,
    save_dashboard_config,
};
use crate::api::user::handler::{create_user, delete_user, modify_user};

use crate::config::load_config::CONFIG;
use crate::middleware::auth::{
    auth, authorize, bootstrap_status, refresh_access_token, session_info, signup_auth,
};

pub fn api_route() -> Router {
    Router::new()
        .merge(bootstrap_routes())
        .merge(public_routes())
        .merge(admin_routes())
        .merge(user_routes())
        .merge(mx_routes())
        .layer(axum::middleware::from_fn(audit_request))
        .layer(cors_layer())
}

fn public_routes() -> Router {
    Router::new()
        .route("/ping", get(ping))
        .route("/mx/v1/auth/bootstrap/status", get(bootstrap_status))
        .route("/mx/v1/auth/authenticate", post(authorize))
        .route("/mx/v1/auth/refresh", post(refresh_access_token))
        .route("/mx/v1/deployment/config", get(get_deployment_config))
}

fn bootstrap_routes() -> Router {
    Router::new()
        .route("/mx/v1/auth/create", post(create_super_user))
        .layer(axum::middleware::from_fn(signup_auth))
}

fn admin_routes() -> Router {
    Router::new()
        .route("/mx/v1/admin/audit", get(get_audit_log))
        .route("/mx/v1/admin/schema", get(get_admin_record_schema))
        .route("/mx/v1/admin/schema/fields", post(create_schema_field))
        .route("/mx/v1/admin/schema/order", put(update_schema_order))
        .route("/mx/v1/admin/schema/fields/{uid}", put(update_schema_field))
        .route(
            "/mx/v1/admin/schema/system/{key}",
            put(update_system_schema_field),
        )
        .route("/mx/v1/admin/dashboard-config", put(save_dashboard_config))
        .route(
            "/mx/v1/admin/deployment-config",
            put(save_deployment_config),
        )
        .route(
            "/mx/v1/admin/storage-layout",
            get(get_storage_layout).put(update_storage_layout),
        )
        .route(
            "/mx/v1/admin/backups",
            get(list_backups).post(create_backup),
        )
        .route("/mx/v1/admin/backups/{uid}/verify", post(verify_backup))
        .route("/mx/v1/admin/backups/{uid}/download", get(download_backup))
        .route("/mx/v1/auth/get", post(get_user))
        .route("/mx/v1/auth/modify", patch(modify_super_user))
        .route("/mx/v1/auth/delete", delete(delete_super_user))
        .route("/mx/v1/auth/enable_2fa", post(enable_2fa_user))
        .route("/mx/v1/auth/disable_2fa", post(disable_2fa_user))
        .route("/mx/v1/auth/check_2fa", post(check_2fa_status))
        .route("/mx/v1/db/query", post(execute_query))
        .layer(axum::middleware::from_fn(auth))
}

fn user_routes() -> Router {
    Router::new()
        .route("/mx/v1/auth/session", get(session_info))
        .route("/mx/v1/user/create", post(create_user))
        .route("/mx/v1/user/modify", patch(modify_user))
        .route("/mx/v1/user/delete", delete(delete_user))
        .layer(axum::middleware::from_fn(auth))
}

fn mx_routes() -> Router {
    let attachment_body_limit = CONFIG
        .n1
        .attachment_max_size_mb
        .saturating_mul(1024 * 1024)
        .saturating_add(1024 * 1024);

    Router::new()
        .route("/mx/v1/schema", get(get_record_schema))
        .route("/mx/v1/dashboard/config", get(get_dashboard_config))
        .route("/mx/v1/reports/action-rate", get(get_action_rate_report))
        .route(
            "/mx/v1/reports/user-performance",
            get(get_user_performance_report),
        )
        .route("/mx/v1/reports/export.csv", get(export_report_csv))
        .route("/mx/v1/status/revision", get(get_records_revision))
        .route(
            "/mx/v1/records",
            get(list_mx_records).post(create_mx_record),
        )
        .route(
            "/mx/v1/records/{uid}",
            put(update_mx_record).delete(delete_mx_record),
        )
        .route(
            "/mx/v1/records/{uid}/attachments/fields/{field_uid}",
            post(upload_mx_attachment_field).layer(DefaultBodyLimit::max(attachment_body_limit)),
        )
        .route(
            "/mx/v1/records/{record_uid}/attachments/{attachment_uid}",
            delete(delete_mx_attachment),
        )
        .route(
            "/mx/v1/records/{record_uid}/attachments/{attachment_uid}/preview",
            get(preview_mx_attachment),
        )
        .route(
            "/mx/v1/records/{record_uid}/attachments/{attachment_uid}/download",
            get(download_mx_attachment),
        )
        .layer(axum::middleware::from_fn(auth))
}

fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_headers([
            HeaderName::from_static("authorization"),
            HeaderName::from_static("content-type"),
            HeaderName::from_static("if-none-match"),
        ])
        .expose_headers([
            HeaderName::from_static("etag"),
            HeaderName::from_static("content-disposition"),
        ])
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
}

async fn ping() -> &'static str {
    "pong"
}

pub async fn handler_404() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Html(
            r#"
            <!doctype html>
            <html>
            <head>
                <meta charset="utf-8">
                <title>404</title>
                <style>
                    body {
                        margin: 0;
                        min-height: 100vh;
                        display: grid;
                        place-items: center;
                        background: #000;
                        color: #fff;
                        font-family: Arial, Helvetica, sans-serif;
                    }

                    .card {
                        padding: 40px;
                        background: #212121;
                        border-radius: 12px;
                        text-align: center;
                    }

                    h1 {
                        margin: 0;
                        font-size: 72px;
                        color: #ad2121;
                    }

                    p {
                        color: #ccc;
                    }
                </style>
            </head>

            <body>
                <div class="card">
                    <h1>404</h1>
                    <p>Page not found</p>
                </div>
            </body>
            </html>
            "#,
        ),
    )
}
