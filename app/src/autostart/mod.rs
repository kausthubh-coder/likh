//! Start with Windows: one value, `likh`, under the current user's Run key,
//! pointing at this exe. No admin rights, no scheduled task. Turning it off
//! deletes the value.

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

#[cfg(not(target_os = "windows"))]
pub fn set(enabled: bool) -> Result<(), String> {
    let _ = (enabled, RUN, NAME);
    Err("Start at login isn't available on macOS yet.".into())
}

#[cfg(not(target_os = "windows"))]
pub fn adopt_murmur_value() -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_quotes_paths_with_spaces() {
        let exe = std::path::Path::new(r"C:\Program Files\likh\likh.exe");
        assert_eq!(command(exe), r#""C:\Program Files\likh\likh.exe""#);
    }
}
