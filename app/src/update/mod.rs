//! Updates itself from GitHub Releases. likh looks for a newer release (alpha
//! ones included) at launch and after a dictation, at most once a day, so it
//! never wakes up just to check. A newer Windows installer is downloaded in
//! the background and checked against its signature; the user picks "Restart
//! to update", likh runs it silently and the installer opens the new likh.
//!
//! Signatures: every release carries `likh-setup-windows-x64.exe.sig`, an
//! Ed25519 signature of the installer made in the release workflow with the
//! `LIKH_UPDATE_SIGNING_KEY` secret. likh only runs an installer that matches
//! `PUBLIC_KEY`, so a swapped download never runs, even though the builds
//! themselves aren't code-signed.
//!
//! macOS has no installer yet: there likh only says a new version exists and
//! links to it.
//!
//! `LIKH_UPDATE_FEED=<url>` reads releases from somewhere else (a local server
//! in tests) instead of GitHub.

use crate::tray::Pick;
use iced::futures::channel::mpsc::UnboundedSender;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(test)]
mod tests;

const RELEASES: &str = "https://api.github.com/repos/kausthubh-coder/likh/releases?per_page=20";
pub const INSTALLER: &str = "likh-setup-windows-x64.exe";
const SIGNATURE: &str = "likh-setup-windows-x64.exe.sig";
/// The public half of the release signing key (Ed25519, 32 bytes, base64).
const PUBLIC_KEY: &str = "osDHDUqqWaXVcDKgFNWND4CK6itGRVYnsSmhh18oJN0=";
/// Automatic checks happen at most this often.
pub const EVERY: Duration = Duration::from_secs(24 * 60 * 60);
/// Installers are a few MB; anything this big isn't ours.
const LARGEST_DOWNLOAD: u64 = 64 * 1024 * 1024;
/// Can likh install its own updates here? Only Windows has an installer.
pub const INSTALLS: bool = cfg!(target_os = "windows");

/// A release newer than the running likh.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Release {
    pub version: String,
    pub installer: String,
    pub signature: String,
    /// The release's page on GitHub, for macOS and for reading what changed.
    pub page: String,
}

/// Where updating stands. Shown in About and the tray; `likh ctl status` reports it too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum State {
    /// Nothing checked since likh started.
    #[default]
    Idle,
    Checking,
    UpToDate {
        checked_at: u64,
    },
    Downloading {
        version: String,
    },
    /// Downloaded and verified: "Restart to update" runs it.
    Ready {
        version: String,
        installer: PathBuf,
    },
    /// A newer version exists but this OS can't install it (macOS): link to it.
    Available {
        version: String,
        page: String,
    },
    Failed {
        reason: String,
    },
}

/// The running likh's updater: cheap to clone, every clone shares one state.
#[derive(Clone)]
pub struct Updates {
    state: Arc<Mutex<State>>,
    /// Told whenever the state changes, so the tray and the likh window refresh.
    ui: Option<UnboundedSender<Pick>>,
}

impl Updates {
    pub fn new(ui: Option<UnboundedSender<Pick>>) -> Self {
        Self { state: Arc::new(Mutex::new(State::Idle)), ui }
    }

    /// An updater already in `state`, for view tests.
    #[cfg(test)]
    pub fn with_state(state: State) -> Self {
        Self { state: Arc::new(Mutex::new(state)), ui: None }
    }

    pub fn state(&self) -> State {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
    }

    fn set(&self, state: State) {
        *self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = state;
        if let Some(ui) = &self.ui {
            let _ = ui.unbounded_send(Pick::UpdateChanged);
        }
    }

    /// Checks in the background if the last check was over a day ago: at launch,
    /// and after each dictation, so an idle likh never wakes up to do it.
    pub fn check_if_due(&self) {
        let last = last_checked().map(|seconds| UNIX_EPOCH + Duration::from_secs(seconds));
        if due(last, SystemTime::now()) {
            self.check_now();
        }
    }

    /// Checks in the background now ("Check now" in About). Does nothing if a
    /// check or download is already running, or an update is waiting.
    pub fn check_now(&self) {
        {
            let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if matches!(*state, State::Checking | State::Downloading { .. } | State::Ready { .. }) {
                return;
            }
            *state = State::Checking;
        }
        if let Some(ui) = &self.ui {
            let _ = ui.unbounded_send(Pick::UpdateChanged);
        }
        let updates = self.clone();
        let spawned = std::thread::Builder::new().name("likh-update".into()).spawn(move || {
            let state = updates.check_and_download();
            updates.set(state);
        });
        if let Err(error) = spawned {
            self.set(State::Failed { reason: error.to_string() });
        }
    }

    fn check_and_download(&self) -> State {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let feed = std::env::var("LIKH_UPDATE_FEED").unwrap_or_else(|_| RELEASES.to_string());
        let answer = get_text(&feed).and_then(|json| newest(&json, &current()));
        let release = match answer {
            Ok(Some(release)) => release,
            Ok(None) => {
                remember_checked(now);
                return State::UpToDate { checked_at: now };
            }
            Err(reason) => return State::Failed { reason },
        };
        remember_checked(now);
        if !INSTALLS {
            return State::Available { version: release.version, page: release.page };
        }
        self.set(State::Downloading { version: release.version.clone() });
        match download(&release) {
            Ok(installer) => State::Ready { version: release.version, installer },
            Err(reason) => State::Failed { reason },
        }
    }

    /// "Restart to update": runs the verified installer silently and quits;
    /// the installer opens the new likh when it's done.
    pub fn apply(&self) -> Result<(), String> {
        let State::Ready { installer, .. } = self.state() else {
            return Err("No update is ready yet.".into());
        };
        std::process::Command::new(&installer)
            .args(installer_args())
            .spawn()
            .map_err(|error| format!("couldn't start the installer: {error}"))?;
        // The installer replaces likh.exe, so likh has to be gone first.
        std::process::exit(0);
    }
}

/// Silent, no reboot, and open likh again afterwards (`relaunch` is read by installer/windows.iss).
pub fn installer_args() -> [&'static str; 4] {
    ["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/relaunch=1"]
}

/// The version of the running likh.
pub fn current() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or_else(|_| Version::new(0, 0, 0))
}

/// Whether an automatic check is due.
pub fn due(last: Option<SystemTime>, now: SystemTime) -> bool {
    last.is_none_or(|last| now.duration_since(last).is_ok_and(|since| since >= EVERY))
}

/// The newest release in GitHub's release list that is newer than `current`
/// and has a signed Windows installer. Drafts and unparseable tags are skipped.
pub fn newest(json: &str, current: &Version) -> Result<Option<Release>, String> {
    #[derive(Deserialize)]
    struct GitHubRelease {
        tag_name: String,
        #[serde(default)]
        draft: bool,
        html_url: String,
        #[serde(default)]
        assets: Vec<Asset>,
    }
    #[derive(Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
    }
    let releases: Vec<GitHubRelease> =
        serde_json::from_str(json).map_err(|_| "GitHub sent back a release list likh can't read.".to_string())?;
    let asset = |release: &GitHubRelease, name: &str| {
        release.assets.iter().find(|asset| asset.name == name).map(|asset| asset.browser_download_url.clone())
    };
    let newest = releases
        .iter()
        .filter(|release| !release.draft)
        .filter_map(|release| {
            let version = Version::parse(release.tag_name.trim_start_matches('v')).ok()?;
            let installer = asset(release, INSTALLER)?;
            let signature = asset(release, SIGNATURE)?;
            Some((version, Release { version: String::new(), installer, signature, page: release.html_url.clone() }))
        })
        .filter(|(version, _)| version > current)
        .max_by(|(a, _), (b, _)| a.cmp(b));
    Ok(newest.map(|(version, release)| Release { version: version.to_string(), ..release }))
}

/// True if `signature` (base64) is the release key's Ed25519 signature of `bytes`.
pub fn verify(bytes: &[u8], signature: &str, public_key: &[u8]) -> bool {
    use base64::Engine;
    let Ok(signature) = base64::engine::general_purpose::STANDARD.decode(signature.trim()) else { return false };
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public_key).verify(bytes, &signature).is_ok()
}

fn public_key() -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(PUBLIC_KEY).unwrap_or_default()
}

/// Downloads the installer and its signature, and keeps the installer only if they match.
fn download(release: &Release) -> Result<PathBuf, String> {
    let signature = get_text(&release.signature)?;
    let installer = get_bytes(&release.installer)?;
    if !verify(&installer, &signature, &public_key()) {
        return Err(format!("The {} download didn't match its signature, so likh threw it away.", release.version));
    }
    let folder = crate::settings::folder().ok_or("there is no local data folder")?.join("updates");
    // Only the newest download is ever needed.
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let path = folder.join(format!("likh-setup-{}.exe", release.version));
    std::fs::write(&path, &installer).map_err(|error| error.to_string())?;
    Ok(path)
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .user_agent(concat!("likh/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn get_text(url: &str) -> Result<String, String> {
    agent()
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|error| format!("Couldn't reach GitHub to check for updates ({error})."))?
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())
}

fn get_bytes(url: &str) -> Result<Vec<u8>, String> {
    agent()
        .get(url)
        .call()
        .map_err(|error| format!("Couldn't download the update ({error})."))?
        .body_mut()
        .with_config()
        .limit(LARGEST_DOWNLOAD)
        .read_to_vec()
        .map_err(|error| error.to_string())
}

/// When likh last checked, in seconds since 1970, kept beside the settings so
/// restarting likh doesn't check again.
fn last_checked() -> Option<u64> {
    let file = crate::settings::folder()?.join("update-checked");
    std::fs::read_to_string(file).ok()?.trim().parse().ok()
}

fn remember_checked(now: u64) {
    if let Some(folder) = crate::settings::folder() {
        let _ = std::fs::create_dir_all(&folder);
        let _ = std::fs::write(folder.join("update-checked"), now.to_string());
    }
}
