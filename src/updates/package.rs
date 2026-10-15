use super::{
    UpdateConfig,
    store::{self, Result, Staged},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

pub const FORMAT: u32 = 1;
const MAX_METADATA: u64 = 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedRelease {
    pub key_id: String,
    pub payload: String,
    pub signature: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: u32,
    pub product: String,
    pub version: String,
    pub channel: String,
    pub target: String,
    pub minimum_updater: u32,
    pub files: Vec<FileInfo>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileInfo {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendIdentity {
    pub version: String,
    pub binary_sha256: String,
    pub index_sha256: String,
}

pub fn target() -> String {
    format!(
        "{}-unknown-linux-{}",
        std::env::consts::ARCH,
        if cfg!(target_env = "musl") {
            "musl"
        } else {
            "gnu"
        }
    )
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn hex_decode(value: &str) -> Result<Vec<u8>> {
    if value.len() % 2 != 0 || !value.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid hexadecimal value.".into());
    }
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}
pub fn hash_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hex(&hash.finalize()))
}
pub fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() < 1024
        && !path.contains(['\\', ':', '\0'])
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
        && (matches!(
            path,
            "litiaina-mx" | "LICENSE" | "README.md" | "CHANGELOG.md"
        ) || path.starts_with("frontend/dist/")
            || path.starts_with("docs/")
            || path.starts_with("assets/"))
}
pub fn verify(config: &UpdateConfig, signed: &SignedRelease) -> Result<Manifest> {
    if signed.payload.len() > MAX_METADATA as usize || signed.signature.len() > 128 {
        return Err("Signed release metadata exceeds its safety budget.".into());
    }
    let key = config
        .trusted_keys
        .get(&signed.key_id)
        .ok_or("The release signing key is not trusted by this deployment.")?;
    let payload = STANDARD
        .decode(&signed.payload)
        .map_err(|_| "Invalid release payload encoding.")?;
    let signature = STANDARD
        .decode(&signed.signature)
        .map_err(|_| "Invalid release signature encoding.")?;
    UnparsedPublicKey::new(&ED25519, hex_decode(key)?)
        .verify(&payload, &signature)
        .map_err(|_| "Release signature verification failed. Nothing was installed.")?;
    let manifest: Manifest =
        serde_json::from_slice(&payload).map_err(|e| format!("Invalid signed release: {e}"))?;
    if manifest.format != FORMAT
        || manifest.product != "litiaina-mx"
        || manifest.minimum_updater > FORMAT
        || manifest.target != target()
    {
        return Err(
            "The release format, product or CPU/platform does not match this updater.".into(),
        );
    }
    let version =
        semver::Version::parse(&manifest.version).map_err(|_| "Invalid signed release version.")?;
    if version.major != 1
        || !version.build.is_empty()
        || (version.pre.is_empty() && manifest.channel != "stable")
        || (!version.pre.is_empty()
            && (manifest.channel != "rc" || !version.pre.as_str().starts_with("rc.")))
        || (config.channel == "stable" && !version.pre.is_empty())
    {
        return Err("Release is outside the permitted 1.x stable/RC channel. Historical 4.x builds are not update targets.".into());
    }
    let mut paths = BTreeSet::new();
    let mut total = 0u64;
    if manifest.files.is_empty() || manifest.files.len() > 10000 {
        return Err("Invalid release file count.".into());
    }
    for item in &manifest.files {
        if !safe_path(&item.path)
            || !paths.insert(item.path.clone())
            || item.sha256.len() != 64
            || hex_decode(&item.sha256).is_err()
        {
            return Err("Unsafe/duplicate package path or invalid checksum.".into());
        }
        total = total
            .checked_add(item.size)
            .ok_or("Release size overflow.")?;
    }
    if total > config.max_unpacked_bytes {
        return Err("Release exceeds this deployment's unpacked package safety budget.".into());
    }
    for required in [
        "litiaina-mx",
        "frontend/dist/index.html",
        "frontend/dist/mx-release.json",
        "LICENSE",
    ] {
        if !paths.contains(required) {
            return Err(format!("Release is missing {required}."));
        }
    }
    Ok(manifest)
}
pub fn newer(candidate: &str, current: &str) -> Result<bool> {
    let candidate = semver::Version::parse(candidate).map_err(|_| "Invalid candidate version.")?;
    let current = semver::Version::parse(current).map_err(|_| "Invalid installed version.")?;
    Ok(candidate > current)
}
pub fn signed_id(signed: &SignedRelease) -> String {
    hex(&Sha256::digest(signed.payload.as_bytes()))
}
pub fn available_space(path: &Path) -> Result<u64> {
    #[cfg(unix)]
    {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        let path =
            CString::new(path.as_os_str().as_bytes()).map_err(|_| "Invalid storage path.")?;
        let mut value = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        if unsafe { libc::statvfs(path.as_ptr(), value.as_mut_ptr()) } != 0 {
            return Err("Cannot check free space for the update.".into());
        }
        let value = unsafe { value.assume_init() };
        Ok((value.f_bavail as u64).saturating_mul(value.f_frsize as u64))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err("Update installation requires Linux/systemd.".into())
    }
}
pub fn check_space(config: &UpdateConfig, required: u64) -> Result<()> {
    if available_space(&config.root)? < required.saturating_add(64 * 1024 * 1024) {
        Err("Insufficient free space for staging, a verified backup and rollback. No running files were replaced.".into())
    } else {
        Ok(())
    }
}
pub fn envelope(path: &Path) -> Result<SignedRelease> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| format!("Invalid .mxupdate package: {e}"))?;
    let mut entry = zip
        .by_name("release.json")
        .map_err(|_| "Package has no signed release.json.")?;
    if entry.size() > MAX_METADATA {
        return Err("Release metadata is too large.".into());
    }
    let mut data = Vec::new();
    entry
        .by_ref()
        .take(MAX_METADATA + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() > MAX_METADATA as usize {
        return Err("Release metadata is too large.".into());
    }
    serde_json::from_slice(&data).map_err(|e| e.to_string())
}
pub fn verify_tree(config: &UpdateConfig, staged: &Staged) -> Result<Manifest> {
    let manifest = verify(config, &staged.signed)?;
    if manifest.version != staged.version {
        return Err("Staged version does not match its signed manifest.".into());
    }
    let directory = store::slot(config, &staged.slot)?;
    if fs::symlink_metadata(&directory)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Release slot must not be a symlink.".into());
    }
    for item in &manifest.files {
        let path = directory.join(&item.path);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|e| format!("Missing staged file {}: {e}", item.path))?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() != item.size
            || hash_file(&path)? != item.sha256
        {
            return Err(format!(
                "Staged file changed or is incomplete: {}. Restage the signed release.",
                item.path
            ));
        }
        if !fs::canonicalize(&path)
            .map_err(|e| e.to_string())?
            .starts_with(&directory)
        {
            return Err("A staged file escapes its immutable slot.".into());
        }
    }
    let marker: FrontendIdentity = store::read(&directory.join("frontend/dist/mx-release.json"))?;
    if marker.version != manifest.version
        || manifest
            .files
            .iter()
            .find(|f| f.path == "litiaina-mx")
            .is_none_or(|f| f.sha256 != marker.binary_sha256)
        || manifest
            .files
            .iter()
            .find(|f| f.path == "frontend/dist/index.html")
            .is_none_or(|f| f.sha256 != marker.index_sha256)
    {
        return Err("Frontend/backend release identities do not match.".into());
    }
    Ok(manifest)
}
pub fn stage(
    config: &UpdateConfig,
    path: &Path,
    expected: Option<&SignedRelease>,
) -> Result<Staged> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > config.max_package_bytes {
        return Err("Package exceeds the configured update download safety budget.".into());
    }
    let signed = envelope(path)?;
    if expected.is_some_and(|expected| {
        signed_id(expected) != signed_id(&signed)
            || expected.signature != signed.signature
            || expected.key_id != signed.key_id
    }) {
        return Err("Downloaded package does not match the trusted release offer.".into());
    }
    let manifest = verify(config, &signed)?;
    let total = manifest.files.iter().map(|f| f.size).sum::<u64>();
    check_space(config, total)?;
    let slot = format!("{}-{}", manifest.version, &signed_id(&signed)[..16]);
    let destination = store::slot(config, &slot)?;
    let staged = Staged {
        version: manifest.version.clone(),
        slot,
        signed,
    };
    if destination.exists() {
        verify_tree(config, &staged)?;
        return Ok(staged);
    }
    let temporary = config
        .root
        .join("slots")
        .join(format!("staging-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&temporary).map_err(|e| e.to_string())?;
    // This exact private, freshly generated directory is the only recursive
    // cleanup target. No path chosen by an archive can become a deletion root.
    struct PartialSlot(PathBuf);
    impl Drop for PartialSlot {
        fn drop(&mut self) {
            if self.0.exists() {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
    }
    let _partial = PartialSlot(temporary.clone());
    let expected: BTreeMap<_, _> = manifest
        .files
        .iter()
        .map(|item| (item.path.as_str(), item))
        .collect();
    let mut archive = zip::ZipArchive::new(File::open(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if archive.len() != expected.len() + 1 {
        return Err("Package contains missing or unexpected files.".into());
    }
    let mut seen = BTreeSet::new();
    let mut done = 0u64;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        if !seen.insert(name.clone()) {
            return Err("Duplicate ZIP entries are not allowed.".into());
        }
        if name == "release.json" {
            continue;
        }
        let item = expected
            .get(name.as_str())
            .ok_or("Package contains an unsigned or unsafe path.")?;
        if entry.is_dir()
            || entry
                .unix_mode()
                .is_some_and(|mode| !matches!(mode & 0o170000, 0 | 0o100000))
            || entry.size() != item.size
        {
            return Err(format!("Invalid package file type or length: {name}"));
        }
        let path = temporary.join(&name);
        fs::create_dir_all(path.parent().ok_or("No file parent.")?).map_err(|e| e.to_string())?;
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut written = 0u64;
        let mut buffer = [0; 65536];
        loop {
            let count = entry.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            written = written
                .checked_add(count as u64)
                .ok_or("File size overflow.")?;
            if written > item.size {
                return Err("Package expanded beyond its signed file size.".into());
            }
            hash.update(&buffer[..count]);
            output
                .write_all(&buffer[..count])
                .map_err(|e| e.to_string())?;
        }
        if written != item.size || hex(&hash.finalize()) != item.sha256 {
            return Err(format!("Checksum mismatch: {name}. Nothing was activated."));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            output
                .set_permissions(fs::Permissions::from_mode(if name == "litiaina-mx" {
                    0o755
                } else {
                    0o644
                }))
                .map_err(|e| e.to_string())?;
        }
        output.sync_all().map_err(|e| e.to_string())?;
        done += written;
        store::progress(
            config,
            "verifying",
            "Verifying signed package files.",
            done,
            total,
        )?;
    }
    // Sync every directory, not only file contents, before publishing a slot.
    fn sync_tree(path: &Path) -> Result<()> {
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.is_dir() {
                sync_tree(&path)?;
            }
        }
        store::sync_dir(path)
    }
    sync_tree(&temporary)?;
    fs::rename(&temporary, &destination).map_err(|e| e.to_string())?;
    store::sync_dir(&config.root.join("slots"))?;
    verify_tree(config, &staged)?;
    Ok(staged)
}
pub async fn executable_version(path: &Path) -> Result<String> {
    use tokio::io::AsyncReadExt;
    let executable =
        fs::canonicalize(path).map_err(|e| format!("Cannot identify candidate executable: {e}"))?;
    let directory = std::env::temp_dir().join(format!("mx-version-probe-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&directory).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    struct ProbeDirectory(PathBuf);
    impl Drop for ProbeDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _directory = ProbeDirectory(directory.clone());
    // A signed but buggy release might ignore --version. Never let such a
    // probe open the real working directory/configuration or inherit secrets.
    let mut child = tokio::process::Command::new(executable)
        .arg("--version")
        .current_dir(&directory)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("TMPDIR", &directory)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Cannot execute candidate: {e}"))?;
    let stdout = child.stdout.take().ok_or("Missing version stdout.")?;
    let stderr = child.stderr.take().ok_or("Missing version stderr.")?;
    let (status, stdout, stderr) = tokio::time::timeout(Duration::from_secs(10), async {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut stdout = stdout.take(4097);
        let mut stderr = stderr.take(1025);
        tokio::try_join!(stdout.read_to_end(&mut out), stderr.read_to_end(&mut err))?;
        let status = child.wait().await?;
        Ok::<_, std::io::Error>((status, out, err))
    })
    .await
    .map_err(|_| "Candidate --version timed out.")?
    .map_err(|e| e.to_string())?;
    if !status.success() || !stderr.is_empty() || stdout.len() > 4096 {
        return Err("Candidate --version did not exit cleanly.".into());
    }
    let output = String::from_utf8(stdout).map_err(|_| "Invalid version output.")?;
    let version = output
        .trim()
        .strip_prefix("litiaina-mx ")
        .ok_or("Candidate has no supported MX version command.")?;
    semver::Version::parse(version).map_err(|_| "Invalid executable version.")?;
    Ok(version.into())
}
pub fn keygen(path: &Path) -> Result<()> {
    let random = ring::rand::SystemRandom::new();
    let pkcs8 =
        Ed25519KeyPair::generate_pkcs8(&random).map_err(|_| "Could not generate signing key.")?;
    let key =
        Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).map_err(|_| "Could not load signing key.")?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| {
        format!("Cannot create new private key (existing files are never replaced): {e}")
    })?;
    file.write_all(pkcs8.as_ref())
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    store::sync_dir(path.parent().unwrap_or(Path::new(".")))?;
    let public = hex(key.public_key().as_ref());
    println!(
        "Private signing key created at {}. Keep it OFF the MX server.\ntrusted_keys: {{\"{}\": \"{}\"}}",
        path.display(),
        &public[..16],
        public
    );
    Ok(())
}
pub async fn build(source: &Path, key_path: &Path, output: &Path, channel: &str) -> Result<()> {
    let metadata_path = output.with_extension("json");
    if output.exists() || metadata_path.exists() {
        return Err("Package output already exists; published candidates are immutable.".into());
    }
    if fs::metadata(key_path).map_err(|e| e.to_string())?.len() > 4096 {
        return Err("Signing key is not a small Ed25519 PKCS#8 key file.".into());
    }
    let key = Ed25519KeyPair::from_pkcs8(&fs::read(key_path).map_err(|e| e.to_string())?)
        .map_err(|_| "Invalid Ed25519 PKCS#8 signing key.")?;
    let version = executable_version(&source.join("litiaina-mx")).await?;
    fn walk(root: &Path, path: &Path, files: &mut Vec<FileInfo>) -> Result<()> {
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            let path = entry.path();
            if kind.is_symlink() {
                return Err("Release inputs must not contain symlinks.".into());
            }
            if kind.is_dir() {
                walk(root, &path, files)?;
                continue;
            }
            let name = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_str()
                .ok_or("Non-UTF8 release path.")?
                .replace('\\', "/");
            if name == "frontend/dist/mx-release.json" {
                continue;
            }
            if !kind.is_file() || !safe_path(&name) {
                return Err(format!(
                    "Refusing release input {name}: never package mx.env, config, SQLite, TLS or private keys."
                ));
            }
            files.push(FileInfo {
                path: name,
                size: entry.metadata().map_err(|e| e.to_string())?.len(),
                sha256: hash_file(&path)?,
            });
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(source, source, &mut files)?;
    let identity = FrontendIdentity {
        version: version.clone(),
        binary_sha256: files
            .iter()
            .find(|f| f.path == "litiaina-mx")
            .ok_or("Missing executable.")?
            .sha256
            .clone(),
        index_sha256: files
            .iter()
            .find(|f| f.path == "frontend/dist/index.html")
            .ok_or("Missing frontend.")?
            .sha256
            .clone(),
    };
    let marker = serde_json::to_vec_pretty(&identity).map_err(|e| e.to_string())?;
    files.push(FileInfo {
        path: "frontend/dist/mx-release.json".into(),
        size: marker.len() as u64,
        sha256: hex(&Sha256::digest(&marker)),
    });
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = Manifest {
        format: FORMAT,
        product: "litiaina-mx".into(),
        version: version.clone(),
        channel: channel.into(),
        target: target(),
        minimum_updater: FORMAT,
        files,
    };
    let payload = serde_json::to_vec(&manifest).map_err(|e| e.to_string())?;
    let public = hex(key.public_key().as_ref());
    let signed = SignedRelease {
        key_id: public[..16].into(),
        payload: STANDARD.encode(&payload),
        signature: STANDARD.encode(key.sign(&payload).as_ref()),
    };
    let config = UpdateConfig {
        channel: "rc".into(),
        trusted_keys: BTreeMap::from([(signed.key_id.clone(), public)]),
        ..Default::default()
    };
    verify(&config, &signed)?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("release.json", options)
        .map_err(|e| e.to_string())?;
    zip.write_all(&serde_json::to_vec(&signed).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    for item in &manifest.files {
        zip.start_file(
            &item.path,
            options.unix_permissions(if item.path == "litiaina-mx" {
                0o755
            } else {
                0o644
            }),
        )
        .map_err(|e| e.to_string())?;
        if item.path == "frontend/dist/mx-release.json" {
            zip.write_all(&marker).map_err(|e| e.to_string())?;
        } else {
            let mut input = File::open(source.join(&item.path)).map_err(|e| e.to_string())?;
            let mut digest = Sha256::new();
            let mut written = 0u64;
            let mut buffer = [0; 65536];
            loop {
                let count = input.read(&mut buffer).map_err(|e| e.to_string())?;
                if count == 0 {
                    break;
                }
                written += count as u64;
                digest.update(&buffer[..count]);
                zip.write_all(&buffer[..count]).map_err(|e| e.to_string())?;
            }
            if written != item.size || hex(&digest.finalize()) != item.sha256 {
                return Err(format!(
                    "Release input changed during packaging: {}. Do not publish this output.",
                    item.path
                ));
            }
        }
    }
    zip.finish()
        .map_err(|e| e.to_string())?
        .sync_all()
        .map_err(|e| e.to_string())?;
    let mut metadata = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&metadata_path)
        .map_err(|e| e.to_string())?;
    metadata
        .write_all(&serde_json::to_vec_pretty(&signed).map_err(|e| e.to_string())?)
        .and_then(|_| metadata.sync_all())
        .map_err(|e| e.to_string())?;
    store::sync_dir(output.parent().unwrap_or(Path::new(".")))?;
    println!(
        "Packaged MX {version} ({}) at {} and {}",
        target(),
        output.display(),
        metadata_path.display()
    );
    Ok(())
}
