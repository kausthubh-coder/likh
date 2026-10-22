//! Start with Windows: one value, `likh`, under the current user's Run key,
//! pointing at this exe. No admin rights, no scheduled task. Turning it off
//! deletes the value.
//!
//! Open at login on macOS: one LaunchAgent, `~/Library/LaunchAgents/app.likh.plist`,
//! that runs this executable when the user logs in. Turning it off deletes it.

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const NAME: &str = "likh";

/// The command Windows runs at sign-in: this exe, quoted for paths with spaces.
#[cfg_attr(not(target_os = "windows"), allow(dead_code, reason = "start at login is Windows-only for now"))]
pub fn command(exe: &std::path::Path) -> String {
    format!("\"{}\"", exe.display())
}

#[cfg(target_os = "windows")]
pub fn set(enabled: bool) -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    let (run, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN).map_err(|error| error.to_string())?;
    if enabled {
        let exe = std::env::current_exe().map_err(|error| error.to_string())?;
        run.set_value(NAME, &command(&exe)).map_err(|error| error.to_string())
    } else {
        match run.delete_value(NAME) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.to_string()),
            _ => Ok(()),
        }
    }
}

/// likh was called murmur until 0.1.0-alpha.3. If murmur started with Windows,
/// this exe takes its place, so the choice survives the rename and the old exe
/// doesn't start next to it. Delete once alpha.2 is gone.
#[cfg(target_os = "windows")]
pub fn adopt_murmur_value() -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    let Ok(run) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN, KEY_READ | KEY_WRITE) else {
        return Ok(());
    };
    if run.get_raw_value("murmur").is_err() {
        return Ok(());
    }
    set(true)?;
    run.delete_value("murmur").map_err(|error| error.to_string())
}

/// What `set` calls it, in the settings and in setup.
pub const NAME_FOR_PEOPLE: &str = if cfg!(target_os = "macos") { "Open at login" } else { "Start with Windows" };

#[cfg(target_os = "macos")]
const LAUNCH_AGENT: &str = "app.likh";

/// The LaunchAgent that opens this executable at login.
#[cfg_attr(not(target_os = "macos"), allow(dead_code, reason = "only macOS uses a LaunchAgent"))]
pub fn launch_agent(exe: &std::path::Path) -> String {
    let path = exe.display().to_string().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>app.likh</string>
  <key>ProgramArguments</key>
  <array>
    <string>{path}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>ProcessType</key>
  <string>Interactive</string>
</dict>
</plist>
"#
    )
}

#[cfg(target_os = "macos")]
pub fn set(enabled: bool) -> Result<(), String> {
    let _ = (RUN, NAME);
    let home = dirs::home_dir().ok_or("there is no home folder")?;
    let file = home.join("Library").join("LaunchAgents").join(format!("{LAUNCH_AGENT}.plist"));
    if enabled {
        let exe = std::env::current_exe().map_err(|error| error.to_string())?;
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        }
        std::fs::write(&file, launch_agent(&exe)).map_err(|error| error.to_string())
    } else {
        match std::fs::remove_file(&file) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.to_string()),
            _ => Ok(()),
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn set(enabled: bool) -> Result<(), String> {
    let _ = (enabled, RUN, NAME);
    Err("Starting at login is for Windows and macOS.".into())
}

#[cfg(not(target_os = "windows"))]
pub fn adopt_murmur_value() -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_launch_agent_opens_this_app_at_login() {
        let plist = launch_agent(std::path::Path::new("/Applications/likh.app/Contents/MacOS/likh"));
        assert!(plist.contains("<string>/Applications/likh.app/Contents/MacOS/likh</string>"), "{plist}");
        assert!(plist.contains("<key>RunAtLoad</key>\n  <true/>"), "{plist}");
        let odd = launch_agent(std::path::Path::new("/Users/me/A&B <apps>/likh"));
        assert!(odd.contains("/Users/me/A&amp;B &lt;apps&gt;/likh"), "{odd}");
    }

    #[test]
    fn the_command_quotes_paths_with_spaces() {
        let exe = std::path::Path::new(r"C:\Program Files\likh\likh.exe");
        assert_eq!(command(exe), r#""C:\Program Files\likh\likh.exe""#);
    }
}
