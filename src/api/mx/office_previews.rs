//! Optional server-side PDF previews, isolated from browser Office editing.
//! One bounded converter; identical immutable inputs share a result. A failed
//! conversion is cooled down so browser retries cannot start a process storm.
use super::model::FileAttachment;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};
use tokio::sync::{Semaphore, watch};
use uuid::Uuid;

const MAX_INPUT: usize = 16 * 1024 * 1024;
const MAX_EXPANDED: u64 = 32 * 1024 * 1024;
const MAX_PDF: u64 = 32 * 1024 * 1024;
const CONVERSION_TIMEOUT: Duration = Duration::from_secs(45);
const COOLDOWN: Duration = Duration::from_secs(30);
const BUSY: &str = "Another Office preview is being prepared. Try again shortly, open the file in MX Office, or download the original.";
const TOO_LARGE: &str = "This document is too complex for a server PDF preview. Open it in MX Office or download the original. Your file is unchanged.";
type Outcome = Result<Arc<Vec<u8>>, String>;
struct Entry {
    receiver: watch::Receiver<Option<Outcome>>,
    expires: Option<Instant>,
}
struct Coordinator {
    jobs: Mutex<HashMap<PathBuf, Entry>>,
    slots: Arc<Semaphore>,
}
impl Coordinator {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            jobs: Mutex::new(HashMap::new()),
            slots: Arc::new(Semaphore::new(1)),
        })
    }
    async fn run<F>(self: &Arc<Self>, key: PathBuf, convert: F) -> Outcome
    where
        F: FnOnce() -> Result<Vec<u8>, String> + Send + 'static,
    {
        let mut receiver = {
            let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
            jobs.retain(|_, entry| {
                entry
                    .expires
                    .is_none_or(|deadline| deadline > Instant::now())
            });
            if let Some(entry) = jobs.get(&key) {
                entry.receiver.clone()
            } else {
                let permit = self
                    .slots
                    .clone()
                    .try_acquire_owned()
                    .map_err(|_| BUSY.to_string())?;
                if jobs.len() >= 64 {
                    if let Some(old) = jobs
                        .iter()
                        .find(|(_, e)| e.expires.is_some())
                        .map(|(k, _)| k.clone())
                    {
                        jobs.remove(&old);
                    }
                }
                let (sender, receiver) = watch::channel(None);
                jobs.insert(
                    key.clone(),
                    Entry {
                        receiver: receiver.clone(),
                        expires: None,
                    },
                );
                let owner = self.clone();
                // The task owns cleanup/deadline even if every client disconnects.
                tokio::spawn(async move {
                    let outcome = tokio::task::spawn_blocking(convert)
                        .await
                        .map_err(|_| {
                            "Office preview worker stopped. Download the original or retry."
                                .to_string()
                        })
                        .and_then(|value| value)
                        .map(Arc::new);
                    let mut jobs = owner.jobs.lock().unwrap_or_else(|e| e.into_inner());
                    let failed = outcome.is_err();
                    let _ = sender.send(Some(outcome));
                    if failed {
                        if let Some(entry) = jobs.get_mut(&key) {
                            entry.expires = Some(Instant::now() + COOLDOWN);
                        }
                    } else {
                        jobs.remove(&key);
                    }
                    drop(permit);
                });
                receiver
            }
        };
        loop {
            if let Some(outcome) = receiver.borrow().clone() {
                return outcome;
            }
            receiver.changed().await.map_err(|_| {
                "Office preview worker disconnected. Your original is unchanged.".to_string()
            })?;
        }
    }
}

fn validate(extension: &str, bytes: &[u8]) -> Result<(), String> {
    check_size(bytes.len() as u64)?;
    if matches!(
        extension,
        "xlsx" | "ods" | "docx" | "odt" | "pptx" | "odp" | "docm" | "xlsm" | "pptm"
    ) {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| {
            "This Office archive is damaged. Download the original to inspect it.".to_string()
        })?;
        if archive.len() > 4096 {
            return Err(TOO_LARGE.into());
        }
        let mut expanded = 0u64;
        for index in 0..archive.len() {
            expanded =
                expanded.saturating_add(archive.by_index(index).map_err(|e| e.to_string())?.size());
            if expanded > MAX_EXPANDED {
                return Err(TOO_LARGE.into());
            }
        }
    }
    Ok(())
}
pub(crate) fn check_size(size: u64) -> Result<(), String> {
    if size > MAX_INPUT as u64 {
        Err(TOO_LARGE.into())
    } else {
        Ok(())
    }
}

struct WorkDirectory(PathBuf);
impl Drop for WorkDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct ConversionChild(Child);
impl Drop for ConversionChild {
    fn drop(&mut self) {
        // LibreOffice's launcher spawns soffice.bin; killing only the launcher
        // leaves an expensive orphan. This process group contains only our job.
        #[cfg(unix)]
        unsafe {
            libc::kill(-(self.0.id() as i32), libc::SIGKILL);
        }
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/PID", &self.0.id().to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn bounded_command(mut command: Command, timeout: Duration) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
        #[cfg(target_os = "linux")]
        unsafe {
            command.pre_exec(|| {
                let memory = libc::rlimit {
                    rlim_cur: 2 * 1024 * 1024 * 1024,
                    rlim_max: 2 * 1024 * 1024 * 1024,
                };
                let output = libc::rlimit {
                    rlim_cur: MAX_PDF,
                    rlim_max: MAX_PDF,
                };
                if libc::setrlimit(libc::RLIMIT_AS, &memory) != 0
                    || libc::setrlimit(libc::RLIMIT_FSIZE, &output) != 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let mut child = ConversionChild(command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()
        .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { "PDF preview is unavailable on this server. Open in MX Office or download the original.".into() } else { format!("Could not start the PDF preview: {e}") })?);
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.0.try_wait().map_err(|e| e.to_string())? {
            return if status.success() {
                Ok(())
            } else {
                Err("The PDF converter could not render this document within its resource limits. Open in MX Office or download the original.".into())
            };
        }
        if Instant::now() >= deadline {
            return Err("PDF preview stopped after 45 seconds. Open in MX Office or download the original; your file is unchanged.".into());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}
fn convert(extension: &str, bytes: Vec<u8>, cache: &Path) -> Result<Vec<u8>, String> {
    if fs::metadata(cache).is_ok_and(|metadata| metadata.len() <= MAX_PDF) {
        if let Ok(pdf) = fs::read(cache) {
            if pdf.len() as u64 <= MAX_PDF && pdf.starts_with(b"%PDF-") {
                return Ok(pdf);
            }
        }
    }
    let work =
        WorkDirectory(std::env::temp_dir().join(format!("mx-preview-work-{}", Uuid::new_v4())));
    let profile = work.0.join("lo-profile");
    fs::create_dir_all(&profile).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&work.0, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    let source = work.0.join(format!("source.{extension}"));
    fs::write(&source, bytes).map_err(|e| e.to_string())?;
    let mut command = Command::new("libreoffice");
    command
        .args([
            "--headless",
            "--nologo",
            "--nodefault",
            "--nolockcheck",
            "--nofirststartwizard",
        ])
        .arg(format!(
            "-env:UserInstallation=file://{}",
            profile.display()
        ))
        .args(["--convert-to", "pdf", "--outdir"])
        .arg(&work.0)
        .arg(source);
    bounded_command(command, CONVERSION_TIMEOUT)?;
    let generated = work.0.join("source.pdf");
    if fs::metadata(&generated)
        .map_err(|_| "The converter did not produce a PDF. Download the original.".to_string())?
        .len()
        > MAX_PDF
    {
        return Err(TOO_LARGE.into());
    }
    let pdf = fs::read(generated).map_err(|e| e.to_string())?;
    if !pdf.starts_with(b"%PDF-") {
        return Err("The converter did not return a valid PDF. Your original is unchanged.".into());
    }
    fs::create_dir_all(cache.parent().ok_or("Preview cache unavailable")?)
        .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            cache.parent().ok_or("Preview cache unavailable")?,
            fs::Permissions::from_mode(0o700),
        )
        .map_err(|e| e.to_string())?;
    }
    let staging = work.0.join("cached.pdf");
    fs::write(&staging, &pdf).map_err(|e| e.to_string())?;
    fs::rename(staging, cache).map_err(|e| e.to_string())?;
    Ok(pdf)
}
pub(crate) async fn generate(
    attachment: FileAttachment,
    bytes: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let extension = attachment
        .file_name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    validate(&extension, &bytes)?;
    let mut digest = Sha256::new();
    digest.update(extension.as_bytes());
    digest.update([0]);
    digest.update(&bytes);
    let digest: String = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let cache = std::env::temp_dir()
        .join("mx-preview-cache")
        .join(format!("{digest}.pdf"));
    static COORDINATOR: OnceLock<Arc<Coordinator>> = OnceLock::new();
    let coordinator = COORDINATOR.get_or_init(Coordinator::new);
    let target = cache.clone();
    coordinator
        .run(cache, move || convert(&extension, bytes, &target))
        .await
        .map(|pdf| (*pdf).clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn duplicate_requests_share_one_conversion() {
        let coordinator = Coordinator::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let owner = coordinator.clone();
            let count = calls.clone();
            tasks.push(tokio::spawn(async move {
                owner
                    .run("same-version".into(), move || {
                        count.fetch_add(1, Ordering::SeqCst);
                        std::thread::sleep(Duration::from_millis(100));
                        Ok(b"pdf".to_vec())
                    })
                    .await
            }));
        }
        for task in tasks {
            assert_eq!(task.await.unwrap().unwrap().as_slice(), b"pdf");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn failures_are_shared_and_cooled_down() {
        let coordinator = Coordinator::new();
        assert!(
            coordinator
                .run("failed".into(), || Err("bad document".into()))
                .await
                .is_err()
        );
        let result = coordinator
            .run("failed".into(), || {
                panic!("Must not start a second converter")
            })
            .await;
        assert_eq!(result.unwrap_err(), "bad document");
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn different_documents_cannot_start_parallel_converters() {
        let coordinator = Coordinator::new();
        let owner = coordinator.clone();
        let (started, waiting) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            owner
                .run("first".into(), move || {
                    let _ = started.send(());
                    std::thread::sleep(Duration::from_millis(100));
                    Ok(vec![])
                })
                .await
        });
        waiting.await.unwrap();
        assert_eq!(
            coordinator
                .run("second".into(), || panic!("Must not run in parallel"))
                .await
                .unwrap_err(),
            BUSY
        );
        task.await.unwrap().unwrap();
        assert!(
            coordinator
                .run("second".into(), || Ok(vec![]))
                .await
                .is_ok()
        );
    }
    #[test]
    fn preview_budgets_do_not_depend_only_on_compressed_size() {
        let mut bytes = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
            writer
                .start_file(
                    "xl/worksheets/sheet1.xml",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            std::io::Write::write_all(&mut writer, b"<worksheet/>").unwrap();
            writer.finish().unwrap();
        }
        validate("xlsx", &bytes).unwrap();
        let at = bytes
            .windows(4)
            .position(|v| v == [0x50, 0x4b, 0x01, 0x02])
            .unwrap();
        bytes[at + 24..at + 28].copy_from_slice(&((MAX_EXPANDED + 1) as u32).to_le_bytes());
        assert_eq!(validate("xlsx", &bytes).unwrap_err(), TOO_LARGE);
        assert!(validate("xlsx", b"damaged").is_err());
        assert_eq!(
            validate("xls", &vec![0; MAX_INPUT + 1]).unwrap_err(),
            TOO_LARGE
        );
    }
    #[test]
    #[cfg(unix)]
    fn deadline_kills_conversion_process_group() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "sleep 30 & wait"]);
        let start = Instant::now();
        assert!(
            bounded_command(command, Duration::from_millis(100))
                .unwrap_err()
                .contains("stopped")
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }
    #[test]
    #[ignore = "Requires an existing host LibreOffice; browser Office does not need it"]
    fn installed_converter_creates_a_bounded_pdf() {
        let directory =
            WorkDirectory(std::env::temp_dir().join(format!("mx-preview-test-{}", Uuid::new_v4())));
        fs::create_dir_all(&directory.0).unwrap();
        let cache = directory.0.join("cache.pdf");
        let input = br"{\rtf1\ansi MX bounded PDF preview test}".to_vec();
        let pdf = convert("rtf", input.clone(), &cache).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(pdf.len() < 1024 * 1024);
        assert_eq!(convert("rtf", input, &cache).unwrap(), pdf);
    }
}
