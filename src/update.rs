//! Automatic updates from GitHub Releases.
//!
//! The running app checks the repository's latest release a few seconds after it starts
//! and every few hours after that. When there's a newer version it downloads the file for
//! this computer, checks it against the release's `SHA256SUMS.txt`, and keeps it ready.
//! It's installed when the user clicks "Restart to update", or by itself once COAT has been
//! idle for a while (installing restarts COAT, and reports live in memory, so it never
//! happens in the middle of someone's work).
//!
//! Installing swaps the program on disk and starts the new version, which takes over the
//! web UI's port:
//! - macOS: the whole `COAT.app` folder is renamed aside and the new one moved in.
//! - Windows: the running `COAT.exe` is renamed aside (Windows allows that) and the new one
//!   moved in.
//!
//! The new version removes the old copy when it starts. Only official release builds
//! update themselves; a developer's own build reports updates as disabled.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The GitHub repository releases come from. Set by the release workflow; forks get their own.
pub const REPO: &str = match option_env!("COAT_REPO") {
    Some(repo) => repo,
    None => "archways404/TVX-SOC-COAT",
};

/// Only builds made by the release workflow replace themselves.
pub const RELEASE_BUILD: bool = option_env!("COAT_RELEASE_BUILD").is_some();

const FIRST_CHECK_AFTER: Duration = Duration::from_secs(8);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// A downloaded update installs by itself once COAT has had no requests for this long.
pub const IDLE_BEFORE_INSTALL: Duration = Duration::from_secs(30 * 60);
/// Override for the "latest release" API URL (used by the update test).
const UPDATE_URL_ENV: &str = "COAT_UPDATE_URL";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Status {
    /// This build doesn't update itself.
    Disabled { reason: String },
    Idle,
    Checking,
    UpToDate { checked: String },
    Available { version: String, notes_url: String, installable: bool, why_not: String },
    Downloading { version: String },
    Ready { version: String, notes_url: String },
    Installing { version: String },
    Failed { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Settings {
    auto_update: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { auto_update: true }
    }
}

#[derive(Debug, Clone)]
struct Release {
    version: String,
    notes_url: String,
    asset_url: String,
    sums_url: String,
}

/// Where an update would be installed, or why it can't be.
#[derive(Debug, Clone)]
enum Target {
    MacApp { bundle: PathBuf },
    WindowsExe { exe: PathBuf },
    Unsupported(String),
}

pub struct Updater {
    status: Mutex<Status>,
    settings: Mutex<Settings>,
    release: Mutex<Option<Release>>,
    staged: Mutex<Option<PathBuf>>,
    busy: Mutex<()>,
}

impl Updater {
    pub fn new() -> Arc<Updater> {
        let status = if RELEASE_BUILD {
            Status::Idle
        } else {
            Status::Disabled { reason: "development build".into() }
        };
        Arc::new(Updater {
            status: Mutex::new(status),
            settings: Mutex::new(load_settings()),
            release: Mutex::new(None),
            staged: Mutex::new(None),
            busy: Mutex::new(()),
        })
    }

    pub fn enabled(&self) -> bool {
        !matches!(*self.status.lock().unwrap(), Status::Disabled { .. })
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    pub fn status_json(&self) -> Value {
        let mut value = serde_json::to_value(self.status()).unwrap_or(Value::Null);
        if let Some(object) = value.as_object_mut() {
            object.insert("current".into(), json!(VERSION));
            object.insert("auto".into(), json!(self.settings.lock().unwrap().auto_update));
            object.insert("releases_url".into(), json!(format!("https://github.com/{REPO}/releases")));
        }
        value
    }

    pub fn set_auto(&self, on: bool) {
        let mut settings = self.settings.lock().unwrap();
        settings.auto_update = on;
        save_settings(&settings);
    }

    fn auto(&self) -> bool {
        self.settings.lock().unwrap().auto_update
    }

    fn set_status(&self, status: Status) {
        *self.status.lock().unwrap() = status;
    }

    /// Background loop: first check soon after start, then every few hours. A downloaded
    /// update installs itself when `is_idle` says nobody is using COAT.
    pub fn run_in_background(self: &Arc<Self>, is_idle: impl Fn() -> bool + Send + 'static) {
        if !self.enabled() {
            return;
        }
        let updater = self.clone();
        thread::spawn(move || {
            thread::sleep(FIRST_CHECK_AFTER);
            let mut last_check: Option<Instant> = None;
            loop {
                if last_check.is_none_or(|at| at.elapsed() >= CHECK_EVERY) {
                    updater.check();
                    last_check = Some(Instant::now());
                }
                let ready = matches!(updater.status(), Status::Ready { .. });
                if ready && updater.auto() && is_idle() {
                    updater.install();
                }
                thread::sleep(Duration::from_secs(60));
            }
        });
    }

    /// Look for a newer release; download it right away if updates are automatic.
    pub fn check(&self) {
        let Ok(_guard) = self.busy.try_lock() else { return };
        if !self.enabled() || matches!(self.status(), Status::Ready { .. } | Status::Installing { .. }) {
            return;
        }
        self.set_status(Status::Checking);
        let release = match latest_release() {
            Ok(release) => release,
            Err(message) => return self.set_status(Status::Failed { message }),
        };
        if !is_newer(&release.version, VERSION) {
            *self.release.lock().unwrap() = None;
            return self.set_status(Status::UpToDate { checked: Local::now().format("%H:%M").to_string() });
        }
        let target = install_target();
        let why_not = match &target {
            Target::Unsupported(reason) => reason.clone(),
            _ => String::new(),
        };
        *self.release.lock().unwrap() = Some(release.clone());
        self.set_status(Status::Available {
            version: release.version.clone(), notes_url: release.notes_url.clone(),
            installable: why_not.is_empty(), why_not: why_not.clone(),
        });
        if why_not.is_empty() && self.auto() {
            self.download_locked(&release, &target);
        }
    }

    /// Download (if needed) and install, then restart. Only returns if something failed.
    pub fn install(&self) {
        let Ok(_guard) = self.busy.try_lock() else { return };
        let Some(release) = self.release.lock().unwrap().clone() else { return };
        let target = install_target();
        if let Target::Unsupported(reason) = &target {
            return self.set_status(Status::Failed { message: format!("Can't install here: {reason}") });
        }
        if self.staged.lock().unwrap().is_none() {
            self.download_locked(&release, &target);
        }
        let Some(staged) = self.staged.lock().unwrap().clone() else { return };
        self.set_status(Status::Installing { version: release.version.clone() });
        if let Err(message) = apply(&staged, &target) {
            *self.staged.lock().unwrap() = None;
            self.set_status(Status::Failed { message });
        }
    }

    fn download_locked(&self, release: &Release, target: &Target) {
        self.set_status(Status::Downloading { version: release.version.clone() });
        match stage(release, target) {
            Ok(staged) => {
                *self.staged.lock().unwrap() = Some(staged);
                self.set_status(Status::Ready { version: release.version.clone(), notes_url: release.notes_url.clone() });
            }
            Err(message) => self.set_status(Status::Failed { message }),
        }
    }
}

// ---- talking to GitHub ------------------------------------------------------------------------

fn agent(timeout: Duration) -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    let config = ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_global(Some(timeout))
        .http_status_as_error(true)
        .user_agent(format!("COAT/{VERSION}"))
        .build();
    ureq::Agent::new_with_config(config)
}

/// Only HTTPS, or plain HTTP to this computer (for the update test).
fn allowed_url(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://127.0.0.1:") || url.starts_with("http://localhost:")
}

fn latest_release() -> Result<Release, String> {
    let url = std::env::var(UPDATE_URL_ENV)
        .unwrap_or_else(|_| format!("https://api.github.com/repos/{REPO}/releases/latest"));
    if !allowed_url(&url) {
        return Err(format!("refusing to check for updates over an insecure address: {url}"));
    }
    let body = agent(Duration::from_secs(30))
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .and_then(|mut response| response.body_mut().read_to_string())
        .map_err(|e| match e {
            ureq::Error::StatusCode(404) => "no releases published yet".to_string(),
            ureq::Error::StatusCode(code) => format!("GitHub answered HTTP {code}"),
            other => format!("couldn't reach GitHub ({other})"),
        })?;
    let json: Value = serde_json::from_str(&body).map_err(|e| format!("unexpected answer from GitHub: {e}"))?;
    parse_release(&json, asset_name())
}

fn parse_release(json: &Value, asset: &str) -> Result<Release, String> {
    let tag = json["tag_name"].as_str().ok_or("release without a tag")?;
    let version = tag.trim_start_matches('v').to_string();
    parse_version(&version).ok_or_else(|| format!("release tag {tag} isn't a version"))?;
    let find = |name: &str| {
        json["assets"].as_array().into_iter().flatten()
            .find(|a| a["name"] == name)
            .and_then(|a| a["browser_download_url"].as_str())
            .map(str::to_string)
    };
    let asset_url = find(asset).ok_or_else(|| format!("release {tag} has no {asset}"))?;
    let sums_url = find("SHA256SUMS.txt").ok_or_else(|| format!("release {tag} has no SHA256SUMS.txt"))?;
    if !allowed_url(&asset_url) || !allowed_url(&sums_url) {
        return Err("release files aren't served over HTTPS".into());
    }
    Ok(Release {
        version,
        notes_url: json["html_url"].as_str().unwrap_or("").to_string(),
        asset_url,
        sums_url,
    })
}

fn asset_name() -> &'static str {
    if cfg!(target_os = "macos") { "COAT-macOS.zip" } else if cfg!(windows) { "COAT.exe" } else { "unsupported" }
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    let mut response = agent(Duration::from_secs(300)).get(url).call().map_err(|e| format!("download failed ({e})"))?;
    let mut bytes = Vec::new();
    response.body_mut().with_config().limit(400 * 1024 * 1024).reader()
        .read_to_end(&mut bytes).map_err(|e| format!("download failed ({e})"))?;
    Ok(bytes)
}

/// `sha256sum` format: `<hex>  <file name>` per line.
fn expected_sha256(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == file).then(|| hash.trim().to_lowercase())
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

// ---- versions ---------------------------------------------------------------------------------

pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.trim().trim_start_matches('v').split('.').map(|p| p.parse::<u64>().ok());
    let version = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(version)
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => false,
    }
}

// ---- installing -------------------------------------------------------------------------------

fn install_target() -> Target {
    if !RELEASE_BUILD {
        return Target::Unsupported("this is a development build".into());
    }
    let Some(exe) = installed_exe() else {
        return Target::Unsupported("can't find where COAT is installed".into());
    };
    if cfg!(target_os = "macos") {
        let Some(bundle) = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")) else {
            return Target::Unsupported("COAT isn't running from COAT.app".into());
        };
        if bundle.to_string_lossy().contains("/AppTranslocation/") {
            return Target::Unsupported("move COAT into your Applications folder first".into());
        }
        let parent = bundle.parent().unwrap_or(Path::new("/"));
        if !writable(parent) {
            return Target::Unsupported(format!("no permission to replace COAT in {}", parent.display()));
        }
        return Target::MacApp { bundle: bundle.to_path_buf() };
    }
    if cfg!(windows) {
        let parent = exe.parent().unwrap_or(Path::new("."));
        if !writable(parent) {
            return Target::Unsupported(format!("no permission to replace COAT in {}", parent.display()));
        }
        return Target::WindowsExe { exe };
    }
    Target::Unsupported("automatic updates are only available on Mac and Windows".into())
}

/// This program's path. On macOS with symlinks resolved (e.g. an alias in Applications); on
/// Windows as is, because resolving gives `\\?\C:\…` paths that some tools don't accept.
fn installed_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    if cfg!(windows) { Some(exe) } else { exe.canonicalize().ok() }
}

fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".coat-write-test-{}", std::process::id()));
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

fn mac_staging_dir(bundle: &Path) -> PathBuf {
    bundle.with_file_name(".COAT-update")
}

fn mac_old_bundle(bundle: &Path) -> PathBuf {
    bundle.with_file_name(".COAT-old.app")
}

fn windows_sibling(exe: &Path, suffix: &str) -> PathBuf {
    let stem = exe.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "COAT".into());
    exe.with_file_name(format!("{stem}.{suffix}.exe"))
}

/// Download and verify the release, and unpack it next to the installed copy (same disk,
/// so the final swap is an instant rename). Returns the path of the new program.
fn stage(release: &Release, target: &Target) -> Result<PathBuf, String> {
    let sums = String::from_utf8_lossy(&download(&release.sums_url)?).into_owned();
    let expected = expected_sha256(&sums, asset_name()).ok_or("SHA256SUMS.txt doesn't list this computer's file")?;
    let bytes = download(&release.asset_url)?;
    let actual = sha256_hex(&bytes);
    if actual != expected {
        return Err(format!("the download is damaged (checksum {actual}, expected {expected})"));
    }
    let staged = match target {
        Target::MacApp { bundle } => {
            let dir = mac_staging_dir(bundle);
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).map_err(|e| format!("can't prepare the update: {e}"))?;
            let zip = dir.join(asset_name());
            std::fs::write(&zip, &bytes).map_err(|e| format!("can't save the update: {e}"))?;
            let status = std::process::Command::new("ditto").arg("-x").arg("-k").arg(&zip).arg(&dir).status()
                .map_err(|e| format!("can't unpack the update: {e}"))?;
            if !status.success() {
                return Err("can't unpack the update".into());
            }
            let _ = std::fs::remove_file(&zip);
            dir.join("COAT.app")
        }
        Target::WindowsExe { exe } => {
            let new = windows_sibling(exe, "new");
            std::fs::write(&new, &bytes).map_err(|e| format!("can't save the update: {e}"))?;
            new
        }
        Target::Unsupported(reason) => return Err(reason.clone()),
    };
    let program = staged_program(&staged);
    let output = std::process::Command::new(&program).arg("--version").output()
        .map_err(|e| format!("the downloaded COAT doesn't start: {e}"))?;
    let reported = String::from_utf8_lossy(&output.stdout);
    if !reported.contains(&release.version) {
        return Err(format!("the downloaded COAT reports {reported:?}, expected {}", release.version));
    }
    Ok(staged)
}

fn staged_program(staged: &Path) -> PathBuf {
    if staged.extension().is_some_and(|e| e == "app") { staged.join("Contents/MacOS/coat") } else { staged.to_path_buf() }
}

/// Swap the new version in and start it. Exits this process on success.
fn apply(staged: &Path, target: &Target) -> Result<(), String> {
    let (installed, aside) = match target {
        Target::MacApp { bundle } => (bundle.clone(), mac_old_bundle(bundle)),
        Target::WindowsExe { exe } => (exe.clone(), windows_sibling(exe, "old")),
        Target::Unsupported(reason) => return Err(reason.clone()),
    };
    let _ = if aside.is_dir() { std::fs::remove_dir_all(&aside) } else { std::fs::remove_file(&aside) };
    std::fs::rename(&installed, &aside).map_err(|e| format!("can't move the old COAT aside: {e}"))?;
    if let Err(error) = std::fs::rename(staged, &installed) {
        let _ = std::fs::rename(&aside, &installed);
        return Err(format!("can't move the new COAT into place: {error}"));
    }
    crate::app::spawn_background(&staged_program(&installed), &[("COAT_TAKEOVER", "1")])
        .map_err(|e| format!("installed, but couldn't restart: {e}. Open COAT again."))?;
    thread::sleep(Duration::from_millis(200));
    std::process::exit(0);
}

/// Called when COAT starts: remove what the previous version left behind after an update.
pub fn clean_up_after_update() {
    let Some(exe) = installed_exe() else { return };
    let leftovers: Vec<PathBuf> = if cfg!(target_os = "macos") {
        exe.ancestors()
            .find(|p| p.extension().is_some_and(|e| e == "app"))
            .map(|bundle| vec![mac_old_bundle(bundle), mac_staging_dir(bundle)])
            .unwrap_or_default()
    } else if cfg!(windows) {
        vec![windows_sibling(&exe, "old"), windows_sibling(&exe, "new")]
    } else {
        vec![]
    };
    if leftovers.iter().all(|p| !p.exists()) {
        return;
    }
    // The previous version may still be exiting and holding its files: retry for a while.
    thread::spawn(move || {
        for _ in 0..20 {
            let mut remaining = false;
            for path in &leftovers {
                let removed = if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
                remaining |= removed.is_err() && path.exists();
            }
            if !remaining {
                return;
            }
            thread::sleep(Duration::from_millis(500));
        }
    });
}

// ---- settings ---------------------------------------------------------------------------------

/// `COAT_SETTINGS_DIR`, else the usual per-user settings folder of the operating system.
fn settings_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("COAT_SETTINGS_DIR") {
        return Some(PathBuf::from(dir).join("settings.json"));
    }
    let base = if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    base.map(|b| b.join("COAT").join("settings.json"))
}

fn load_settings() -> Settings {
    settings_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_settings(settings: &Settings) {
    let Some(path) = settings_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serde_json::to_string_pretty(settings).unwrap_or_default());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("0.2.10", "0.2.9"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.9", "0.2.0"));
        assert!(!is_newer("nonsense", "0.2.0"));
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
    }

    #[test]
    fn release_json_picks_this_computers_file() {
        let json = json!({
            "tag_name": "v0.3.1",
            "html_url": "https://github.com/o/r/releases/tag/v0.3.1",
            "assets": [
                {"name": "COAT-macOS.zip", "browser_download_url": "https://example.test/COAT-macOS.zip"},
                {"name": "COAT.exe", "browser_download_url": "https://example.test/COAT.exe"},
                {"name": "SHA256SUMS.txt", "browser_download_url": "https://example.test/SHA256SUMS.txt"}
            ]
        });
        let release = parse_release(&json, "COAT.exe").unwrap();
        assert_eq!(release.version, "0.3.1");
        assert_eq!(release.asset_url, "https://example.test/COAT.exe");
        assert_eq!(release.sums_url, "https://example.test/SHA256SUMS.txt");
        assert!(parse_release(&json, "COAT-linux.tar.gz").is_err());
    }

    #[test]
    fn insecure_release_files_are_refused() {
        let json = json!({"tag_name": "v1.0.0", "assets": [
            {"name": "COAT.exe", "browser_download_url": "http://evil.test/COAT.exe"},
            {"name": "SHA256SUMS.txt", "browser_download_url": "http://evil.test/SHA256SUMS.txt"}]});
        assert!(parse_release(&json, "COAT.exe").is_err());
        assert!(allowed_url("http://127.0.0.1:8765/COAT.exe"));
        assert!(!allowed_url("http://example.test/COAT.exe"));
    }

    #[test]
    fn checksums_file_is_read_like_sha256sum_writes_it() {
        let sums = "abc123  COAT-macOS.zip\nDEF456 *COAT.exe\n";
        assert_eq!(expected_sha256(sums, "COAT.exe").as_deref(), Some("def456"));
        assert_eq!(expected_sha256(sums, "COAT-macOS.zip").as_deref(), Some("abc123"));
        assert_eq!(expected_sha256(sums, "other"), None);
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
