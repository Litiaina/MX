//! MX's fragment-scoped N1 transport. Format 4 uploads create virtual prefixes:
//! never stat/mkdir parents before writing an object key.
use super::handler::MxOperationError;
use axum::http::{StatusCode, header::CONTENT_TYPE};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;

#[cfg(test)]
#[path = "n1_tests.rs"]
mod tests;

pub(crate) fn fragment_identifier(configured: &str, secret: &str) -> String {
    // Some older MX configurations copied the raw fragment into both settings.
    // N1 requires its SHA-256 identifier. Never guess/fallback for other failures.
    if configured == secret {
        use sha2::{Digest, Sha256};
        Sha256::digest(configured.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    } else {
        configured.to_owned()
    }
}

#[derive(Debug, Clone, Serialize)]
struct N1MultipartParameters {
    content_type: String,
    total_size_bytes: u64,
    expected_parts: u64,
    part_size_bytes: u64,
}

#[derive(Debug, Deserialize)]
struct N1MultipartSession {
    session_id: String,
    version_id: String,
}

pub(crate) struct N1Storage<'a> {
    client: &'a Client,
    base_url: String,
    token: &'a str,
    part_size_mb: usize,
}

fn multipart_plan(total_size: usize, configured_part_size_mb: usize) -> (usize, usize) {
    let part_size = configured_part_size_mb
        .clamp(5, 100)
        .saturating_mul(1024 * 1024)
        .max(1);
    let expected_parts = total_size.div_ceil(part_size).max(1);
    (part_size, expected_parts)
}

impl<'a> N1Storage<'a> {
    pub(crate) async fn upload_batch(
        &self,
        objects: Vec<(String, String, Vec<u8>)>,
    ) -> Result<serde_json::Value, MxOperationError> {
        let boundary = format!("mx-{}", uuid::Uuid::new_v4().simple());
        let mut body = Vec::new();
        for (key, mime, bytes) in objects {
            validate_object_key(&key)?;
            if bytes.len() > 1024 * 1024 || mime.chars().any(char::is_control) {
                return Err(MxOperationError::InvalidRequest(
                    "Invalid small-file batch object".into(),
                ));
            }
            let encoded: String = key
                .bytes()
                .map(|b| {
                    if b.is_ascii_alphanumeric() || b"._-".contains(&b) {
                        (b as char).to_string()
                    } else {
                        format!("%{b:02X}")
                    }
                })
                .collect();
            body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"object\"; filename=\"{encoded}\"\r\nContent-Type: {mime}\r\n\r\n").as_bytes());
            body.extend_from_slice(&bytes);
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        let response = self
            .client
            .post(format!("{}/noa/v1/upload/batch", self.base_url))
            .bearer_auth(self.token)
            .query(&[("concurrency", "2"), ("durability", "batched")])
            .header(
                CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(body)
            .timeout(Duration::from_secs(120))
            .send()
            .await
            .map_err(|_| MxOperationError::N1("Batch acknowledgement interrupted".into()))?;
        Self::durable_json(response).await
    }
    /// Drive keeps session identities server-side; browsers receive only MX operation IDs.
    pub(crate) async fn durable_init(
        &self,
        key: &str,
        mime: &str,
        size: u64,
        part_size: u64,
    ) -> Result<serde_json::Value, MxOperationError> {
        let response=self.client.post(self.object_url("/noa/v1/upload/multipart/durable/init",key)?)
          .bearer_auth(self.token).timeout(Duration::from_secs(30))
          .json(&json!({"content_type":mime,"total_size_bytes":size,"expected_parts":size.div_ceil(part_size).max(1),"part_size_bytes":part_size})).send().await
          .map_err(|_|MxOperationError::N1("Durable initialization interrupted".into()))?;
        Self::durable_json(response).await
    }
    async fn durable_json(
        response: reqwest::Response,
    ) -> Result<serde_json::Value, MxOperationError> {
        if !response.status().is_success() {
            return Err(MxOperationError::N1(format!(
                "Durable multipart returned HTTP {}",
                response.status()
            )));
        }
        response
            .json()
            .await
            .map_err(|_| MxOperationError::N1("Invalid durable multipart response".into()))
    }
    pub(crate) async fn durable_request(
        &self,
        method: reqwest::Method,
        key: &str,
        version: &str,
        suffix: &str,
        coordinator: Option<&str>,
        body: Option<axum::body::Bytes>,
        json_body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, MxOperationError> {
        let deleting = method == reqwest::Method::DELETE;
        let uploading_part = method == reqwest::Method::PUT;
        if version.is_empty()
            || !version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || suffix.contains("..")
        {
            return Err(MxOperationError::InvalidRequest(
                "Invalid multipart identity".into(),
            ));
        }
        let mut request = self
            .client
            .request(
                method,
                format!(
                    "{}/noa/v1/upload/multipart/durable/{version}{suffix}",
                    self.base_url
                ),
            )
            .bearer_auth(self.token)
            .timeout(Duration::from_secs(120))
            .query(&[("object_key", key)]);
        if let Some(coordinator) = coordinator.filter(|value| !value.is_empty()) {
            request = request.query(&[("coordinator_node_id", coordinator)]);
        }
        if let Some(bytes) = body {
            request = request
                .header(CONTENT_TYPE, "application/octet-stream")
                .query(&[("durability", "batched")])
                .body(bytes);
        }
        if let Some(value) = json_body {
            request = request.json(&value);
        }
        let response = request
            .send()
            .await
            .map_err(|_| MxOperationError::N1("Durable transfer interrupted".into()))?;
        if deleting
            && (response.status() == StatusCode::NOT_FOUND
                || response.status() == StatusCode::NO_CONTENT)
        {
            return Ok(json!({"ok":true}));
        }
        // N1 durable part acknowledgements are plain text (including replay).
        // Init/status/finalize still require their structured JSON contracts.
        if uploading_part && response.status().is_success() {
            return Ok(json!({"ok":true}));
        }
        Self::durable_json(response).await
    }
    pub(crate) async fn is_finalized(&self, key: &str, version: &str) -> bool {
        self.finalized_version(key, version).await
    }
    pub(super) fn new(
        client: &'a Client,
        base_url: &str,
        token: &'a str,
        part_size_mb: usize,
    ) -> Self {
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
            part_size_mb,
        }
    }

    fn object_url(&self, route: &str, object_key: &str) -> Result<reqwest::Url, MxOperationError> {
        validate_object_key(object_key)?;
        let mut url = reqwest::Url::parse(&format!("{}{route}", self.base_url))
            .map_err(|error| MxOperationError::N1(format!("Invalid N1 URL: {error}")))?;
        url.path_segments_mut()
            .map_err(|_| MxOperationError::N1("N1 URL cannot contain object paths".into()))?
            .extend(object_key.split('/'));
        Ok(url)
    }

    pub(super) async fn upload_one_shot(
        &self,
        object_key: &str,
        mime_type: &str,
        bytes: Vec<u8>,
    ) -> Result<(), MxOperationError> {
        let token = self.token;

        let response = self
            .client
            .post(self.object_url("/noa/v1/upload/one-shot", object_key)?)
            .timeout(Duration::from_secs(300))
            .bearer_auth(token)
            .header(CONTENT_TYPE, mime_type)
            .body(bytes)
            .send()
            .await
            .map_err(|error| {
                MxOperationError::N1(format!("failed to upload '{object_key}' to N1: {error}"))
            })?;

        let status = response.status();

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();

            return Err(MxOperationError::N1(format!(
                "N1 upload failed for '{object_key}' with HTTP {status}: {text}"
            )));
        }

        Ok(())
    }

    async fn cancel_multipart(&self, object_key: &str, version_id: &str) {
        let base_url = &self.base_url;
        let token = self.token;
        let _ = self
            .client
            .delete(format!("{base_url}/noa/v1/upload/multipart/{version_id}"))
            .timeout(Duration::from_secs(10))
            .bearer_auth(token)
            .query(&[("object_key", object_key)])
            .send()
            .await;
    }

    pub(super) async fn upload_multipart(
        &self,
        object_key: &str,
        mime_type: &str,
        bytes: Vec<u8>,
    ) -> Result<(), MxOperationError> {
        let base_url = &self.base_url;
        let token = self.token;
        let (part_size, expected_parts) = multipart_plan(bytes.len(), self.part_size_mb);
        let parameters = N1MultipartParameters {
            content_type: mime_type.to_string(),
            total_size_bytes: bytes.len() as u64,
            expected_parts: expected_parts as u64,
            part_size_bytes: part_size as u64,
        };

        let response = self
            .client
            .post(self.object_url("/noa/v1/upload/multipart/init", object_key)?)
            .timeout(Duration::from_secs(20))
            .bearer_auth(token)
            .json(&parameters)
            .send()
            .await
            .map_err(|error| {
                MxOperationError::N1(format!(
                    "failed to initialize multipart upload for '{object_key}': {error}"
                ))
            })?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(MxOperationError::N1(format!(
                "N1 multipart initialization failed for '{object_key}' with HTTP {status}: {text}"
            )));
        }

        let session = response
            .json::<N1MultipartSession>()
            .await
            .map_err(|error| {
                MxOperationError::N1(format!(
                    "N1 returned an invalid multipart session for '{object_key}': {error}"
                ))
            })?;

        for (part_index, part) in bytes.chunks(part_size).enumerate() {
            if let Err(error) = self
                .upload_part(&session.version_id, part_index, part)
                .await
            {
                self.cancel_multipart(object_key, &session.version_id).await;
                return Err(MxOperationError::N1(format!(
                    "N1 multipart part {part_index} failed for '{object_key}': {error}"
                )));
            }
        }

        let response = self
            .client
            .post(format!(
                "{base_url}/noa/v1/upload/multipart/{}/{}/finalize",
                session.version_id, session.session_id
            ))
            .timeout(Duration::from_secs(120))
            .bearer_auth(token)
            .query(&[("object_key", object_key)])
            .json(&parameters)
            .send()
            .await;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                if self
                    .finalized_version(object_key, &session.version_id)
                    .await
                {
                    return Ok(());
                }
                self.cancel_multipart(object_key, &session.version_id).await;
                return Err(MxOperationError::N1(format!(
                    "failed to finalize multipart upload for '{object_key}': {error}"
                )));
            }
        };

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            if matches!(status.as_u16(), 408 | 429 | 500 | 502 | 503 | 504)
                && self
                    .finalized_version(object_key, &session.version_id)
                    .await
            {
                return Ok(());
            }
            self.cancel_multipart(object_key, &session.version_id).await;
            return Err(MxOperationError::N1(format!(
                "N1 multipart finalization failed for '{object_key}' with HTTP {status}: {text}"
            )));
        }

        Ok(())
    }

    async fn finalized_version(&self, object_key: &str, version_id: &str) -> bool {
        // A finalize response can be lost after publication. Only an exact
        // committed version match is success; an older object is not proof.
        let Ok(url) = self.object_url("/noa/v1/objects/meta", object_key) else {
            return false;
        };
        let Ok(response) = self
            .client
            .get(url)
            .bearer_auth(self.token)
            .timeout(Duration::from_secs(20))
            .send()
            .await
        else {
            return false;
        };
        if !response.status().is_success() {
            return false;
        }
        response
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|value| {
                value
                    .get("version_id")
                    .and_then(|v| v.as_str())
                    .map(|v| v == version_id)
            })
            .unwrap_or(false)
    }

    pub(crate) async fn verify_finalized_size(
        &self,
        object_key: &str,
        version_id: &str,
        size: u64,
    ) -> bool {
        let Ok(url) = self.object_url("/noa/v1/objects/meta", object_key) else {
            return false;
        };
        let Ok(response) = self
            .client
            .get(url)
            .bearer_auth(self.token)
            .timeout(Duration::from_secs(20))
            .send()
            .await
        else {
            return false;
        };
        if !response.status().is_success() {
            return false;
        }
        response
            .json::<serde_json::Value>()
            .await
            .ok()
            .is_some_and(|value| {
                value["version_id"].as_str() == Some(version_id)
                    && value["total_size_bytes"].as_u64() == Some(size)
                    && value["is_delete_marker"].as_bool() != Some(true)
            })
    }

    async fn upload_part(&self, version_id: &str, index: usize, part: &[u8]) -> Result<(), String> {
        // N1 v4 standard multipart slots acknowledge a completed part again.
        // Retry this exact session/index/body, never initialize another upload
        // or replay non-idempotent one-shot/finalize requests blindly.
        let body = axum::body::Bytes::copy_from_slice(part);
        for attempt in 0..4 {
            let response = self
                .client
                .put(format!(
                    "{}/noa/v1/upload/multipart/{version_id}/{index}",
                    self.base_url
                ))
                .bearer_auth(self.token)
                .timeout(Duration::from_secs(120))
                .header(CONTENT_TYPE, "application/octet-stream")
                .header(reqwest::header::CONTENT_LENGTH, body.len())
                .body(body.clone())
                .send()
                .await;
            let error = match response {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        return Ok(());
                    }
                    let text = response.text().await.unwrap_or_default();
                    let retryable = matches!(status.as_u16(), 408 | 429 | 500 | 502 | 503 | 504)
                        || (status == StatusCode::CONFLICT
                            && text.contains("Multipart part is already uploading"));
                    let error = format!("HTTP {status}: {text}");
                    if !retryable {
                        return Err(error);
                    }
                    error
                }
                Err(error) => error.to_string(),
            };
            if attempt == 3 {
                return Err(error);
            }
            let jitter = rand::random::<u16>() as u64 % 250;
            tokio::time::sleep(Duration::from_millis((250 << attempt) + jitter)).await;
        }
        unreachable!()
    }

    pub(super) async fn upload(
        &self,
        object_key: &str,
        mime_type: &str,
        bytes: Vec<u8>,
    ) -> Result<(), MxOperationError> {
        let (part_size, _) = multipart_plan(bytes.len(), self.part_size_mb);
        if bytes.len() > part_size {
            self.upload_multipart(object_key, mime_type, bytes).await
        } else {
            self.upload_one_shot(object_key, mime_type, bytes).await
        }
    }

    pub(super) async fn soft_delete(&self, object_key: &str) -> Result<(), MxOperationError> {
        let token = self.token;

        let response = self
            .client
            .delete(self.object_url("/noa/v1/objects", object_key)?)
            .timeout(Duration::from_secs(120))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|error| {
                MxOperationError::N1(format!(
                    "failed to soft-delete '{object_key}' from N1: {error}"
                ))
            })?;

        if response.status().is_success() {
            return check_mutation_response(response, "soft-delete", object_key).await;
        }
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }

        let status = response.status();
        let text = response.text().await.unwrap_or_default();

        Err(MxOperationError::N1(format!(
            "N1 soft-delete failed for '{object_key}' with HTTP {status}: {text}"
        )))
    }

    pub(super) async fn recover(&self, object_key: &str) -> Result<(), MxOperationError> {
        let token = self.token;

        let response = self
            .client
            .post(self.object_url("/noa/v1/objects/recover", object_key)?)
            .timeout(Duration::from_secs(120))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|error| {
                MxOperationError::N1(format!("failed to recover '{object_key}' in N1: {error}"))
            })?;

        let status = response.status();

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();

            return Err(MxOperationError::N1(format!(
                "N1 recovery failed for '{object_key}' with HTTP {status}: {text}"
            )));
        }

        check_mutation_response(response, "recovery", object_key).await
    }

    #[allow(dead_code)]
    pub(super) async fn rename(
        &self,
        source_key: &str,
        destination_key: &str,
    ) -> Result<(), MxOperationError> {
        if source_key == destination_key {
            return Ok(());
        }

        let base_url = &self.base_url;
        let token = self.token;
        validate_object_key(source_key)?;
        validate_object_key(destination_key)?;

        let response = self
            .client
            .post(format!("{base_url}/noa/v1/posix/rename"))
            .timeout(Duration::from_secs(120))
            .bearer_auth(token)
            .json(&json!({
                "source_path": source_key,
                "destination_path": destination_key,
            }))
            .send()
            .await
            .map_err(|error| {
                MxOperationError::N1(format!(
                    "failed to move N1 object '{source_key}' to '{destination_key}': {error}"
                ))
            })?;

        let status = response.status();

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();

            return Err(MxOperationError::N1(format!(
                "N1 move failed from '{source_key}' to '{destination_key}' with HTTP {status}: {text}"
            )));
        }

        check_mutation_response(response, "move", source_key).await
    }

    pub(super) async fn download(
        &self,
        object_key: &str,
        file_name: &str,
    ) -> Result<Vec<u8>, MxOperationError> {
        validate_object_key(object_key)?;
        let base_url = &self.base_url;
        let token = self.token;

        let response = self
            .client
            .get(format!("{base_url}/noa/v1/objects/download"))
            .bearer_auth(token)
            .query(&[("object_key", object_key), ("filename", file_name)])
            .send()
            .await
            .map_err(|error| {
                MxOperationError::N1(format!(
                    "failed to download '{object_key}' from N1: {error}"
                ))
            })?;

        let status = response.status();

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();

            return Err(MxOperationError::N1(format!(
                "N1 download failed for '{object_key}' with HTTP {status}: {text}"
            )));
        }

        let bytes = response.bytes().await.map_err(|error| {
            MxOperationError::N1(format!(
                "failed to read N1 download body for '{object_key}': {error}"
            ))
        })?;

        Ok(bytes.to_vec())
    }

    pub(super) async fn stream(
        &self,
        object_key: &str,
        range: Option<&str>,
    ) -> Result<reqwest::Response, MxOperationError> {
        let token = self.token;
        let mut request = self
            .client
            .get(self.object_url("/noa/v1/objects/stream", object_key)?)
            .bearer_auth(token);

        if let Some(range) = range.filter(|value| !value.trim().is_empty()) {
            request = request.header(reqwest::header::RANGE, range);
        }

        let response = request.send().await.map_err(|error| {
            MxOperationError::N1(format!("failed to stream '{object_key}' from N1: {error}"))
        })?;
        let status = response.status();

        if status.is_success() || status == StatusCode::RANGE_NOT_SATISFIABLE {
            return Ok(response);
        }

        let text = response.text().await.unwrap_or_default();
        Err(MxOperationError::N1(format!(
            "N1 stream failed for '{object_key}' with HTTP {status}: {text}"
        )))
    }
}

fn validate_object_key(object_key: &str) -> Result<(), MxOperationError> {
    // MX owns exact attachment keys, never root/prefix mutation requests.
    if object_key.trim().is_empty()
        || object_key.chars().any(char::is_control)
        || object_key
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
    {
        return Err(MxOperationError::N1("Invalid N1 object key".into()));
    }
    Ok(())
}

async fn check_mutation_response(
    response: reqwest::Response,
    operation: &str,
    object_key: &str,
) -> Result<(), MxOperationError> {
    let report: serde_json::Value = response.json().await.map_err(|error| {
        MxOperationError::N1(format!(
            "Invalid N1 {operation} result for '{object_key}': {error}"
        ))
    })?;
    for field in ["conflicted_paths", "failed_paths"] {
        // POSIX reports omit empty arrays; object mutation reports include
        // them. Both are valid v4 response shapes.
        let Some(value) = report.get(field) else {
            continue;
        };
        let paths = value.as_array().ok_or_else(|| {
            MxOperationError::N1(format!(
                "N1 {operation} result has invalid '{field}' for '{object_key}'"
            ))
        })?;
        if !paths.is_empty() {
            return Err(MxOperationError::N1(format!(
                "N1 {operation} did not complete for '{object_key}': {field}={paths:?}"
            )));
        }
    }
    let affected = report
        .get("affected")
        .or_else(|| report.get("affected_paths"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if affected == 0 {
        return Err(MxOperationError::N1(format!(
            "N1 {operation} reported no completed objects for '{object_key}'"
        )));
    }
    Ok(())
}
