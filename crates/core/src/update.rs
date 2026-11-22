//! Update Check: learning that a newer Hypatia release exists, and installing it on a click.
//!
//! - **Off means nothing**: every network entry point checks `Flags::update_check()` first and
//!   returns [`UpdateError::Disabled`] before any I/O. The player chooses once, on first launch,
//!   and can change it in the Performance sheet.
//! - **No identity**: the check is one `GET` to GitHub's public releases API with a generic
//!   User-Agent; nothing about the player, their account or their games is sent.
//! - **Explicit install**: a download only starts from the player's click. The release zip must
//!   carry a minisign signature by [`RELEASE_PUBLIC_KEY`] whose trusted comment names that exact
//!   tag and asset, so a tampered, renamed or replayed archive is refused before anything is
//!   written. Only then is `hypatia.exe` swapped (the running one renamed aside, which Windows
//!   allows) and the new one started.

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use hypatia_shared::Lang;
use serde_json::Value;

use crate::settings::Flags;

/// GitHub's description of the latest published (non-draft, non-prerelease) release.
pub const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/RubenVP2/hypatia/releases/latest";
/// Where every release lives; release pages must sit below it.
pub use hypatia_shared::links::HYPATIA_RELEASES as RELEASES_PAGE;
/// The Windows archive `release.yml` publishes. Renaming it breaks every installed updater.
pub const ARCHIVE_NAME: &str = "hypatia-windows-x86_64.zip";
/// The minisign signature of [`ARCHIVE_NAME`], published next to it.
pub const SIGNATURE_NAME: &str = "hypatia-windows-x86_64.zip.minisig";
/// The executable inside the archive.
pub const EXE_NAME: &str = "hypatia.exe";
/// Public half of the release signing key; `release.yml` signs with the secret half
/// (`MINISIGN_SECRET_KEY`). Rotating it means shipping a release signed by the old key first.
pub const RELEASE_PUBLIC_KEY: &str = "RWSf8L7IkDzI9TxR9cV2mloXM51lq/kH05iBqCUB4Gicu3SnKVgJboiq";
/// How often a running Hypatia asks again, on top of the check at launch.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(12 * 60 * 60);
/// How soon a check that could not reach GitHub tries again: Hypatia often starts with
/// Windows, before the network is up, and should not then wait a whole `CHECK_INTERVAL`.
pub const RETRY_INTERVAL: Duration = Duration::from_secs(10 * 60);

const DOWNLOAD_PREFIX: &str = "https://github.com/RubenVP2/hypatia/releases/download/";
const USER_AGENT: &str = "Hypatia-Updater";
const CHECK_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_SIGNATURE_BYTES: u64 = 4 * 1024;
const MAX_EXE_BYTES: u64 = 160 * 1024 * 1024;
const MAX_NOTES_CHARS: usize = 2400;

/// A release version (`vMAJOR.MINOR.PATCH`); pre-releases are never offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    /// Parses `v0.6.0` or `0.6.0`; `None` for anything else, pre-releases (`-rc.1`) included.
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        let raw = raw.strip_prefix('v').unwrap_or(raw);
        let mut parts = raw.split('.');
        let mut next = || -> Option<u64> {
            let part = parts.next()?;
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            part.parse().ok()
        };
        let version = Self {
            major: next()?,
            minor: next()?,
            patch: next()?,
        };
        parts.next().is_none().then_some(version)
    }

    /// The version this binary was built as.
    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Self {
            major: 0,
            minor: 0,
            patch: 0,
        })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "v{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// A downloadable file of a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub url: String,
    pub size: u64,
}

/// A published release newer than the running build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub tag: String,
    /// The release page, always below [`RELEASES_PAGE`].
    pub page_url: String,
    /// Release notes as plain text (from the `CHANGELOG.md` section), trimmed for the sheet.
    pub notes: String,
    pub archive: Option<Asset>,
    pub signature: Option<Asset>,
}

impl Release {
    /// One-click install is offered: a Windows build, and the release carries the signed zip.
    pub fn installable(&self) -> bool {
        cfg!(windows) && self.archive.is_some() && self.signature.is_some()
    }
}

/// Why a check or an install did not go through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateError {
    /// Update checks are switched off: nothing was sent.
    Disabled,
    Network,
    /// GitHub answered something that is not a release.
    BadResponse,
    /// This build or this release has no installable archive.
    Unsupported,
    TooLarge,
    /// The archive does not carry a valid signature by the release key for this tag.
    BadSignature,
    /// The signed archive holds no usable `hypatia.exe`.
    BadArchive,
    /// The folder Hypatia runs from cannot be written to.
    NotWritable,
}

impl UpdateError {
    /// A check that failed this way is worth retrying after [`RETRY_INTERVAL`].
    pub fn retry_soon(self) -> bool {
        matches!(self, Self::Network | Self::BadResponse)
    }

    pub fn message(self, lang: Lang) -> &'static str {
        match self {
            Self::Disabled => lang.pick(
                "Update checks are off.",
                "La vérification des mises à jour est désactivée.",
            ),
            Self::Network => lang.pick(
                "GitHub could not be reached. Try again later.",
                "GitHub est injoignable. Réessayez plus tard.",
            ),
            Self::BadResponse => lang.pick(
                "GitHub sent an unexpected answer. Try again later.",
                "GitHub a envoyé une réponse inattendue. Réessayez plus tard.",
            ),
            Self::Unsupported => lang.pick(
                "This update cannot be installed from here: download it from the release page.",
                "Cette mise à jour ne s'installe pas d'ici : téléchargez-la depuis la page de la version.",
            ),
            Self::TooLarge => lang.pick(
                "The download is larger than expected, so it was stopped.",
                "Le téléchargement est plus gros que prévu, il a été interrompu.",
            ),
            Self::BadSignature => lang.pick(
                "The download is not signed by Hypatia's release key, so nothing was installed.",
                "Le téléchargement n'est pas signé par la clé de publication d'Hypatia, rien n'a été installé.",
            ),
            Self::BadArchive => lang.pick(
                "The download does not contain Hypatia, so nothing was installed.",
                "Le téléchargement ne contient pas Hypatia, rien n'a été installé.",
            ),
            Self::NotWritable => lang.pick(
                "Hypatia's folder cannot be written to: download the update from the release page.",
                "Le dossier d'Hypatia n'est pas modifiable : téléchargez la mise à jour depuis la page de la version.",
            ),
        }
    }
}

/// Reads GitHub's `releases/latest` answer; `None` unless it is a published release newer than
/// `current`. Asset URLs outside this repository's download path are dropped.
pub fn newer_release(body: &Value, current: Version) -> Option<Release> {
    let release = parse_release(body)?;
    (release.version > current).then_some(release)
}

fn parse_release(body: &Value) -> Option<Release> {
    if body["draft"].as_bool().unwrap_or(false) || body["prerelease"].as_bool().unwrap_or(false) {
        return None;
    }
    let tag = body["tag_name"].as_str()?.to_string();
    let version = Version::parse(&tag)?;
    let page_url = body["html_url"]
        .as_str()
        .filter(|url| url.starts_with(&format!("{RELEASES_PAGE}/")))
        .map_or_else(|| RELEASES_PAGE.to_string(), str::to_string);
    let asset_prefix = format!("{DOWNLOAD_PREFIX}{tag}/");
    let asset = |name: &str| -> Option<Asset> {
        body["assets"].as_array()?.iter().find_map(|asset| {
            if asset["name"].as_str()? != name {
                return None;
            }
            let url = asset["browser_download_url"].as_str()?;
            (url == format!("{asset_prefix}{name}")).then(|| Asset {
                url: url.to_string(),
                size: asset["size"].as_u64().unwrap_or(0),
            })
        })
    };
    Some(Release {
        version,
        page_url,
        notes: plain_notes(body["body"].as_str().unwrap_or_default()),
        archive: asset(ARCHIVE_NAME),
        signature: asset(SIGNATURE_NAME),
        tag,
    })
}

/// Turns a `CHANGELOG.md` section into plain text: headings lose their `#`, links keep their
/// label, emphasis marks go; long notes are cut at a line end.
pub fn plain_notes(markdown: &str) -> String {
    let mut out = String::new();
    for line in markdown.lines() {
        let line = line.trim_end();
        let line = line.trim_start_matches('#').trim_start();
        let mut text = String::with_capacity(line.len());
        let mut rest = line;
        while let Some(open) = rest.find('[') {
            let after = &rest[open + 1..];
            let Some(close) = after.find("](") else {
                break;
            };
            let Some(end) = after[close + 2..].find(')') else {
                break;
            };
            text.push_str(&rest[..open]);
            text.push_str(&after[..close]);
            rest = &after[close + 2 + end + 1..];
        }
        text.push_str(rest);
        let text = text.replace(['*', '`'], "");
        if text.trim().is_empty() && out.ends_with("\n\n") {
            continue;
        }
        if out.len() + text.len() + 1 > MAX_NOTES_CHARS {
            out.push('…');
            break;
        }
        out.push_str(&text);
        out.push('\n');
    }
    out.trim().to_string()
}

/// Checks that `archive` carries `signature` by `public_key`, made for `tag`'s archive.
pub fn verify_archive(
    archive: &[u8],
    signature: &str,
    public_key: &str,
    tag: &str,
) -> Result<(), UpdateError> {
    let key = minisign_verify::PublicKey::from_base64(public_key)
        .map_err(|_| UpdateError::BadSignature)?;
    let signature =
        minisign_verify::Signature::decode(signature).map_err(|_| UpdateError::BadSignature)?;
    key.verify(archive, &signature, false)
        .map_err(|_| UpdateError::BadSignature)?;
    if signature.trusted_comment() != signed_comment(tag) {
        return Err(UpdateError::BadSignature);
    }
    Ok(())
}

/// The trusted comment `release.yml` signs with: binds the signature to one tag and asset.
pub fn signed_comment(tag: &str) -> String {
    format!("hypatia {tag} {ARCHIVE_NAME}")
}

/// Reads `hypatia.exe` out of the (already verified) release zip.
pub fn extract_exe(archive: &[u8]) -> Result<Vec<u8>, UpdateError> {
    let mut zip =
        zip::ZipArchive::new(Cursor::new(archive)).map_err(|_| UpdateError::BadArchive)?;
    let mut file = zip.by_name(EXE_NAME).map_err(|_| UpdateError::BadArchive)?;
    if file.size() > MAX_EXE_BYTES {
        return Err(UpdateError::TooLarge);
    }
    let mut exe = Vec::with_capacity(file.size() as usize);
    file.by_ref()
        .take(MAX_EXE_BYTES + 1)
        .read_to_end(&mut exe)
        .map_err(|_| UpdateError::BadArchive)?;
    if exe.len() as u64 > MAX_EXE_BYTES {
        return Err(UpdateError::TooLarge);
    }
    if !exe.starts_with(b"MZ") {
        return Err(UpdateError::BadArchive);
    }
    Ok(exe)
}

fn sibling(exe: &Path, suffix: &str) -> PathBuf {
    let mut name = exe.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    exe.with_file_name(name)
}

/// Where the replaced executable waits until the next launch removes it.
pub fn previous_exe_path(exe: &Path) -> PathBuf {
    sibling(exe, ".old")
}

/// Puts `new_exe` in place of `exe`: written next to it first, the running file renamed to
/// `.old`, then the new one renamed in. On failure the original stays (or is put back) in place.
pub fn install_exe(new_exe: &[u8], exe: &Path) -> Result<(), UpdateError> {
    let staged = sibling(exe, ".new");
    let previous = previous_exe_path(exe);
    std::fs::write(&staged, new_exe).map_err(|_| UpdateError::NotWritable)?;
    let _ = std::fs::remove_file(&previous);
    if std::fs::rename(exe, &previous).is_err() {
        let _ = std::fs::remove_file(&staged);
        return Err(UpdateError::NotWritable);
    }
    if std::fs::rename(&staged, exe).is_err() {
        let _ = std::fs::rename(&previous, exe);
        let _ = std::fs::remove_file(&staged);
        return Err(UpdateError::NotWritable);
    }
    Ok(())
}

/// Removes what an earlier install left beside `exe` (the replaced `.old`, an unfinished
/// `.new`). Local files only; the old file may still be closing, so failures are ignored.
pub fn cleanup_previous(exe: &Path) {
    for leftover in [previous_exe_path(exe), sibling(exe, ".new")] {
        if leftover.exists() {
            let _ = std::fs::remove_file(leftover);
        }
    }
}

/// Asks GitHub for the latest release and fetches its signed archive on the player's click.
pub struct Updater {
    flags: Arc<Flags>,
    endpoint: String,
    public_key: String,
    http: OnceLock<reqwest::Client>,
}

impl Updater {
    pub fn new(flags: Arc<Flags>) -> Self {
        Self {
            flags,
            endpoint: LATEST_RELEASE_API.to_string(),
            public_key: RELEASE_PUBLIC_KEY.to_string(),
            http: OnceLock::new(),
        }
    }

    /// Points the check at another endpoint (tests).
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    /// Trusts another signing key (tests).
    pub fn with_public_key(mut self, key: impl Into<String>) -> Self {
        self.public_key = key.into();
        self
    }

    fn http(&self) -> &reqwest::Client {
        self.http.get_or_init(|| {
            reqwest::Client::builder()
                .connect_timeout(CONNECT_TIMEOUT)
                .user_agent(USER_AGENT)
                .build()
                .unwrap_or_default()
        })
    }

    fn check_enabled(&self) -> Result<(), UpdateError> {
        if self.flags.update_check() {
            Ok(())
        } else {
            Err(UpdateError::Disabled)
        }
    }

    /// The latest release if it is newer than `current`, `None` when up to date.
    pub async fn check(&self, current: Version) -> Result<Option<Release>, UpdateError> {
        self.check_enabled()?;
        let response = self
            .http()
            .get(&self.endpoint)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .timeout(CHECK_TIMEOUT)
            .send()
            .await
            .map_err(|_| UpdateError::Network)?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            // No published release yet.
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(UpdateError::Network);
        }
        let body = read_capped(response, 1024 * 1024).await?;
        let body: Value = serde_json::from_slice(&body).map_err(|_| UpdateError::BadResponse)?;
        if body["tag_name"].as_str().is_none() {
            return Err(UpdateError::BadResponse);
        }
        Ok(newer_release(&body, current))
    }

    /// Downloads `release`'s archive and signature, verifies them and returns the new
    /// `hypatia.exe`. Nothing is written to disk.
    pub async fn download(&self, release: &Release) -> Result<Vec<u8>, UpdateError> {
        self.check_enabled()?;
        let (Some(archive), Some(signature)) = (&release.archive, &release.signature) else {
            return Err(UpdateError::Unsupported);
        };
        if archive.size > MAX_ARCHIVE_BYTES {
            return Err(UpdateError::TooLarge);
        }
        let signature = self.fetch(&signature.url, MAX_SIGNATURE_BYTES).await?;
        let signature = String::from_utf8(signature).map_err(|_| UpdateError::BadSignature)?;
        let archive = self.fetch(&archive.url, MAX_ARCHIVE_BYTES).await?;
        verify_archive(&archive, &signature, &self.public_key, &release.tag)?;
        extract_exe(&archive)
    }

    async fn fetch(&self, url: &str, cap: u64) -> Result<Vec<u8>, UpdateError> {
        let response = self
            .http()
            .get(url)
            .timeout(DOWNLOAD_TIMEOUT)
            .send()
            .await
            .map_err(|_| UpdateError::Network)?;
        if !response.status().is_success() {
            return Err(UpdateError::Network);
        }
        read_capped(response, cap).await
    }
}

async fn read_capped(mut response: reqwest::Response, cap: u64) -> Result<Vec<u8>, UpdateError> {
    if response.content_length().is_some_and(|len| len > cap) {
        return Err(UpdateError::TooLarge);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| UpdateError::Network)? {
        if (body.len() + chunk.len()) as u64 > cap {
            return Err(UpdateError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::settings::{Settings, UpdateCheck};

    const ARCHIVE: &[u8] = include_bytes!("../tests/data/update/hypatia-windows-x86_64.zip");
    const SIGNATURE: &str = include_str!("../tests/data/update/hypatia-windows-x86_64.zip.minisig");
    const NO_EXE: &[u8] = include_bytes!("../tests/data/update/no-exe.zip");
    const TEST_KEY: &str = "RWT5Pd9BcQaPRlz03IwsDhEGryqwDFl43BFZH+VcgdaBAOoIkVxcWZyp";

    fn v(raw: &str) -> Version {
        Version::parse(raw).unwrap()
    }

    fn flags(check: UpdateCheck) -> Arc<Flags> {
        Arc::new(Flags::new(&Settings {
            update_check: check,
            ..Settings::default()
        }))
    }

    fn github_release(tag: &str) -> Value {
        let base = format!("{DOWNLOAD_PREFIX}{tag}");
        json!({
            "tag_name": tag,
            "draft": false,
            "prerelease": false,
            "html_url": format!("{RELEASES_PAGE}/tag/{tag}"),
            "body": "### Added\n- *(update)* Check for new releases ([#24](https://github.com/RubenVP2/hypatia/pull/24))\n",
            "assets": [
                { "name": ARCHIVE_NAME, "size": 4_000_000, "browser_download_url": format!("{base}/{ARCHIVE_NAME}") },
                { "name": SIGNATURE_NAME, "size": 300, "browser_download_url": format!("{base}/{SIGNATURE_NAME}") },
            ],
        })
    }

    #[test]
    fn versions_parse_and_order_numerically() {
        assert_eq!(v("v0.10.0").to_string(), "v0.10.0");
        assert!(v("v0.10.0") > v("0.9.7"));
        assert!(v("1.0.0") > v("v0.99.99"));
        for bad in [
            "",
            "v1",
            "1.2",
            "1.2.3.4",
            "v1.2.3-rc.1",
            "1.2.x",
            "1..3",
            "+1.2.3",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
        assert_eq!(
            Version::current().to_string(),
            format!("v{}", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn only_a_newer_published_release_is_offered() {
        let body = github_release("v0.6.0");
        let release = newer_release(&body, v("0.5.0")).expect("newer release");
        assert_eq!(release.tag, "v0.6.0");
        assert_eq!(release.page_url, format!("{RELEASES_PAGE}/tag/v0.6.0"));
        assert_eq!(release.archive.as_ref().unwrap().size, 4_000_000);
        assert!(release.signature.is_some());
        assert_eq!(release.installable(), cfg!(windows));

        assert_eq!(newer_release(&body, v("0.6.0")), None, "same version");
        assert_eq!(newer_release(&body, v("0.7.0")), None, "never a downgrade");

        let mut draft = body.clone();
        draft["draft"] = json!(true);
        assert_eq!(newer_release(&draft, v("0.5.0")), None);
        let mut pre = body.clone();
        pre["prerelease"] = json!(true);
        assert_eq!(newer_release(&pre, v("0.5.0")), None);
        assert_eq!(
            newer_release(&github_release("v0.6.0-rc.1"), v("0.5.0")),
            None
        );
    }

    #[test]
    fn assets_and_pages_outside_the_repository_are_ignored() {
        let mut body = github_release("v0.6.0");
        body["html_url"] = json!("https://evil.example/hypatia");
        body["assets"][0]["browser_download_url"] =
            json!(format!("https://evil.example/{ARCHIVE_NAME}"));
        // An asset from another tag's folder is not this release's archive either.
        body["assets"][1]["browser_download_url"] =
            json!(format!("{DOWNLOAD_PREFIX}v0.1.0/{SIGNATURE_NAME}"));
        let release = newer_release(&body, v("0.5.0")).unwrap();
        assert_eq!(release.page_url, RELEASES_PAGE);
        assert_eq!(release.archive, None);
        assert_eq!(release.signature, None);
        assert!(!release.installable());
    }

    #[test]
    fn release_notes_read_as_plain_text() {
        let notes = plain_notes(
            "\n### Added\n- *(update)* Check for **new** releases ([#24](https://github.com/RubenVP2/hypatia/pull/24))\n\n\n### Fixed\n- `remote` timer\n",
        );
        assert_eq!(
            notes,
            "Added\n- (update) Check for new releases (#24)\n\nFixed\n- remote timer"
        );
        let long = "- line\n".repeat(1000);
        assert!(plain_notes(&long).chars().count() <= MAX_NOTES_CHARS + 1);
        assert!(plain_notes(&long).ends_with('…'));
    }

    #[test]
    fn a_signed_archive_verifies_only_for_its_own_tag_and_key() {
        assert_eq!(
            verify_archive(ARCHIVE, SIGNATURE, TEST_KEY, "v9.9.9"),
            Ok(())
        );
        // Replaying an older signed archive under another tag fails.
        assert_eq!(
            verify_archive(ARCHIVE, SIGNATURE, TEST_KEY, "v9.9.10"),
            Err(UpdateError::BadSignature)
        );
        // The production key did not sign the test archive.
        assert_eq!(
            verify_archive(ARCHIVE, SIGNATURE, RELEASE_PUBLIC_KEY, "v9.9.9"),
            Err(UpdateError::BadSignature)
        );
        let mut tampered = ARCHIVE.to_vec();
        let last = tampered.len() - 30;
        tampered[last] ^= 0x01;
        assert_eq!(
            verify_archive(&tampered, SIGNATURE, TEST_KEY, "v9.9.9"),
            Err(UpdateError::BadSignature)
        );
        assert_eq!(
            verify_archive(ARCHIVE, "not a signature", TEST_KEY, "v9.9.9"),
            Err(UpdateError::BadSignature)
        );
    }

    #[test]
    fn the_release_key_is_a_valid_minisign_key() {
        assert!(minisign_verify::PublicKey::from_base64(RELEASE_PUBLIC_KEY).is_ok());
    }

    #[test]
    fn the_executable_comes_out_of_the_archive() {
        let exe = extract_exe(ARCHIVE).unwrap();
        assert!(exe.starts_with(b"MZhypatia test build v9.9.9"));
        assert_eq!(extract_exe(NO_EXE), Err(UpdateError::BadArchive));
        assert_eq!(extract_exe(b"not a zip"), Err(UpdateError::BadArchive));
    }

    #[test]
    fn install_swaps_the_executable_and_cleanup_removes_the_old_one() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("hypatia.exe");
        std::fs::write(&exe, b"MZ old").unwrap();

        install_exe(b"MZ new", &exe).unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"MZ new");
        assert_eq!(std::fs::read(previous_exe_path(&exe)).unwrap(), b"MZ old");
        assert!(!dir.path().join("hypatia.exe.new").exists());

        cleanup_previous(&exe);
        assert!(!previous_exe_path(&exe).exists());
        assert_eq!(std::fs::read(&exe).unwrap(), b"MZ new");
    }

    #[test]
    fn install_leaves_the_original_in_place_when_it_cannot_swap() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("gone").join("hypatia.exe");
        assert_eq!(
            install_exe(b"MZ new", &missing),
            Err(UpdateError::NotWritable)
        );
    }

    #[test]
    fn only_a_failure_to_reach_github_is_retried_soon() {
        assert!(UpdateError::Network.retry_soon());
        assert!(UpdateError::BadResponse.retry_soon());
        assert!(!UpdateError::Disabled.retry_soon());
        assert!(RETRY_INTERVAL < CHECK_INTERVAL);
    }

    #[tokio::test]
    async fn unreachable_github_is_a_network_error() {
        let updater = Updater::new(flags(UpdateCheck::On)).with_endpoint("http://127.0.0.1:9/");
        assert_eq!(updater.check(v("0.5.0")).await, Err(UpdateError::Network));
    }

    #[tokio::test]
    async fn switched_off_nothing_is_requested() {
        // An endpoint that would fail loudly if contacted.
        let updater = Updater::new(flags(UpdateCheck::Off)).with_endpoint("http://127.0.0.1:9/");
        assert_eq!(updater.check(v("0.5.0")).await, Err(UpdateError::Disabled));
        let release = newer_release(&github_release("v0.6.0"), v("0.5.0")).unwrap();
        assert_eq!(updater.download(&release).await, Err(UpdateError::Disabled));

        let unanswered = Updater::new(flags(UpdateCheck::Ask)).with_endpoint("http://127.0.0.1:9/");
        assert_eq!(
            unanswered.check(v("0.5.0")).await,
            Err(UpdateError::Disabled)
        );
    }

    /// Serves `routes` (path → body) on loopback; returns the base URL.
    async fn serve(routes: Vec<(String, Vec<u8>)>) -> String {
        use axum::{routing::get, Router};
        let mut router = Router::new();
        for (path, body) in routes {
            router = router.route(&path, get(move || async move { body.clone() }));
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        format!("http://{addr}")
    }

    fn local_release(base: &str, tag: &str) -> Release {
        Release {
            version: v(tag),
            tag: tag.to_string(),
            page_url: RELEASES_PAGE.to_string(),
            notes: String::new(),
            archive: Some(Asset {
                url: format!("{base}/archive"),
                size: ARCHIVE.len() as u64,
            }),
            signature: Some(Asset {
                url: format!("{base}/signature"),
                size: SIGNATURE.len() as u64,
            }),
        }
    }

    #[tokio::test]
    async fn check_then_download_a_signed_release() {
        let latest = serde_json::to_vec(&github_release("v0.6.0")).unwrap();
        let base = serve(vec![
            ("/latest".into(), latest),
            ("/archive".into(), ARCHIVE.to_vec()),
            ("/signature".into(), SIGNATURE.as_bytes().to_vec()),
        ])
        .await;
        let updater = Updater::new(flags(UpdateCheck::On))
            .with_endpoint(format!("{base}/latest"))
            .with_public_key(TEST_KEY);

        let found = updater.check(v("0.5.0")).await.unwrap().unwrap();
        assert_eq!(found.version, v("0.6.0"));
        assert_eq!(updater.check(v("0.6.0")).await, Ok(None));

        let exe = updater
            .download(&local_release(&base, "v9.9.9"))
            .await
            .unwrap();
        assert!(exe.starts_with(b"MZ"));
        // Same files offered as another version: the signature names v9.9.9 only.
        assert_eq!(
            updater.download(&local_release(&base, "v9.9.10")).await,
            Err(UpdateError::BadSignature)
        );
    }
}
