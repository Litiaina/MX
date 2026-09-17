use ini::Ini;
use std::{io, path::Path};

pub fn init_default_config<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let mut conf = Ini::new();

    conf.with_section(Some("server"))
        .set("host", "127.0.0.1")
        .set("http_port", "20001")
        .set("https_port", "21001")
        .set("cert_path", "")
        .set("key_path", "");

    conf.with_section(Some("database"))
        .set("db_path", "lux.db")
        .set("pool_max_size", "16")
        .set("pool_min_idle", "4")
        .set("pool_connection_timeout_seconds", "10")
        .set("busy_timeout_milliseconds", "10000");

    conf.with_section(Some("jwt_token_config"))
        .set("login_token_expiration", "900")
        .set("refresh_token_expiration", "2592000");

    conf.with_section(Some("rate_limiter_config"))
        .set("enable_rate_limiting", "true")
        .set("server_limit_per_second", "200")
        .set("server_limit_burst_size", "300");

    conf.with_section(Some("static_site_hosting"))
        .set("static_site_path", "");

    // DGS attachment storage.
    // "fragment" is the hash of the dedicated N1 DGS fragment.
    conf.with_section(Some("n1"))
        .set("base_url", "https://127.0.0.1:50001")
        .set("fragment", "")
        .set("insecure_tls", "true")
        .set("attachment_max_size_mb", "50");

    conf.write_to_file(path)
        .map_err(|error| io::Error::new(io::ErrorKind::Other, error))
}
