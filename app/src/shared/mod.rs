//! Code that two or more features really use. Nothing goes here speculatively.

pub mod icon;
pub mod spring;
#[cfg(test)]
pub mod test_ui;
pub mod theme;
pub mod ui;

/// likh's website, and its guide to installing Codex and signing in.
pub const SITE: &str = "https://getlikh.vercel.app";
pub const CODEX_SETUP: &str = "https://getlikh.vercel.app/codex";

/// Opens a web page or a Windows settings page (`ms-settings:`) with the system's default handler.
pub fn open_url(url: &str) {
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer.exe").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();
    if let Err(error) = result {
        eprintln!("could not open {url}: {error}");
    }
}
