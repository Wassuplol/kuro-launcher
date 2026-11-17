//! Roblox's own package manifest, which is the authority for a direct install.
//!
//! Every deployment publishes `{upload}-rbxPkgManifest.txt`: a version line,
//! then four lines per package: name, MD5, compressed bytes, extracted bytes.
//! The MD5 is what makes a direct install verifiable, and it is also the only
//! baseline a repair can compare against, so it is parsed strictly and never
//! guessed at.

use crate::error::{FailureCode, LaunchError};

/// One downloadable package inside a deployment.
#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    /// Lowercase hex, as published. Compared case-insensitively.
    pub md5: String,
    pub compressed: u64,
    pub extracted: u64,
}

impl Package {
    /// The file name the CDN serves this package under.
    pub fn remote(&self, upload: &str) -> String {
        format!("{upload}-{}", self.name)
    }
}

/// Parses the manifest body. Returns an error rather than skipping a malformed
/// record: a partially understood manifest would silently install a partial
/// client, which is worse than refusing.
pub fn parse(body: &str) -> Result<Vec<Package>, LaunchError> {
    let lines: Vec<&str> = body
        .lines()
        .map(|line| line.trim_end_matches('\r').trim())
        .filter(|line| !line.is_empty())
        .collect();

    let Some(header) = lines.first() else {
        return Err(malformed("the manifest was empty"));
    };
    if *header != "v0" {
        return Err(malformed(&format!("unsupported manifest version {header}")));
    }

    let records = &lines[1..];
    if records.len() % 4 != 0 {
        return Err(malformed(&format!(
            "{} lines cannot be grouped into packages",
            records.len()
        )));
    }

    let mut packages = Vec::with_capacity(records.len() / 4);
    for group in records.chunks(4) {
        let name = group[0].to_string();
        let md5 = group[1].to_ascii_lowercase();
        if md5.len() != 32 || !md5.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(malformed(&format!("{name} has an unusable hash")));
        }
        let compressed = group[2]
            .parse::<u64>()
            .map_err(|_| malformed(&format!("{name} has an unusable size")))?;
        let extracted = group[3]
            .parse::<u64>()
            .map_err(|_| malformed(&format!("{name} has an unusable size")))?;
        packages.push(Package {
            name,
            md5,
            compressed,
            extracted,
        });
    }

    if packages.is_empty() {
        return Err(malformed("the manifest listed no packages"));
    }

    Ok(packages)
}

fn malformed(detail: &str) -> LaunchError {
    LaunchError::new(
        FailureCode::ClientCorrupt,
        format!("Roblox's package list could not be read: {detail}."),
    )
    .with_action("Try the official installer")
}

/// Total download and installed size for a package list, for the one line the
/// install screen shows before anything is fetched.
pub fn totals(packages: &[Package]) -> (u64, u64) {
    packages.iter().fold((0, 0), |(down, disk), package| {
        (down + package.compressed, disk + package.extracted)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "v0\r\nRobloxApp.zip\r\n60e244567d170679c8c497bc5ee888de\r\n138433382\r\n175470635\r\ncontent-terrain.zip\r\nfe7b3ecee91e83f9fade72e8b0f4d47f\r\n4807\r\n47610\r\n";

    #[test]
    fn reads_the_published_shape() {
        let packages = parse(SAMPLE).expect("sample parses");
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, "RobloxApp.zip");
        assert_eq!(packages[0].md5, "60e244567d170679c8c497bc5ee888de");
        assert_eq!(packages[0].compressed, 138433382);
        assert_eq!(packages[0].extracted, 175470635);
        assert_eq!(
            packages[1].remote("version-2c6fd5e09aaf4cfa"),
            "version-2c6fd5e09aaf4cfa-content-terrain.zip"
        );
    }

    #[test]
    fn totals_add_up() {
        let packages = parse(SAMPLE).expect("sample parses");
        assert_eq!(totals(&packages), (138438189, 175518245));
    }

    #[test]
    fn refuses_a_truncated_manifest() {
        let broken = "v0\r\nRobloxApp.zip\r\n60e244567d170679c8c497bc5ee888de\r\n138433382\r\n";
        let error = parse(broken).expect_err("truncated manifest must fail");
        assert_eq!(error.code, FailureCode::ClientCorrupt);
    }

    #[test]
    fn refuses_an_unknown_version_line() {
        assert!(parse("v1\r\nA.zip\r\n60e244567d170679c8c497bc5ee888de\r\n1\r\n1\r\n").is_err());
    }

    #[test]
    fn refuses_a_short_hash() {
        assert!(parse("v0\r\nA.zip\r\nabc\r\n1\r\n1\r\n").is_err());
    }
}
