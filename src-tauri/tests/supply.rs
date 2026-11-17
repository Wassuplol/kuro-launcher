//! What stage 02 has to prove, and how much of it needs Roblox's servers.
//!
//! The tests that need the network are `#[ignore]`d on purpose: CI must not go
//! red because Roblox's CDN had a bad minute, and a test that silently skips is
//! worse than one that has to be asked for. Run them with
//!
//! ```text
//! cargo test --test supply -- --ignored --nocapture
//! ```

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use kuro_lib::error::FailureCode;
use kuro_lib::supply::{self, manifest, pe, prune, Kind};

/// The endpoint the whole milestone hangs on: it must answer, and it must
/// answer in the shape the parser expects.
#[test]
#[ignore = "needs Roblox's version service"]
fn live_resolves_for_both_clients() {
    let player = supply::resolve(Kind::Player, "LIVE").expect("player resolves");
    assert_eq!(player.channel, "LIVE");
    assert!(
        player.upload.starts_with("version-"),
        "upload folder should be a version- name, got {}",
        player.upload
    );
    assert!(
        player.version.split('.').count() >= 3,
        "version should be a dotted build, got {}",
        player.version
    );
    println!("player LIVE {} -> {}", player.version, player.upload);

    let studio = supply::resolve(Kind::Studio, "LIVE").expect("studio resolves");
    println!("studio LIVE {} -> {}", studio.version, studio.upload);
    assert!(studio.upload.starts_with("version-"));
}

/// The measured refusal. If Roblox ever opens these channels anonymously, this
/// test fails and the UI's "unavailable" note becomes a lie, which is exactly
/// when someone should notice.
#[test]
#[ignore = "needs Roblox's version service"]
fn the_closed_channels_still_refuse() {
    for channel in ["zcanary", "zintegration"] {
        let error = supply::resolve(Kind::Player, channel).expect_err("should refuse");
        assert_eq!(error.code, FailureCode::ChannelUnavailable, "{channel}");
        println!("{channel}: {}", error.message);
    }
}

/// Downloads the smallest package in the deployment and checks it against the
/// manifest hash. This is the verification the direct install depends on, run
/// against real bytes rather than a fixture.
#[test]
#[ignore = "needs Roblox's CDN"]
fn a_real_package_matches_its_published_hash() {
    let live = supply::resolve(Kind::Player, "LIVE").expect("player resolves");
    let packages = supply::fetch_manifest(&live.upload).expect("manifest fetches");
    let (download, disk) = manifest::totals(&packages);
    println!(
        "{} packages, {:.1} MB to download, {:.1} MB on disk",
        packages.len(),
        download as f64 / 1e6,
        disk as f64 / 1e6
    );

    let smallest = packages
        .iter()
        .min_by_key(|package| package.compressed)
        .expect("at least one package");

    let url = format!("{}/{}", supply::CDN, smallest.remote(&live.upload));
    let body = supply::agent(std::time::Duration::from_secs(120))
        .get(&url)
        .call()
        .expect("package downloads")
        .into_body()
        .read_to_vec()
        .expect("package body reads");

    let actual = format!("{:x}", <md5::Md5 as md5::Digest>::digest(&body));
    println!("{} {} bytes, md5 {}", smallest.name, body.len(), actual);
    assert_eq!(actual, smallest.md5, "published hash must match the bytes");
}

/// A signature check against the copy of Roblox on this machine. Skipped rather
/// than failed when there is no client installed, because a clean machine is
/// not a broken build.
#[test]
fn an_installed_client_is_signed_by_roblox() {
    let root = match supply::versions_root() {
        Ok(root) => root,
        Err(_) => return,
    };
    if !root.is_dir() {
        println!("no Roblox on this machine, nothing to check");
        return;
    }

    let installed = supply::discover(&root, &None, None).expect("discovery runs");
    assert!(
        !installed.is_empty(),
        "the versions folder exists, so discovery must find something in it"
    );

    // Strict on purpose: a skip here would hide a broken version-resource
    // reader, which is exactly the bug this test exists to catch.
    let client = installed
        .iter()
        .find(|client| client.real)
        .unwrap_or_else(|| {
            panic!(
                "no folder read as a real client: {:?}",
                installed
                    .iter()
                    .map(|c| (&c.folder, &c.publisher, &c.version))
                    .collect::<Vec<_>>()
            )
        });

    let kind = client.kind.expect("a real client has a kind");
    let executable = std::path::Path::new(&client.path).join(kind.executable());
    let identity = pe::read_identity(&executable).expect("identity reads");
    assert!(identity.claims_roblox(), "publisher should be Roblox");
    assert_eq!(
        pe::signature(&executable),
        pe::Signature::Trusted,
        "Windows should trust the installed client's signature"
    );
    println!(
        "{} {} signed, publisher {}",
        client.folder,
        identity.version.unwrap_or_default(),
        identity.company.unwrap_or_default()
    );
}

/// A prune must keep the newest copy, and must keep anything pinned. Both are
/// decisions that cost someone a 250 MB download if they go wrong.
#[test]
fn prune_keeps_the_newest_and_the_pinned() {
    let make = |folder: &str, version: &str, pinned: bool, live: bool| supply::InstalledClient {
        folder: folder.into(),
        path: format!("C:/x/{folder}"),
        root: "This user".into(),
        per_user: true,
        kind: Some(Kind::Player),
        version: Some(version.into()),
        publisher: Some("Roblox Corporation".into()),
        real: true,
        bytes: 100,
        files: 10,
        modified: Some(1),
        tracked: false,
        pinned,
        holder: None,
        live,
    };

    let installed = vec![
        make("version-new", "0.748.0.7481526", false, true),
        make("version-old", "0.747.0.7471481", false, false),
        make("version-pinned", "0.700.0.7000001", true, false),
    ];

    let plan = prune::plan(&installed);
    assert_eq!(plan.remove, vec!["version-old".to_string()]);
    assert_eq!(plan.keep.len(), 2);
    assert_eq!(plan.bytes, 100);
}

/// Studio is never part of a prune. It has its own installer, and a person
/// clearing out old player builds does not expect to lose it.
#[test]
fn prune_leaves_studio_alone() {
    let player = |folder: &str, version: &str, live: bool| supply::InstalledClient {
        folder: folder.into(),
        path: format!("C:/x/{folder}"),
        root: "This user".into(),
        per_user: true,
        kind: Some(Kind::Player),
        version: Some(version.into()),
        publisher: Some("Roblox Corporation".into()),
        real: true,
        bytes: 100,
        files: 10,
        modified: Some(1),
        tracked: false,
        pinned: false,
        holder: None,
        live,
    };

    let mut studio = player("version-studio", "0.600.0.6000000", false);
    studio.kind = Some(Kind::Studio);

    let installed = vec![player("version-new", "0.748.0.7481526", true), studio];
    let plan = prune::plan(&installed);

    assert!(
        plan.remove.is_empty(),
        "nothing should be offered for removal"
    );
    assert!(
        plan.keep.iter().any(|kept| kept.folder == "version-studio"),
        "Studio must be kept"
    );
}

/// Staging folders must never be offered as installs. The install writes them
/// next to real clients, so discovery is the only thing standing between a
/// half-unpacked folder and a launch against it.
#[test]
fn discovery_ignores_anything_that_is_not_a_version_folder() {
    let root = std::env::temp_dir().join("kuro-discovery-test");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("version-real")).expect("real");
    std::fs::create_dir_all(root.join(".kuro-version-real.partial")).expect("staging");
    std::fs::create_dir_all(root.join("RobloxStudioInstaller.exe")).expect("junk");

    let found = supply::discover(&root, &None, None).expect("discovery runs");
    let names: Vec<String> = found.into_iter().map(|client| client.folder).collect();
    assert_eq!(names, vec!["version-real".to_string()]);
    let _ = std::fs::remove_dir_all(&root);
}

/// The whole direct path, into a scratch folder, against Roblox's real CDN.
///
/// This is the milestone's central claim, so it is proven here rather than
/// described: manifest, 22 hashed packages, unpack, signature gate, rename.
#[test]
#[ignore = "downloads about 242 MB from Roblox's CDN"]
fn a_direct_install_lands_and_verifies() {
    let live = supply::resolve(Kind::Player, "LIVE").expect("resolves");
    let root = std::env::temp_dir().join("kuro-direct-install-test");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch root");

    let cancel = Arc::new(AtomicBool::new(false));
    let report = |phase: &str, label: String, _done: u64, _total: u64| {
        if phase != "download" {
            println!("{phase}: {label}");
        }
    };

    supply::install::direct_into(&report, &cancel, &root, Kind::Player, &live.upload)
        .expect("the direct install must succeed against Roblox's own packages");

    let landed = root.join(&live.upload);
    assert!(
        landed.is_dir(),
        "the version folder must exist after a rename"
    );
    assert!(
        landed.join("RobloxPlayerBeta.exe").is_file(),
        "the client executable must be in it"
    );
    assert!(
        landed.join(".kuro-integrity.json").is_file(),
        "a hash baseline must be written for later repair"
    );

    let report = supply::install::verify(&landed).expect("verify runs");
    assert!(report.tracked, "the folder must be tracked");
    assert!(
        report.clean(),
        "a fresh install must verify clean, got {} mismatched and {} missing",
        report.mismatched.len(),
        report.missing.len()
    );
    println!("verified {} files", report.checked);

    let _ = std::fs::remove_dir_all(&root);
}

/// Prints what discovery sees for every folder, including whether something
/// holds it open. Useful on a machine where a client is running, and the only
/// way to see the lock answer without a window.
#[test]
#[ignore = "reads this machine's Roblox install"]
fn prints_what_discovery_sees() {
    let installed = supply::discover_all(&None, None).expect("discovery runs");
    for client in &installed {
        println!(
            "{} | {} | root={} | version={:?} | real={} | holder={:?} | {} bytes | {} files",
            client.folder,
            client.kind.map(|k| k.label()).unwrap_or("?"),
            client.root,
            client.version,
            client.real,
            client.holder,
            client.bytes,
            client.files
        );
    }
    println!("install root: {:?}", supply::install_root());
    assert!(!installed.is_empty(), "expected at least one folder");
}
