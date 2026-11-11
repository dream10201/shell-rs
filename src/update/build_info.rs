//! What this build is, as stamped by the release workflow.
//!
//! Only the release workflow sets `SHELLRS_UPDATE_CHANNEL`, so a `cargo run`
//! build has no channel and never looks for updates: it is not something a
//! published package could replace. A release build starts on its own
//! channel; 设置 › 关于 › 更新渠道 switches it.

use serde::{Deserialize, Serialize};

/// This build's version, from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The commit the release was built from.
pub const COMMIT: Option<&str> = option_env!("SHELLRS_COMMIT");

/// The day the release was built, `YYYY-MM-DD`.
pub const BUILD_DATE: Option<&str> = option_env!("SHELLRS_BUILD_DATE");

/// The Apple team that signs ShellRS: a new bundle must come from it. Empty
/// or missing for a build signed ad hoc.
pub const APPLE_TEAM_ID: Option<&str> = option_env!("SHELLRS_APPLE_TEAM_ID");

/// Where each channel's signed manifest lives. Every ShellRS ever shipped
/// asks here, so the address never changes; a new manifest format gets a
/// new `schema`, not a new path.
pub const MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/dream10201/shell-rs/update/v1/{channel}.json";

/// The ShellRS website.
pub const WEBSITE: &str = "https://github.com/dream10201/shell-rs";

/// Where a person downloads ShellRS by hand.
pub const DOWNLOAD_PAGE: &str = "https://github.com/dream10201/shell-rs/releases/latest";

/// What each version changed, for the interface in `locale`: the Chinese
/// changelog for Chinese, the English one for the others.
pub fn changelog_page(locale: &str) -> &'static str {
    if locale == "zh-CN" {
        "https://github.com/dream10201/shell-rs/blob/main/CHANGELOG.zh.md"
    } else {
        "https://github.com/dream10201/shell-rs/blob/main/CHANGELOG.md"
    }
}

/// The minisign public keys a manifest may be signed with: the key in use,
/// and a spare kept offline so the first can be replaced. Rotating means
/// shipping a release that knows the new key before signing with it.
///
/// The release workflow refuses to build a release while this is empty: a
/// build without keys rejects every manifest.
pub const TRUSTED_KEYS: &[&str] = &[
    // In use: the release workflow signs with it (key id F6D01A9007CCCD5D).
    "RWRdzcwHkBrQ9lK3Rootkiwu/ursIlH+HHDURjfOnaMuFFEMS3NvIzHB",
    // Spare, kept offline (key id 74DFC7B0E38CCC88).
    "RWSIzIzjsMffdDdkCc29zBy6kWr1wmvwcr0pfCqBl2stIEIfTYoHgz4+",
];

/// Which stream of releases a copy follows. Beta also gets every stable
/// release that is not behind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Beta,
}

impl Channel {
    /// The name in the manifest's path and in its signature.
    pub fn key(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "stable" => Some(Self::Stable),
            "beta" => Some(Self::Beta),
            _ => None,
        }
    }

    /// The channel this build follows; `None` for a development build.
    pub fn of_this_build() -> Option<Self> {
        option_env!("SHELLRS_UPDATE_CHANNEL").and_then(Self::from_key)
    }

    pub fn manifest_url(self) -> String {
        MANIFEST_URL.replace("{channel}", self.key())
    }
}

/// This build's version, parsed.
pub fn version() -> semver::Version {
    semver::Version::parse(VERSION).expect("Cargo.toml holds a semver version")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_channel_has_a_manifest_of_its_own() {
        assert_eq!(
            Channel::Stable.manifest_url(),
            "https://raw.githubusercontent.com/dream10201/shell-rs/update/v1/stable.json"
        );
        assert_eq!(
            Channel::Beta.manifest_url(),
            "https://raw.githubusercontent.com/dream10201/shell-rs/update/v1/beta.json"
        );
        assert_eq!(Channel::from_key("nightly"), None);
    }

    #[test]
    fn the_package_version_parses() {
        assert_eq!(version().to_string(), VERSION);
    }
}
