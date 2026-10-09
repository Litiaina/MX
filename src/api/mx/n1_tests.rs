use super::*;

#[test]
fn raw_fragment_compatibility_is_exact_and_does_not_rehash_identifiers() {
    let raw = "test-only-fragment";
    use sha2::{Digest, Sha256};
    let identifier: String = Sha256::digest(raw.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(fragment_identifier(raw, raw), identifier);
    assert_eq!(fragment_identifier(&identifier, raw), identifier);
    assert_eq!(
        fragment_identifier("another-fragment", raw),
        "another-fragment"
    );
}
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{HeaderMap, Response},
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

struct Step {
    method: &'static str,
    path: String,
    status: StatusCode,
    body: Vec<u8>,
    headers: Vec<(&'static str, String)>,
}

impl Step {
    fn new(
        method: &'static str,
        path: impl Into<String>,
        status: StatusCode,
        body: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            method,
            path: path.into(),
            status,
            body: body.into(),
            headers: vec![],
        }
    }
    fn json(method: &'static str, path: impl Into<String>, body: serde_json::Value) -> Self {
        let mut step = Self::new(
            method,
            path,
            StatusCode::OK,
            serde_json::to_vec(&body).unwrap(),
        );
        step.headers
            .push(("content-type", "application/json".into()));
        step
    }
}

struct Captured {
    uri: String,
    headers: HeaderMap,
    body: Vec<u8>,
}

#[derive(Default)]
struct MockState {
    steps: VecDeque<Step>,
    requests: Vec<Captured>,
}

struct Mock {
    base: String,
    state: Arc<Mutex<MockState>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Mock {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Mock {
    async fn start(steps: Vec<Step>) -> Self {
        let state = Arc::new(Mutex::new(MockState {
            steps: steps.into(),
            requests: vec![],
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let router = Router::new()
            .fallback(mock_request)
            .with_state(state.clone());
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Self { base, state, task }
    }

    fn requests(&self) -> std::sync::MutexGuard<'_, MockState> {
        let state = self.state.lock().unwrap();
        assert!(
            state.steps.is_empty(),
            "Not all expected N1 requests were made"
        );
        for request in &state.requests {
            assert_eq!(
                request.headers["authorization"],
                "Bearer test-fragment-token"
            );
        }
        state
    }
}

async fn mock_request(
    State(state): State<Arc<Mutex<MockState>>>,
    request: Request,
) -> Response<Body> {
    let (parts, body) = request.into_parts();
    let body = to_bytes(body, 110 * 1024 * 1024).await.unwrap().to_vec();
    let mut state = state.lock().unwrap();
    let step = state
        .steps
        .pop_front()
        .expect("Unexpected N1 request (including forbidden mkdir/stat)");
    assert_eq!(parts.method.as_str(), step.method);
    assert_eq!(parts.uri.path(), step.path);
    state.requests.push(Captured {
        uri: parts.uri.to_string(),
        headers: parts.headers,
        body,
    });
    let mut response = Response::builder().status(step.status);
    for (name, value) in step.headers {
        response = response.header(name, value);
    }
    response.body(Body::from(step.body)).unwrap()
}

fn client() -> Client {
    Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}

fn session() -> serde_json::Value {
    json!({"version_id":"version-1", "session_id":"session-1"})
}

#[tokio::test]
async fn drive_finalization_checks_only_exact_metadata_not_object_download() {
    let path = "/noa/v1/objects/meta/drive-test.mp4";
    let mock = Mock::start(vec![
        Step::json("GET", path, json!({"version_id":"expected","total_size_bytes":15_000_000_000u64})),
        Step::json("GET", path, json!({"version_id":"old","total_size_bytes":15_000_000_000u64})),
        Step::json("GET", path, json!({"version_id":"expected","total_size_bytes":1})),
        Step::json("GET", path, json!({"version_id":"expected","total_size_bytes":15_000_000_000u64,"is_delete_marker":true})),
    ]).await;
    let client = client();
    let storage = N1Storage::new(&client, &mock.base, "test-fragment-token", 16);
    assert!(
        storage
            .verify_finalized_size("drive-test.mp4", "expected", 15_000_000_000)
            .await
    );
    assert!(
        !storage
            .verify_finalized_size("drive-test.mp4", "expected", 15_000_000_000)
            .await
    );
    assert!(
        !storage
            .verify_finalized_size("drive-test.mp4", "expected", 15_000_000_000)
            .await
    );
    assert!(
        !storage
            .verify_finalized_size("drive-test.mp4", "expected", 15_000_000_000)
            .await
    );
    assert_eq!(mock.requests().requests.len(), 4);
}
fn success_report() -> serde_json::Value {
    json!({"affected_paths":1, "conflicted_paths":[], "failed_paths":[]})
}

#[test]
fn object_keys_are_encoded_without_losing_path_segments() {
    let client = client();
    let storage = N1Storage::new(&client, "https://n1.example/", "test", 16);
    let url = storage
        .object_url("/noa/v1/objects/stream", "records/患者/a #?%.txt")
        .unwrap();
    assert_eq!(
        url.path(),
        "/noa/v1/objects/stream/records/%E6%82%A3%E8%80%85/a%20%23%3F%25.txt"
    );
    assert!(url.query().is_none());
    assert!(url.fragment().is_none());
    for key in ["", "/", "a/", "a//b", "a/../b", "a/./b", "a\0b"] {
        assert!(
            storage.object_url("/noa/v1/objects", key).is_err(),
            "{key:?}"
        );
    }
}

#[test]
fn multipart_plan_bounds_sizes_and_counts() {
    const MIB: usize = 1024 * 1024;
    assert_eq!(multipart_plan(94 * MIB, 16), (16 * MIB, 6));
    assert_eq!(multipart_plan(16 * MIB, 16), (16 * MIB, 1));
    assert_eq!(multipart_plan(16 * MIB + 1, 16), (16 * MIB, 2));
    assert_eq!(multipart_plan(0, 0), (5 * MIB, 1));
    assert_eq!(multipart_plan(101 * MIB, 999), (100 * MIB, 2));
}

#[tokio::test]
async fn durable_parts_accept_plain_text_acknowledgements_and_reject_failures() {
    let mock = Mock::start(vec![
        Step::new(
            "PUT",
            "/noa/v1/upload/multipart/durable/version/0",
            StatusCode::OK,
            "Part uploaded",
        ),
        Step::new(
            "PUT",
            "/noa/v1/upload/multipart/durable/version/0",
            StatusCode::OK,
            "Part already uploaded",
        ),
        Step::new(
            "PUT",
            "/noa/v1/upload/multipart/durable/version/1",
            StatusCode::SERVICE_UNAVAILABLE,
            "Retry later",
        ),
    ])
    .await;
    let client = Client::new();
    let storage = N1Storage::new(&client, &mock.base, "test-fragment-token", 16);
    for _ in 0..2 {
        assert!(
            storage
                .durable_request(
                    reqwest::Method::PUT,
                    "__mx/drive/test/file.MP4",
                    "version",
                    "/0",
                    None,
                    Some(axum::body::Bytes::from_static(b"part")),
                    None
                )
                .await
                .is_ok()
        );
    }
    assert!(
        storage
            .durable_request(
                reqwest::Method::PUT,
                "__mx/drive/test/file.MP4",
                "version",
                "/1",
                None,
                Some(axum::body::Bytes::from_static(b"part")),
                None
            )
            .await
            .is_err()
    );
    assert_eq!(mock.requests().requests.len(), 3);
}

#[tokio::test]
async fn all_mx_storage_prefixes_upload_without_mkdir_or_stat() {
    let keys = [
        "records/HR/Attachments/test.txt",
        "__mx/collaboration/channel/message/file.txt",
        "__mx/profile-photos/user/avatar.png",
        "__mx/notification-sounds/user/sound.ogg",
        "__mx/branding/logo.png",
        "__mx/backups/database/2026/10/07/mx.db",
    ];
    let mock = Mock::start(
        keys.iter()
            .map(|key| {
                Step::new(
                    "POST",
                    format!("/noa/v1/upload/one-shot/{key}"),
                    StatusCode::CREATED,
                    b"{}".to_vec(),
                )
            })
            .collect(),
    )
    .await;
    let client = client();
    let storage = N1Storage::new(&client, &mock.base, "test-fragment-token", 5);
    for key in keys {
        storage
            .upload(key, "application/octet-stream", b"payload".to_vec())
            .await
            .unwrap();
    }
    assert_eq!(mock.requests().requests.len(), keys.len());
}

#[tokio::test]
async fn multipart_uses_v4_sizing_raw_bodies_and_finalize_query() {
    let mock = Mock::start(vec![
        Step::json(
            "POST",
            "/noa/v1/upload/multipart/init/videos/clip%20%23%3F.mp4",
            session(),
        ),
        Step::new(
            "PUT",
            "/noa/v1/upload/multipart/version-1/0",
            StatusCode::OK,
            b"Part uploaded".to_vec(),
        ),
        Step::new(
            "PUT",
            "/noa/v1/upload/multipart/version-1/1",
            StatusCode::OK,
            b"Part uploaded".to_vec(),
        ),
        Step::new(
            "PUT",
            "/noa/v1/upload/multipart/version-1/2",
            StatusCode::OK,
            b"Part uploaded".to_vec(),
        ),
        Step::json(
            "POST",
            "/noa/v1/upload/multipart/version-1/session-1/finalize",
            json!({"version_id":"version-1"}),
        ),
    ])
    .await;
    let client = client();
    let storage = N1Storage::new(&client, &mock.base, "test-fragment-token", 5);
    let bytes: Vec<u8> = (0..11 * 1024 * 1024 + 17)
        .map(|index| (index % 251) as u8)
        .collect();
    storage
        .upload("videos/clip #?.mp4", "video/mp4", bytes.clone())
        .await
        .unwrap();
    let state = mock.requests();
    let init: serde_json::Value = serde_json::from_slice(&state.requests[0].body).unwrap();
    assert_eq!(
        init,
        json!({"content_type":"video/mp4", "total_size_bytes":bytes.len(), "expected_parts":3, "part_size_bytes":5 * 1024 * 1024})
    );
    let finalize: serde_json::Value = serde_json::from_slice(&state.requests[4].body).unwrap();
    assert_eq!(init, finalize);
    assert!(
        state.requests[4]
            .uri
            .ends_with("?object_key=videos%2Fclip+%23%3F.mp4")
    );
    let mut reconstructed = vec![];
    for request in &state.requests[1..4] {
        assert!(request.body.len() <= 5 * 1024 * 1024);
        assert_eq!(
            request.headers["content-length"]
                .to_str()
                .unwrap()
                .parse::<usize>()
                .unwrap(),
            request.body.len()
        );
        assert_eq!(request.headers["content-type"], "application/octet-stream");
        reconstructed.extend_from_slice(&request.body);
    }
    assert_eq!(reconstructed, bytes);
}

#[tokio::test]
async fn transient_parts_retry_the_same_session_and_identical_bytes() {
    let mock = Mock::start(vec![
        Step::json("POST", "/noa/v1/upload/multipart/init/retry.bin", session()),
        Step::new(
            "PUT",
            "/noa/v1/upload/multipart/version-1/0",
            StatusCode::SERVICE_UNAVAILABLE,
            b"busy".to_vec(),
        ),
        // A lost acknowledgement can leave the first attempt still in flight.
        Step::new(
            "PUT",
            "/noa/v1/upload/multipart/version-1/0",
            StatusCode::CONFLICT,
            b"Multipart part is already uploading".to_vec(),
        ),
        Step::new(
            "PUT",
            "/noa/v1/upload/multipart/version-1/0",
            StatusCode::OK,
            vec![],
        ),
        Step::json(
            "POST",
            "/noa/v1/upload/multipart/version-1/session-1/finalize",
            json!({"version_id":"version-1"}),
        ),
    ])
    .await;
    let client = client();
    N1Storage::new(&client, &mock.base, "test-fragment-token", 5)
        .upload_multipart("retry.bin", "application/octet-stream", vec![17; 1024])
        .await
        .unwrap();
    let state = mock.requests();
    assert_eq!(state.requests.len(), 5);
    assert_eq!(state.requests[1].body, state.requests[2].body);
    assert_eq!(state.requests[2].body, state.requests[3].body);
}

#[tokio::test]
async fn permanent_part_outage_has_finite_retries_and_cancels_without_finalize() {
    let mut steps = vec![Step::json(
        "POST",
        "/noa/v1/upload/multipart/init/outage.bin",
        session(),
    )];
    for _ in 0..4 {
        steps.push(Step::new(
            "PUT",
            "/noa/v1/upload/multipart/version-1/0",
            StatusCode::BAD_GATEWAY,
            vec![],
        ));
    }
    steps.push(Step::new(
        "DELETE",
        "/noa/v1/upload/multipart/version-1",
        StatusCode::OK,
        vec![],
    ));
    let mock = Mock::start(steps).await;
    let client = client();
    assert!(
        N1Storage::new(&client, &mock.base, "test-fragment-token", 5)
            .upload_multipart("outage.bin", "application/octet-stream", vec![17; 1024])
            .await
            .is_err()
    );
    assert_eq!(mock.requests().requests.len(), 6);
}

#[tokio::test]
async fn failed_multipart_parts_cancel_without_finalizing() {
    let mock = Mock::start(vec![
        Step::json("POST", "/noa/v1/upload/multipart/init/large.bin", session()),
        Step::new(
            "PUT",
            "/noa/v1/upload/multipart/version-1/0",
            StatusCode::PAYLOAD_TOO_LARGE,
            b"part too large".to_vec(),
        ),
        Step::new(
            "DELETE",
            "/noa/v1/upload/multipart/version-1",
            StatusCode::OK,
            vec![],
        ),
    ])
    .await;
    let client = client();
    let storage = N1Storage::new(&client, &mock.base, "test-fragment-token", 5);
    assert!(
        storage
            .upload(
                "large.bin",
                "application/octet-stream",
                vec![0; 6 * 1024 * 1024]
            )
            .await
            .is_err()
    );
    assert!(
        mock.requests().requests[2]
            .uri
            .ends_with("?object_key=large.bin")
    );
}

#[tokio::test]
async fn lost_finalize_acknowledgement_checks_exact_committed_version() {
    for (committed_version, expected_success) in [("version-1", true), ("older-version", false)] {
        let mut steps = vec![
            Step::json("POST", "/noa/v1/upload/multipart/init/ack.bin", session()),
            Step::new(
                "PUT",
                "/noa/v1/upload/multipart/version-1/0",
                StatusCode::OK,
                vec![],
            ),
            Step::new(
                "POST",
                "/noa/v1/upload/multipart/version-1/session-1/finalize",
                StatusCode::BAD_GATEWAY,
                vec![],
            ),
            Step::json(
                "GET",
                "/noa/v1/objects/meta/ack.bin",
                json!({"version_id":committed_version}),
            ),
        ];
        if !expected_success {
            steps.push(Step::new(
                "DELETE",
                "/noa/v1/upload/multipart/version-1",
                StatusCode::OK,
                vec![],
            ));
        }
        let mock = Mock::start(steps).await;
        let client = client();
        assert_eq!(
            N1Storage::new(&client, &mock.base, "test-fragment-token", 5)
                .upload_multipart("ack.bin", "application/octet-stream", vec![42])
                .await
                .is_ok(),
            expected_success
        );
        assert_eq!(
            mock.requests().requests.len(),
            if expected_success { 4 } else { 5 }
        );
    }
}

#[tokio::test]
async fn failed_multipart_finalize_cancels_the_session() {
    let mock = Mock::start(vec![
        Step::json("POST", "/noa/v1/upload/multipart/init/file.bin", session()),
        Step::new(
            "PUT",
            "/noa/v1/upload/multipart/version-1/0",
            StatusCode::OK,
            vec![],
        ),
        Step::new(
            "POST",
            "/noa/v1/upload/multipart/version-1/session-1/finalize",
            StatusCode::CONFLICT,
            b"missing parts".to_vec(),
        ),
        Step::new(
            "DELETE",
            "/noa/v1/upload/multipart/version-1",
            StatusCode::OK,
            vec![],
        ),
    ])
    .await;
    let client = client();
    assert!(
        N1Storage::new(&client, &mock.base, "test-fragment-token", 5)
            .upload_multipart("file.bin", "application/octet-stream", vec![42])
            .await
            .is_err()
    );
    assert_eq!(mock.requests().requests.len(), 4);
}

#[tokio::test]
async fn downloads_and_partial_streams_preserve_v4_ranges() {
    let mut range = Step::new(
        "GET",
        "/noa/v1/objects/stream/docs/a%20%23%3F%25.bin",
        StatusCode::PARTIAL_CONTENT,
        b"2345".to_vec(),
    );
    range.headers = vec![
        ("content-range", "bytes 2-5/10".into()),
        ("accept-ranges", "bytes".into()),
    ];
    let mut invalid = Step::new(
        "GET",
        "/noa/v1/objects/stream/docs/a%20%23%3F%25.bin",
        StatusCode::RANGE_NOT_SATISFIABLE,
        vec![],
    );
    invalid.headers = vec![("content-range", "bytes */10".into())];
    let mock = Mock::start(vec![
        Step::new(
            "GET",
            "/noa/v1/objects/download",
            StatusCode::OK,
            b"0123456789".to_vec(),
        ),
        range,
        invalid,
    ])
    .await;
    let client = client();
    let storage = N1Storage::new(&client, &mock.base, "test-fragment-token", 5);
    let key = "docs/a #?%.bin";
    assert_eq!(
        storage.download(key, "file #?.bin").await.unwrap(),
        b"0123456789"
    );
    let response = storage.stream(key, Some("bytes=2-5")).await.unwrap();
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.headers()["content-range"], "bytes 2-5/10");
    assert_eq!(response.headers()["accept-ranges"], "bytes");
    assert_eq!(response.bytes().await.unwrap(), &b"2345"[..]);
    let response = storage.stream(key, Some("bytes=999-")).await.unwrap();
    assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(response.headers()["content-range"], "bytes */10");
    let state = mock.requests();
    assert!(
        state.requests[0]
            .uri
            .contains("object_key=docs%2Fa+%23%3F%25.bin")
    );
    assert!(state.requests[0].uri.contains("filename=file+%23%3F.bin"));
    assert_eq!(state.requests[1].headers["range"], "bytes=2-5");
}

#[tokio::test]
async fn mutation_reports_are_checked_and_missing_delete_is_idempotent() {
    let mock = Mock::start(vec![
        Step::json(
            "POST",
            "/noa/v1/posix/rename",
            json!({"operation":"rename", "kind":"file", "affected":1}),
        ),
        Step::json("DELETE", "/noa/v1/objects/docs/new.txt", success_report()),
        Step::json(
            "POST",
            "/noa/v1/objects/recover/docs/new.txt",
            success_report(),
        ),
        Step::new(
            "DELETE",
            "/noa/v1/objects/docs/missing.txt",
            StatusCode::NOT_FOUND,
            vec![],
        ),
        Step::json(
            "POST",
            "/noa/v1/posix/rename",
            json!({"conflicted_paths":["docs/new.txt"], "failed_paths":[]}),
        ),
        Step::json(
            "DELETE",
            "/noa/v1/objects/docs/new.txt",
            json!({"conflicted_paths":[], "failed_paths":["docs/new.txt"]}),
        ),
        Step::json(
            "POST",
            "/noa/v1/objects/recover/docs/new.txt",
            json!({"conflicted_paths":["docs/new.txt"], "failed_paths":[]}),
        ),
        Step::json("DELETE", "/noa/v1/objects/docs/new.txt", json!({})),
    ])
    .await;
    let client = client();
    let storage = N1Storage::new(&client, &mock.base, "test-fragment-token", 5);
    storage
        .rename("docs/old.txt", "docs/new.txt")
        .await
        .unwrap();
    storage.soft_delete("docs/new.txt").await.unwrap();
    storage.recover("docs/new.txt").await.unwrap();
    storage.soft_delete("docs/missing.txt").await.unwrap();
    assert!(
        storage
            .rename("docs/old.txt", "docs/new.txt")
            .await
            .is_err()
    );
    assert!(storage.soft_delete("docs/new.txt").await.is_err());
    assert!(storage.recover("docs/new.txt").await.is_err());
    assert!(storage.soft_delete("docs/new.txt").await.is_err());
    let state = mock.requests();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&state.requests[0].body).unwrap(),
        json!({"source_path":"docs/old.txt", "destination_path":"docs/new.txt"})
    );
}

#[tokio::test]
#[ignore = "Requires an isolated N1 v4 instance; writes only a unique test prefix"]
async fn real_n1_v4_round_trip() {
    let base = std::env::var("MX_TEST_N1_URL").expect("MX_TEST_N1_URL");
    let token = std::env::var("MX_TEST_N1_TOKEN").expect("MX_TEST_N1_TOKEN");
    let client = Client::builder()
        .no_proxy()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(120))
        .build()
        .unwrap();
    let storage = N1Storage::new(&client, &base, &token, 5);
    let prefix = format!("mx-v4-test/{}", uuid::Uuid::new_v4());
    let small = format!("{prefix}/preview #?%.txt");
    let moved = format!("{prefix}/renamed.txt");
    storage
        .upload(&small, "text/plain", b"0123456789".to_vec())
        .await
        .unwrap();
    assert_eq!(
        storage.download(&small, "preview.txt").await.unwrap(),
        b"0123456789"
    );
    let response = storage.stream(&small, Some("bytes=2-5")).await.unwrap();
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.headers()["content-range"], "bytes 2-5/10");
    assert_eq!(response.bytes().await.unwrap(), &b"2345"[..]);
    let invalid = storage.stream(&small, Some("bytes=999-")).await.unwrap();
    // N1's current ordinary-object reader ignores unsatisfiable ranges and
    // returns the full 200 body; exports use strict 416 handling. MX must
    // preserve either response without fabricating a partial response.
    match invalid.status() {
        StatusCode::OK => assert_eq!(invalid.bytes().await.unwrap(), &b"0123456789"[..]),
        StatusCode::RANGE_NOT_SATISFIABLE => {}
        status => panic!("Unexpected N1 range response: {status}"),
    }
    storage.rename(&small, &moved).await.unwrap();
    storage.soft_delete(&moved).await.unwrap();
    assert!(storage.download(&moved, "test.txt").await.is_err());
    storage.recover(&moved).await.unwrap();
    assert_eq!(
        storage.download(&moved, "test.txt").await.unwrap(),
        b"0123456789"
    );
    let large = format!("{prefix}/large.bin");
    let bytes: Vec<u8> = (0..11 * 1024 * 1024 + 17)
        .map(|index| (index % 251) as u8)
        .collect();
    storage
        .upload(&large, "application/octet-stream", bytes.clone())
        .await
        .unwrap();
    assert_eq!(storage.download(&large, "large.bin").await.unwrap(), bytes);
    storage.soft_delete(&large).await.unwrap();
    storage.soft_delete(&moved).await.unwrap();

    let paths = [
        "records",
        "__mx/collaboration",
        "__mx/profile-photos",
        "__mx/notification-sounds",
        "__mx/branding",
        "__mx/backups/database",
    ];
    // Distinct keys share the same authenticated transport concurrently, with
    // no parent-directory creation and no writes to any existing MX object.
    let keys: Vec<_> = paths
        .iter()
        .map(|path| format!("{path}/{prefix}/file.bin"))
        .collect();
    let results = futures::future::join_all(
        keys.iter()
            .map(|key| storage.upload(key, "application/octet-stream", b"test-payload".to_vec())),
    )
    .await;
    for result in results {
        result.unwrap();
    }
    for key in keys {
        assert_eq!(
            storage.download(&key, "file.bin").await.unwrap(),
            b"test-payload"
        );
        storage.soft_delete(&key).await.unwrap();
    }
}
