//! Code that two or more features really use. Nothing goes here speculatively.

pub mod icon;
pub mod spring;
#[cfg(test)]
pub mod test_ui;
pub mod theme;
pub mod ui;

/// likh's website, and its guide to installing Codex and signing in.
pub const SITE: &str = "https://likh.kausthubh.dev";
pub const CODEX_SETUP: &str = "https://likh.kausthubh.dev/codex";

/// The system's own page for the microphone permission.
pub const MIC_SETTINGS: &str = if cfg!(target_os = "macos") {
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
} else {
    "ms-settings:privacy-microphone"
};
/// macOS: the Accessibility list likh must be on to hear its keys and paste.
#[cfg_attr(not(target_os = "macos"), allow(dead_code, reason = "only macOS asks for Accessibility"))]
pub const ACCESSIBILITY_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";
/// Where the system's settings are, as people call it.
pub const SYSTEM_SETTINGS: &str = if cfg!(target_os = "macos") { "System Settings" } else { "Windows Settings" };

/// Runs blocking work (a network check) on its own thread, for `Task::perform`,
/// so the window keeps drawing while it waits. None if the thread died.
pub async fn off_thread<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (done, result) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = done.send(work());
    });
    result.await.ok()
}

/// Opens a web page or a Windows settings page (`ms-settings:`) with the system's default handler.
pub fn open_url(url: &str) {
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer.exe").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();
    if let Err(error) = result {
        crate::diagnostics::warn("app", format!("could not open {url}: {error}"));
    }
}
