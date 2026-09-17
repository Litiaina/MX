use tower_http::services::ServeDir;

use crate::config::load_config::CONFIG;

pub fn serve_static_files() -> ServeDir {
    ServeDir::new(&CONFIG.static_hosting.static_web_path)
        .append_index_html_on_directories(true)
}
