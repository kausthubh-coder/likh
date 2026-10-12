//! Start with Windows: one value, `murmur`, under the current user's Run key,
//! pointing at this exe. No admin rights, no scheduled task. Turning it off
//! deletes the value.

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const NAME: &str = "murmur";

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

#[cfg(not(target_os = "windows"))]
pub fn set(enabled: bool) -> Result<(), String> {
    let _ = (enabled, RUN, NAME);
    Err("Start at login isn't available on macOS yet.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_quotes_paths_with_spaces() {
        let exe = std::path::Path::new(r"C:\Program Files\murmur\murmur.exe");
        assert_eq!(command(exe), r#""C:\Program Files\murmur\murmur.exe""#);
    }
}
