use tower_http::services::ServeDir;

use crate::config::load_config::CONFIG;
use axum::{extract::Request, middleware::Next, response::Response};

pub fn serve_static_files() -> ServeDir {
    ServeDir::new(&CONFIG.static_hosting.static_web_path)
        .append_index_html_on_directories(true)
        .precompressed_br()
        .precompressed_gzip()
}

// Do not apply COEP globally: the normal workspace can use configured logos,
// call windows and other resources that are not cross-origin isolated.
pub async fn office_isolation(request: Request, next: Next) -> Response {
    let office = request.uri().path().starts_with("/office/");
    let mut response = next.run(request).await;
    if office {
        let headers = response.headers_mut();
        headers.insert("cross-origin-opener-policy", "same-origin".parse().unwrap());
        headers.insert(
            "cross-origin-embedder-policy",
            "require-corp".parse().unwrap(),
        );
        headers.insert(
            "cross-origin-resource-policy",
            "same-origin".parse().unwrap(),
        );
        headers.insert("referrer-policy", "no-referrer".parse().unwrap());
        // UNO's generated bindings need JavaScript evaluation as well as Wasm.
        // This exception is confined to the isolated editor, never the workspace.
        headers.insert("content-security-policy", "default-src 'self'; script-src 'self' 'unsafe-eval'; worker-src 'self' blob:; connect-src 'self' blob:; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; object-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'".parse().unwrap());
    }
    response
}
