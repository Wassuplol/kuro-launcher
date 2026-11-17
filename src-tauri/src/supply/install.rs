//! Getting a client onto the disk, two ways, with the same promise: what lands
//! in `Versions` is what Roblox published.
//!
//! Direct install is the honest one. Roblox publishes a package manifest with
//! an MD5 per archive; every archive is hashed as it arrives, unpacked into a
//! staging folder, hashed again per file into a baseline, and only then renamed
//! into place. A rename on one volume is atomic, so a half-installed client is
//! never visible to Roblox or to this app.
//!
//! The official installer is the other path. It is downloaded, its signature is
//! checked, and it is left to do its own work. Any failure on the direct path
//! falls through to it automatically, which is what the brief asks for.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use md5::{Digest as _, Md5};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::error::{FailureCode, LaunchError};

use super::manifest::Package;
use super::{agent, fetch_manifest, pe, versions_root, Kind, CDN};

/// The hash baseline this app writes after a direct install. Its presence is
/// also how a folder is known to be ours, which is why it is a dot file that
/// Roblox never looks for.
pub const BASELINE: &str = ".kuro-integrity.json";

/// How the client arrives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// Roblox's own installer. The default, because it is Roblox's supported
    /// path and it repairs whatever it finds.
    Bootstrap,
    /// Packages straight from the CDN, hash for hash.
    Direct,
}

/// What the install screen shows while work is happening.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// manifest, download, extract, verify, installer, done
    pub phase: String,
    pub label: String,
    pub done: u64,
    pub total: u64,
}

/// The event name the command layer emits on, and the shape it sends.
pub const EVENT: &str = "supply:progress";

/// Where progress goes. The install engine takes this instead of an app
/// handle, so the whole pipeline can run without a window and be tested.
pub type Reporter<'a> = &'a dyn Fn(&str, String, u64, u64);

fn cancelled() -> LaunchError {
    LaunchError::new(FailureCode::Cancelled, "The install was cancelled.")
        .with_action("Start again")
}

fn disk(path: &Path, detail: &str, error: std::io::Error) -> LaunchError {
    LaunchError::new(
        FailureCode::ConfigUnwritable,
        format!("{detail}: {} ({error})", path.display()),
    )
}

/// The handles one install needs while it runs: where to report, how to stop,
/// and how far the whole job has got. Carried together so no helper has to take
/// seven arguments to say the same thing.
struct Job<'a> {
    cancel: &'a Arc<AtomicBool>,
    report: Reporter<'a>,
    /// Bytes fetched across every package so far, so one bar covers the job
    /// rather than restarting at each package.
    downloaded: u64,
    download_total: u64,
}

impl<'a> Job<'a> {
    fn new(cancel: &'a Arc<AtomicBool>, report: Reporter<'a>) -> Self {
        Self {
            cancel,
            report,
            downloaded: 0,
            download_total: 0,
        }
    }

    fn stopped(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    fn say(&self, phase: &str, label: impl Into<String>, done: u64, total: u64) {
        (self.report)(phase, label.into(), done, total);
    }
}

/// The staging folder for a deployment. It starts with a dot, and discovery
/// only accepts `version-`, so a half-written client can never be mistaken for
/// an installed one.
fn staging(root: &Path, upload: &str) -> PathBuf {
    root.join(format!(".kuro-{upload}.partial"))
}

/// Removes staging folders left behind by a crash or a closed window.
pub fn sweep(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(".kuro-") {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// One verified download, written to `into` and hashed on the way through.
fn download(
    job: &mut Job,
    url: &str,
    into: &Path,
    expected_md5: Option<&str>,
    label: &str,
) -> Result<u64, LaunchError> {
    let response = agent(Duration::from_secs(1800))
        .get(url)
        .call()
        .map_err(|e| {
            LaunchError::new(
                FailureCode::Network,
                format!("{label} could not be downloaded: {e}"),
            )
            .with_action("Try the official installer")
        })?;

    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(LaunchError::new(
            FailureCode::Network,
            format!("Roblox has no {label} at that address (answered {status})."),
        )
        .with_action("Try the official installer"));
    }

    let mut reader = response.into_body().into_reader();
    let mut file =
        File::create(into).map_err(|e| disk(into, "A download could not be saved", e))?;

    let mut hasher = Md5::new();
    let mut written = 0u64;
    let mut buffer = vec![0u8; 256 * 1024];
    let mut last_report = Instant::now();

    loop {
        if job.stopped() {
            drop(file);
            let _ = fs::remove_file(into);
            return Err(cancelled());
        }

        let read = reader.read(&mut buffer).map_err(|e| {
            LaunchError::new(
                FailureCode::Network,
                format!("{label} stopped arriving: {e}"),
            )
            .with_action("Retry")
        })?;
        if read == 0 {
            break;
        }

        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .map_err(|e| disk(into, "A download could not be written", e))?;
        written += read as u64;

        if last_report.elapsed() >= Duration::from_millis(120) {
            let done = job.downloaded + written;
            let total = job.download_total;
            job.say("download", label.to_string(), done, total);
            last_report = Instant::now();
        }
    }

    file.flush()
        .map_err(|e| disk(into, "A download could not be flushed", e))?;
    drop(file);

    if let Some(expected) = expected_md5 {
        let actual = format!("{:x}", hasher.finalize());
        if actual != expected.to_ascii_lowercase() {
            let _ = fs::remove_file(into);
            return Err(LaunchError::new(
                FailureCode::ClientCorrupt,
                format!(
                    "{label} arrived with the wrong hash, so it was thrown away rather than installed."
                ),
            )
            .with_action("Try the official installer"));
        }
    }

    Ok(written)
}

/// One file's hash, and which archive it came from, so a repair knows exactly
/// what to fetch again.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHash {
    sha256: String,
    package: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Baseline {
    upload: String,
    kind: Kind,
    created: u64,
    /// Relative path, using forward slashes so the file is portable.
    files: BTreeMap<String, FileHash>,
}

impl Baseline {
    fn read(path: &Path) -> Option<Baseline> {
        let bytes = fs::read(path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }
}

/// Unpacks one archive into `into`, hashing every file on the way out.
///
/// `enclosed_name` is the traversal guard: it returns `None` for any entry
/// whose path would escape the destination, so a hostile archive cannot write
/// outside the staging folder.
fn extract(
    archive_path: &Path,
    into: &Path,
    package: &str,
    hashes: &mut BTreeMap<String, FileHash>,
) -> Result<(), LaunchError> {
    let file = File::open(archive_path)
        .map_err(|e| disk(archive_path, "An archive could not be read", e))?;
    let mut zip = zip::ZipArchive::new(BufReader::new(file)).map_err(|e| {
        LaunchError::new(
            FailureCode::ClientCorrupt,
            format!("{package} is not a readable archive: {e}"),
        )
        .with_action("Try the official installer")
    })?;

    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|e| {
            LaunchError::new(
                FailureCode::ClientCorrupt,
                format!("{package} could not be unpacked: {e}"),
            )
        })?;

        let Some(relative) = entry.enclosed_name() else {
            // A path that tries to leave the folder. Refusing the whole archive
            // is the only safe answer.
            return Err(LaunchError::new(
                FailureCode::ClientCorrupt,
                format!("{package} contains a path that escapes the install folder."),
            ));
        };

        let target = into.join(&relative);

        if entry.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|e| disk(&target, "A folder could not be created", e))?;
            continue;
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| disk(parent, "A folder could not be created", e))?;
        }

        let mut out =
            File::create(&target).map_err(|e| disk(&target, "A file could not be created", e))?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 256 * 1024];

        loop {
            let read = entry.read(&mut buffer).map_err(|e| {
                LaunchError::new(
                    FailureCode::ClientCorrupt,
                    format!("{package} could not be unpacked: {e}"),
                )
            })?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            out.write_all(&buffer[..read])
                .map_err(|e| disk(&target, "A file could not be written", e))?;
        }

        out.flush()
            .map_err(|e| disk(&target, "A file could not be flushed", e))?;
        drop(out);

        hashes.insert(
            relative.to_string_lossy().replace('\\', "/"),
            FileHash {
                sha256: format!("{:x}", hasher.finalize()),
                package: package.to_string(),
            },
        );
    }

    Ok(())
}

/// Places one manifest entry that is a plain file rather than an archive.
///
/// Roblox's manifest ends with `RobloxPlayerInstaller.exe`, which is served as
/// the executable itself: its compressed and extracted sizes are equal and it
/// has no end-of-central-directory record. Treating it as an archive fails the
/// whole install, which is exactly what happened the first time this ran.
fn place_file(
    source: &Path,
    into: &Path,
    package: &str,
    hashes: &mut BTreeMap<String, FileHash>,
) -> Result<(), LaunchError> {
    let target = into.join(package);
    let mut input =
        File::open(source).map_err(|e| disk(source, "A package could not be read", e))?;
    let mut output =
        File::create(&target).map_err(|e| disk(&target, "A file could not be created", e))?;

    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|e| disk(source, "A package could not be read", e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .map_err(|e| disk(&target, "A file could not be written", e))?;
    }
    output
        .flush()
        .map_err(|e| disk(&target, "A file could not be flushed", e))?;

    hashes.insert(
        package.to_string(),
        FileHash {
            sha256: format!("{:x}", hasher.finalize()),
            package: package.to_string(),
        },
    );

    Ok(())
}

/// Proves the unpacked client is the one Roblox ships: a version resource that
/// names Roblox Corporation, and a signature Windows trusts.
fn gate(staging: &Path, kind: Kind) -> Result<(), LaunchError> {
    let executable = staging.join(kind.executable());
    if !executable.is_file() {
        return Err(LaunchError::new(
            FailureCode::ClientCorrupt,
            format!(
                "The download finished but {} was not in it, so it was thrown away.",
                kind.executable()
            ),
        )
        .with_action("Try the official installer"));
    }

    let identity = pe::read_identity(&executable).ok_or_else(|| {
        LaunchError::new(
            FailureCode::ClientCorrupt,
            format!("{} carries no version information.", kind.executable()),
        )
        .with_action("Try the official installer")
    })?;

    if !identity.claims_roblox() {
        return Err(LaunchError::new(
            FailureCode::ClientCorrupt,
            format!(
                "The download names {} as its publisher, not Roblox Corporation, so it was thrown away.",
                identity.company.unwrap_or_else(|| "nobody".into())
            ),
        )
        .with_action("Try the official installer"));
    }

    match pe::signature(&executable) {
        pe::Signature::Trusted => Ok(()),
        pe::Signature::Unsigned => Err(LaunchError::new(
            FailureCode::ClientCorrupt,
            "The downloaded client is not signed, so it was thrown away.",
        )
        .with_action("Try the official installer")),
        pe::Signature::Rejected => Err(LaunchError::new(
            FailureCode::ClientCorrupt,
            "Windows does not trust the downloaded client's signature, so it was thrown away.",
        )
        .with_action("Try the official installer")),
        pe::Signature::Unknown => Err(LaunchError::new(
            FailureCode::ClientCorrupt,
            "The downloaded client's signature could not be checked, so it was thrown away.",
        )
        .with_action("Try the official installer")),
    }
}

/// The direct path: manifest, packages, unpack, gate, rename.
fn direct(job: &mut Job, root: &Path, kind: Kind, upload: &str) -> Result<(), LaunchError> {
    job.say("manifest", "Reading Roblox's package list", 0, 0);
    let packages = fetch_manifest(upload)?;
    let (download_total, disk_total) = super::manifest::totals(&packages);
    job.download_total = download_total;

    let staging = staging(root, upload);
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)
        .map_err(|e| disk(&staging, "The staging folder could not be created", e))?;

    let work = root.join(format!(".kuro-{upload}.download"));
    fs::create_dir_all(&work)
        .map_err(|e| disk(&work, "The download folder could not be created", e))?;

    let outcome = unpack_all(job, kind, upload, &packages, &staging, &work, disk_total);

    let _ = fs::remove_dir_all(&work);

    if let Err(error) = outcome {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    // Atomic on one volume: the client appears whole or not at all.
    let final_path = root.join(upload);
    if final_path.exists() {
        let _ = fs::remove_dir_all(&staging);
        return Err(LaunchError::new(
            FailureCode::ClientCorrupt,
            format!("{upload} is already in the versions folder."),
        )
        .with_action("Repair it instead"));
    }

    job.say("verify", "Checking the signature", 0, 0);
    if let Err(error) = gate(&staging, kind) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    fs::rename(&staging, &final_path).map_err(|e| {
        disk(
            &final_path,
            "The finished client could not be moved into place",
            e,
        )
    })?;

    Ok(())
}

fn unpack_all(
    job: &mut Job,
    kind: Kind,
    upload: &str,
    packages: &[Package],
    staging: &Path,
    work: &Path,
    disk_total: u64,
) -> Result<(), LaunchError> {
    let mut hashes: BTreeMap<String, FileHash> = BTreeMap::new();
    let mut unpacked = 0u64;

    for package in packages {
        if job.stopped() {
            return Err(cancelled());
        }

        let url = format!("{CDN}/{}", package.remote(upload));
        let local = work.join(&package.name);

        download(job, &url, &local, Some(&package.md5), &package.name)?;
        job.downloaded += package.compressed;

        job.say(
            "extract",
            format!("Unpacking {}", package.name),
            unpacked,
            disk_total,
        );
        if package.name.to_ascii_lowercase().ends_with(".zip") {
            extract(&local, staging, &package.name, &mut hashes)?;
        } else {
            place_file(&local, staging, &package.name, &mut hashes)?;
        }
        unpacked += package.extracted;

        // The archive has done its job; keeping it would double the disk cost.
        let _ = fs::remove_file(&local);
    }

    let baseline = Baseline {
        upload: upload.to_string(),
        kind,
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        files: hashes,
    };
    let body = serde_json::to_vec(&baseline).map_err(|e| {
        LaunchError::new(
            FailureCode::Internal,
            format!("The integrity baseline could not be encoded: {e}"),
        )
    })?;
    fs::write(staging.join(BASELINE), body)
        .map_err(|e| disk(staging, "The baseline could not be written", e))?;

    Ok(())
}

/// The official path: download Roblox's installer, check it, let it run.
fn bootstrap(job: &mut Job, root: &Path, kind: Kind, upload: &str) -> Result<(), LaunchError> {
    let installer = match kind {
        Kind::Player => "RobloxPlayerInstaller.exe",
        Kind::Studio => "RobloxStudioInstaller.exe",
    };

    let work = root.join(format!(".kuro-{upload}.download"));
    fs::create_dir_all(&work)
        .map_err(|e| disk(&work, "The download folder could not be created", e))?;
    let local = work.join(installer);

    job.say("installer", "Downloading Roblox's installer", 0, 0);
    let size = download(
        job,
        &format!("{CDN}/{upload}-{installer}"),
        &local,
        None,
        installer,
    )?;

    job.say("installer", "Checking the installer's signature", 0, size);
    let identity = pe::read_identity(&local);
    let trusted = pe::signature(&local) == pe::Signature::Trusted;
    if !trusted
        || !identity
            .as_ref()
            .is_some_and(pe::FileIdentity::claims_roblox)
    {
        let _ = fs::remove_dir_all(&work);
        return Err(LaunchError::new(
            FailureCode::ClientCorrupt,
            "Roblox's installer did not carry a signature this machine trusts, so it was not run.",
        ));
    }

    job.say("installer", "Roblox's installer is running", 0, size);
    std::process::Command::new(&local)
        .current_dir(&work)
        .spawn()
        .map_err(|e| {
            LaunchError::new(
                FailureCode::Network,
                format!("Roblox's installer could not be started: {e}"),
            )
        })?;

    // Wait for Roblox to put the folder there. The installer decides when it is
    // finished; polling its result is more honest than guessing at its timing.
    //
    // Every root is watched, not the one this function was handed. Roblox's
    // installer chooses its own destination: asked to install on this machine it
    // wrote to Program Files while the root it was given was the per-user one,
    // reported success, and this poll sat waiting in the wrong folder until it
    // timed out. Its own log is what settled it.
    let deadline = Instant::now() + Duration::from_secs(600);

    while Instant::now() < deadline {
        if job.stopped() {
            return Err(cancelled());
        }

        let landed = super::roots().iter().any(|root| {
            Path::new(&root.path)
                .join(upload)
                .join(kind.executable())
                .is_file()
        });
        if landed {
            let _ = fs::remove_dir_all(&work);
            return Ok(());
        }

        std::thread::sleep(Duration::from_millis(700));
    }

    Err(LaunchError::new(
        FailureCode::Network,
        "Roblox's installer finished without placing the client in any versions folder.",
    )
    .with_action("Try the direct install"))
}

/// The direct path on its own, with no fallback.
///
/// Exposed so the pipeline can be exercised against a scratch folder: the
/// fallback is a product decision, and a test that silently ran Roblox's
/// installer instead would prove nothing about the code under test.
pub fn direct_into(
    report: Reporter,
    cancel: &Arc<AtomicBool>,
    root: &Path,
    kind: Kind,
    upload: &str,
) -> Result<(), LaunchError> {
    let mut job = Job::new(cancel, report);
    direct(&mut job, root, kind, upload)
}

/// Runs an install and reports what ended up on disk.
pub fn run(
    report: Reporter,
    cancel: &Arc<AtomicBool>,
    kind: Kind,
    upload: &str,
    method: Method,
) -> Result<(), LaunchError> {
    install_into(report, cancel, &versions_root()?, kind, upload, method)
}

/// The same install, into a folder the caller names. `run` is this plus the
/// real versions root, which is also what lets the pipeline be tested without
/// touching the machine's actual Roblox install.
pub fn install_into(
    report: Reporter,
    cancel: &Arc<AtomicBool>,
    root: &Path,
    kind: Kind,
    upload: &str,
    method: Method,
) -> Result<(), LaunchError> {
    fs::create_dir_all(root)
        .map_err(|e| disk(root, "The Roblox versions folder could not be created", e))?;
    sweep(root);

    let mut job = Job::new(cancel, report);

    let result = match method {
        Method::Direct => direct(&mut job, root, kind, upload),
        Method::Bootstrap => bootstrap(&mut job, root, kind, upload),
    };

    // The brief asks for this explicitly: a direct install that fails anywhere
    // falls through to Roblox's own installer rather than leaving the person
    // with nothing. A cancel is the one exception, because it was asked for.
    if let Err(error) = &result {
        if method == Method::Direct && error.code != FailureCode::Cancelled {
            // The reason the direct path failed survives on screen: a fallback
            // that works is still something the person should be told about.
            job.say("fallback", error.message.clone(), 0, 0);
            let fallback = bootstrap(&mut job, root, kind, upload);
            if fallback.is_ok() {
                job.say("done", "Installed", 1, 1);
            }
            return fallback;
        }
    }

    match &result {
        Ok(()) => job.say("done", "Installed", 1, 1),
        Err(error) => job.say("failed", error.message.clone(), 0, 0),
    }

    result
}

/// Re-hashes a folder against its baseline and names every file that moved.
///
/// A folder this app did not install has no baseline, and that is reported as
/// exactly what it is rather than as a pass.
pub fn verify(path: &Path) -> Result<VerifyReport, LaunchError> {
    let baseline = Baseline::read(&path.join(BASELINE));
    let Some(baseline) = baseline else {
        return Ok(VerifyReport {
            folder: folder_name(path),
            tracked: false,
            checked: 0,
            mismatched: Vec::new(),
            missing: Vec::new(),
            packages: Vec::new(),
        });
    };

    let mut mismatched = Vec::new();
    let mut missing = Vec::new();
    let mut packages = std::collections::BTreeSet::new();

    for (relative, expected) in &baseline.files {
        let target = path.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
        match fs::read(&target) {
            Ok(bytes) => {
                let actual = format!("{:x}", Sha256::digest(&bytes));
                if actual != expected.sha256 {
                    mismatched.push(relative.clone());
                    packages.insert(expected.package.clone());
                }
            }
            Err(_) => {
                missing.push(relative.clone());
                packages.insert(expected.package.clone());
            }
        }
    }

    Ok(VerifyReport {
        folder: folder_name(path),
        tracked: true,
        checked: baseline.files.len(),
        mismatched,
        missing,
        packages: packages.into_iter().collect(),
    })
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyReport {
    pub folder: String,
    /// False when Kuro has no baseline for this folder, which means the check
    /// could not run rather than that the folder is clean.
    pub tracked: bool,
    pub checked: usize,
    pub mismatched: Vec<String>,
    pub missing: Vec<String>,
    /// The archives a repair would need to fetch again.
    pub packages: Vec<String>,
}

impl VerifyReport {
    pub fn clean(&self) -> bool {
        self.tracked && self.mismatched.is_empty() && self.missing.is_empty()
    }
}

/// Repairs a tracked folder by fetching only the archives that own a bad file
/// and unpacking them over the existing install.
pub fn repair(
    report: Reporter,
    cancel: &Arc<AtomicBool>,
    upload: &str,
    packages: &[String],
) -> Result<(), LaunchError> {
    let root = versions_root()?;
    let target = root.join(upload);
    if !target.is_dir() {
        return Err(LaunchError::new(
            FailureCode::ClientMissing,
            format!("{upload} is no longer in the versions folder."),
        )
        .with_action("Install it again"));
    }

    let wanted = fetch_manifest(upload)?;
    let work = root.join(format!(".kuro-{upload}.download"));
    fs::create_dir_all(&work)
        .map_err(|e| disk(&work, "The download folder could not be created", e))?;

    let total: u64 = wanted
        .iter()
        .filter(|package| packages.iter().any(|name| name == &package.name))
        .map(|package| package.compressed)
        .sum();

    let mut job = Job::new(cancel, report);
    job.download_total = total;

    let mut hashes: BTreeMap<String, FileHash> = BTreeMap::new();
    let mut result = Ok(());

    for package in wanted
        .iter()
        .filter(|package| packages.iter().any(|name| name == &package.name))
    {
        let local = work.join(&package.name);
        if let Err(error) = download(
            &mut job,
            &format!("{CDN}/{}", package.remote(upload)),
            &local,
            Some(&package.md5),
            &package.name,
        ) {
            result = Err(error);
            break;
        }
        job.downloaded += package.compressed;

        job.say(
            "extract",
            format!("Repairing {}", package.name),
            job.downloaded,
            total,
        );
        let placed = if package.name.to_ascii_lowercase().ends_with(".zip") {
            extract(&local, &target, &package.name, &mut hashes)
        } else {
            place_file(&local, &target, &package.name, &mut hashes)
        };
        if let Err(error) = placed {
            result = Err(error);
            break;
        }
        let _ = fs::remove_file(&local);
    }

    let _ = fs::remove_dir_all(&work);
    result?;

    // Fold the repaired files into the existing baseline so the next check is
    // against the same contract, not a fresh one.
    let baseline_path = target.join(BASELINE);
    if let Some(mut baseline) = Baseline::read(&baseline_path) {
        for (path, hash) in hashes {
            baseline.files.insert(path, hash);
        }
        if let Ok(body) = serde_json::to_vec(&baseline) {
            let _ = fs::write(&baseline_path, body);
        }
    }

    job.say("done", "Repaired", 1, 1);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staging_folders_are_invisible_to_discovery() {
        let root = Path::new("C:/x/Versions");
        let path = staging(root, "version-abc");
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        assert!(!name.starts_with(super::super::FOLDER_PREFIX));
        assert!(name.starts_with(".kuro-"));
    }

    #[test]
    fn a_clean_report_needs_a_baseline() {
        let report = VerifyReport {
            folder: "version-x".into(),
            tracked: false,
            checked: 0,
            mismatched: vec![],
            missing: vec![],
            packages: vec![],
        };
        assert!(!report.clean(), "an unchecked folder is not a clean folder");
    }
}
