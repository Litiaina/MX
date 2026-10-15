use super::{
    config::UpdateConfig,
    package::{self, FileInfo, Manifest, SignedRelease},
    store::{self, Policy},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use ring::signature::{Ed25519KeyPair, KeyPair};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Write, path::PathBuf};

fn fixture() -> (UpdateConfig, Ed25519KeyPair, PathBuf) {
    let root = std::env::temp_dir().join(format!("mx-update-unit-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let key = Ed25519KeyPair::from_seed_unchecked(&[29; 32]).unwrap();
    let config = UpdateConfig {
        root: root.clone(),
        deployment_dir: root.with_extension("deployment"),
        trusted_keys: BTreeMap::from([("test".into(), package::hex(key.public_key().as_ref()))]),
        ..Default::default()
    };
    store::initialize(&config).unwrap();
    (config, key, root)
}
fn manifest() -> Manifest {
    Manifest {
        format: 1,
        product: "litiaina-mx".into(),
        version: "1.0.0-rc.2".into(),
        channel: "rc".into(),
        target: package::target(),
        minimum_updater: 1,
        files: [
            "litiaina-mx",
            "LICENSE",
            "frontend/dist/index.html",
            "frontend/dist/mx-release.json",
        ]
        .into_iter()
        .map(|path| FileInfo {
            path: path.into(),
            size: 1,
            sha256: package::hex(&Sha256::digest(b"x")),
        })
        .collect(),
    }
}
fn sign(key: &Ed25519KeyPair, manifest: &Manifest) -> SignedRelease {
    let payload = serde_json::to_vec(manifest).unwrap();
    SignedRelease {
        key_id: "test".into(),
        payload: STANDARD.encode(&payload),
        signature: STANDARD.encode(key.sign(&payload).as_ref()),
    }
}

#[test]
fn signed_metadata_rejects_tampering_untrusted_keys_and_wrong_platform() {
    let (config, key, _) = fixture();
    let mut signed = sign(&key, &manifest());
    assert!(package::verify(&config, &signed).is_ok());
    signed.payload = STANDARD.encode(b"{}");
    assert!(
        package::verify(&config, &signed)
            .unwrap_err()
            .contains("signature")
    );
    let mut signed = sign(&key, &manifest());
    signed.key_id = "unknown".into();
    assert!(package::verify(&config, &signed).is_err());
    let mut m = manifest();
    m.target = "unrelated-platform".into();
    assert!(package::verify(&config, &sign(&key, &m)).is_err());
}
#[test]
fn channel_version_and_historical_release_line_are_enforced() {
    let (mut config, key, _) = fixture();
    config.channel = "stable".into();
    assert!(package::verify(&config, &sign(&key, &manifest())).is_err());
    let mut m = manifest();
    m.version = "1.0.0".into();
    m.channel = "stable".into();
    assert!(package::verify(&config, &sign(&key, &m)).is_ok());
    for version in ["4.1.2", "2.0.0", "1.0.0+retag", "garbage", "1.0.0-beta.2"] {
        m.version = version.into();
        assert!(
            package::verify(&config, &sign(&key, &m)).is_err(),
            "{version}"
        );
    }
    assert!(package::newer("1.0.0-rc.2", "1.0.0-rc.1").unwrap());
    assert!(package::newer("1.0.0", "1.0.0-rc.99").unwrap());
    assert!(!package::newer("1.0.0-rc.1", "1.0.0-rc.2").unwrap());
    assert!(!package::newer("1.0.0", "1.0.0").unwrap());
}
#[test]
fn signed_paths_must_be_unique_regular_release_files_with_bounded_sizes() {
    let (config, key, _) = fixture();
    for path in [
        "../mx.env",
        "/etc/passwd",
        "frontend/dist/../../mx.db",
        "frontend\\dist\\index.html",
        "mx.config",
        "certificates/key.pem",
        "frontend/dist//oops",
        "docs/./bad",
        "frontend/dist/C:bad",
    ] {
        let mut m = manifest();
        m.files.push(FileInfo {
            path: path.into(),
            size: 0,
            sha256: "0".repeat(64),
        });
        assert!(package::verify(&config, &sign(&key, &m)).is_err(), "{path}");
    }
    let mut m = manifest();
    m.files.push(m.files[0].clone());
    assert!(package::verify(&config, &sign(&key, &m)).is_err());
    let mut m = manifest();
    m.files[0].size = u64::MAX;
    assert!(package::verify(&config, &sign(&key, &m)).is_err());
    let mut m = manifest();
    m.files.retain(|f| f.path != "LICENSE");
    assert!(package::verify(&config, &sign(&key, &m)).is_err());
}
#[test]
fn maintenance_windows_handle_midnight_and_manila_offset_without_implicit_all_day() {
    let p = Policy {
        mode: "automatic".into(),
        start_hour: 2,
        end_hour: 3,
        utc_offset_minutes: 480,
    };
    p.validate().unwrap();
    assert!(p.contains(18, 0));
    assert!(p.contains(18, 59));
    assert!(!p.contains(19, 0));
    assert!(!p.contains(17, 59));
    let p = Policy {
        start_hour: 23,
        end_hour: 2,
        ..p
    };
    assert!(p.contains(15, 0));
    assert!(p.contains(17, 59));
    assert!(!p.contains(18, 0));
    assert!(
        Policy {
            start_hour: 3,
            end_hour: 3,
            ..p.clone()
        }
        .validate()
        .is_err()
    );
    assert!(
        Policy {
            mode: "unexpected".into(),
            ..p
        }
        .validate()
        .is_err()
    );
}
#[test]
fn queue_is_durable_idempotent_payload_bound_and_serialized() {
    let (config, _, _) = fixture();
    let request = store::Request {
        uid: uuid::Uuid::new_v4().to_string(),
        action: "check".into(),
        version: None,
        actor_uid: "test".into(),
        created_at: store::now(),
    };
    store::queue(&config, request.clone()).unwrap();
    store::queue(&config, request.clone()).unwrap();
    let mut changed = request.clone();
    changed.action = "stage".into();
    assert!(store::queue(&config, changed).is_err());
    let mut other = request.clone();
    other.uid = uuid::Uuid::new_v4().to_string();
    assert!(store::queue(&config, other).is_err());
    assert_eq!(
        store::read::<store::Request>(&config.root.join("request.json"))
            .unwrap()
            .uid,
        request.uid
    );
    store::terminal(&config, "completed", "done", None).unwrap();
    store::terminal(&config, "completed", "done", None).unwrap();
    assert_eq!(store::state(&config).unwrap().history.len(), 1);
    assert!(store::slot(&config, "../outside").is_err());
}
#[test]
fn config_is_opt_in_and_rejects_unbounded_units_urls_and_trust() {
    let (mut config, _, _) = fixture();
    config.validate().unwrap();
    for unit in ["--all", "mx.service;touch bad", "../mx.service"] {
        config.service = unit.into();
        assert!(config.validate().is_err());
    }
    config.service = "mx.service".into();
    assert!(
        config
            .validate_url("http://127.0.0.1:1234/releases")
            .is_err()
    );
    assert!(
        config
            .validate_url("https://attacker.invalid/package")
            .is_err()
    );
    assert!(config.validate_url("file:///etc/passwd").is_err());
    config.feed_url = Some("http://127.0.0.1:1234/releases".into());
    config.allow_loopback_http = true;
    assert!(config.validate_url("http://127.0.0.1:1234/file").is_ok());
    assert!(config.validate_url("http://127.0.0.1:9999/file").is_err());
    config.auto_install = true;
    assert!(config.validate().is_err());
    config.maintenance_start_utc_hour = Some(2);
    config.maintenance_end_utc_hour = Some(3);
    assert!(config.validate().is_ok());
}
#[test]
fn lock_is_exclusive_across_independent_handles() {
    let (config, _, _) = fixture();
    let path = config.root.join("exclusive.lock");
    let first = store::Lock::acquire(&path, false).unwrap();
    assert!(store::Lock::acquire(&path, false).is_err());
    drop(first);
    assert!(store::Lock::acquire(&path, false).is_ok());
}

#[test]
fn failed_activation_remains_blocked_after_ui_history_is_trimmed_but_a_manual_success_clears_it() {
    let (config, _, _) = fixture();
    let request = |action: &str, version: Option<&str>| store::Request {
        uid: uuid::Uuid::new_v4().to_string(),
        action: action.into(),
        version: version.map(String::from),
        actor_uid: "test".into(),
        created_at: store::now(),
    };
    store::queue(&config, request("install", Some("1.0.0-rc.2"))).unwrap();
    store::terminal(
        &config,
        "rolled_back",
        "failed readiness",
        Some("failure".into()),
    )
    .unwrap();
    store::remove(&config.root.join("request.json")).unwrap();
    for _ in 0..35 {
        store::queue(&config, request("check", None)).unwrap();
        store::terminal(&config, "completed", "checked", None).unwrap();
        store::remove(&config.root.join("request.json")).unwrap();
    }
    let state = store::state(&config).unwrap();
    assert_eq!(state.history.len(), 30);
    assert!(store::automatic_blocked(&state, "1.0.0-rc.2", store::now()));
    store::queue(&config, request("install", Some("1.0.0-rc.2"))).unwrap();
    store::terminal(&config, "completed", "operator retry succeeded", None).unwrap();
    assert!(!store::automatic_blocked(
        &store::state(&config).unwrap(),
        "1.0.0-rc.2",
        store::now()
    ));
}

#[test]
fn a_successful_restage_clears_transient_download_backoff_without_clearing_activation_blocks() {
    let (config, _, _) = fixture();
    let request = || store::Request {
        uid: uuid::Uuid::new_v4().to_string(),
        action: "stage".into(),
        version: Some("1.0.0-rc.2".into()),
        actor_uid: "test".into(),
        created_at: store::now(),
    };
    store::queue(&config, request()).unwrap();
    store::terminal(
        &config,
        "failed",
        "download interrupted",
        Some("network".into()),
    )
    .unwrap();
    assert!(store::automatic_blocked(
        &store::state(&config).unwrap(),
        "1.0.0-rc.2",
        store::now()
    ));
    assert!(!store::automatic_blocked(
        &store::state(&config).unwrap(),
        "1.0.0-rc.2",
        store::now() + 16 * 60 * 1000
    ));
    store::remove(&config.root.join("request.json")).unwrap();
    store::queue(&config, request()).unwrap();
    store::terminal(&config, "completed", "verified", None).unwrap();
    assert!(!store::automatic_blocked(
        &store::state(&config).unwrap(),
        "1.0.0-rc.2",
        store::now()
    ));
}
#[test]
fn readiness_proof_cannot_be_replayed_for_another_version_pid_database_or_activation() {
    let identity = super::health::Identity {
        version: "1.0.0-rc.2".into(),
        pid: 123,
        database_id: "database-id".into(),
        frontend_version: Some("1.0.0-rc.2".into()),
        update_id: Some("activation-id".into()),
        ready: true,
        managed: true,
    };
    let proof = super::health::proof("private-activation-secret", &identity).unwrap();
    assert!(super::health::verify(
        "private-activation-secret",
        &identity,
        &proof
    ));
    assert!(!super::health::verify("other-secret", &identity, &proof));
    for changed in [
        super::health::Identity {
            version: "1.0.0-rc.3".into(),
            ..identity.clone()
        },
        super::health::Identity {
            pid: 124,
            ..identity.clone()
        },
        super::health::Identity {
            database_id: "other-db".into(),
            ..identity.clone()
        },
        super::health::Identity {
            update_id: Some("other-activation".into()),
            ..identity.clone()
        },
        super::health::Identity {
            ready: false,
            ..identity.clone()
        },
        super::health::Identity {
            managed: false,
            ..identity.clone()
        },
    ] {
        assert!(!super::health::verify(
            "private-activation-secret",
            &changed,
            &proof
        ));
    }
}

#[test]
fn roots_must_not_escape_through_parent_components_or_contain_the_deployment() {
    let (config, _, _) = fixture();
    let mut bad = config.clone();
    bad.root = PathBuf::from("/srv/..");
    assert!(bad.validate().is_err());
    let mut bad = config;
    bad.deployment_dir = bad.root.join("deployment");
    assert!(bad.validate().is_err());
}

#[test]
#[cfg(unix)]
fn version_probes_never_run_inside_a_real_deployment_or_inherit_its_secrets() {
    use std::os::unix::fs::PermissionsExt;
    let (_config, _, root) = fixture();
    let script = root.join("version-fixture");
    // Refuse to write anywhere except the uniquely named probe sandbox. Even
    // a regressed test implementation must not create a file in the repo.
    fs::write(&script,"#!/bin/sh\ncase \"$PWD\" in */mx-version-probe-*) ;; *) exit 42;; esac\n[ -z \"$MX_UPDATE_CONFIG$JWT_SECRET$AUTH_KEYS$N1_MX_SECRET\" ] || exit 43\nmkdir probe-only-output\nprintf 'litiaina-mx 1.0.0-rc.2\\n'\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    assert_eq!(
        runtime
            .block_on(package::executable_version(&script))
            .unwrap(),
        "1.0.0-rc.2"
    );
    assert!(!root.join("probe-only-output").exists());
}

#[test]
fn zip_tampering_is_rejected_before_publishing_a_slot() {
    let (config, key, root) = fixture();
    let m = manifest();
    let signed = sign(&key, &m);
    let path = root.join("tampered.mxupdate");
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    zip.start_file("release.json", opts).unwrap();
    zip.write_all(&serde_json::to_vec(&signed).unwrap())
        .unwrap();
    for f in &m.files {
        zip.start_file(&f.path, opts).unwrap();
        zip.write_all(b"y").unwrap();
    }
    zip.finish().unwrap();
    assert!(
        package::stage(&config, &path, Some(&signed))
            .unwrap_err()
            .contains("Checksum")
    );
    assert!(fs::read_dir(root.join("slots")).unwrap().all(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("staging-")
    }));
}
