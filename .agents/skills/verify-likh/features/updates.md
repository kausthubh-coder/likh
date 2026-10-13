# Updates

likh updates itself from GitHub Releases. At launch, and after a dictation once a day, it reads the release list, downloads a newer signed Windows installer in the background and verifies it. The tray's update line then says "Restart to update to <version>", and the foot of the settings sidebar (every tab) and About show `Restart to update`. Restarting quits likh, runs the installer silently, and the installer opens the new likh.

## Sub-features

- `upd-check` a check at launch and after dictations, at most once a day (the time is kept in `$LIKH_HOME/update-checked`), never on a timer. "Check now" in About, or `likh ctl update`.
- `upd-pick` the newest non-draft release, alpha ones included, that is newer than the running version and has both `likh-setup-windows-x64.exe` and its `.sig`.
- `upd-verify` the installer runs only if its Ed25519 signature matches `PUBLIC_KEY` in `app/src/update/mod.rs`; otherwise it's deleted and About says so.
- `upd-tray` the tray always has an update line: Check for updates, Checking for updates… (greyed), Up to date · Check again, Downloading likh <version>… (greyed), Restart to update to <version>, Couldn't check · Try again.
- `upd-sidebar` the settings sidebar's foot, on every tab: `Check for updates` above `v<version>`, `Restart to update` above `v<this> → v<new>` once one is ready.
- `upd-ready` the state is `ready`: the tray line "Restart to update to <version>", and in About "likh <version> is downloaded and checked." with `Restart to update`.
- `upd-apply` Restart (tray, About, or `likh ctl install-update`) quits likh and runs the installer with `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /relaunch=1`. The installer opens likh again, which inherits likh's environment.
- `upd-off` "Check for updates automatically" in About stops the automatic checks; Check now still works.
- `upd-mac` on macOS the state is `available` with a link to the release page. Nothing is installed.

## How to get to it (user POV)

- It happens on its own. The tray's update line and the foot of the settings sidebar show where it stands, and either one checks or restarts.

## Driving it with scripts

Preconditions:

- `scripts/build-installer.ps1` (Inno Setup 6, per user: `winget install JRSoftware.InnoSetup --scope user`), then sign it with the person's backup key: `LIKH_UPDATE_SIGNING_KEY_FILE=~/.likh-release/update-signing-key.pem bun scripts/sign-release.mjs target/installer/likh-setup-windows-x64.exe`. Ask before using that key.
- A fake release list: a tiny `Bun.serve` on 127.0.0.1 that answers `/releases` with one release `v0.1.0-alpha.99` whose two assets point back at the local installer and `.sig`. It must look like GitHub's list (`tag_name`, `draft`, `html_url`, `assets[].name`, `assets[].browser_download_url`).

Recipe (2026-10-12, worked end to end, before likh was installed on this machine). The person now runs an installed likh from `%LOCALAPPDATA%\Programs\likh`. This recipe installs over it and its cleanup uninstalls it, so run it only when they ask, and stop at the unit tests in `update/tests.rs` otherwise.

1. Install silently: `likh-setup-windows-x64.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART`.
2. Start the installed `likh.exe run --skip-onboarding` with `LIKH_HOME` (with an `onboarded` file in it, since the relaunch passes no flags), `LIKH_UPDATE_FEED=http://127.0.0.1:<port>/releases` and a fake `CODEX_HOME`.
3. `likh ctl status` within a few seconds: `update.state` is `ready` with version `0.1.0-alpha.99`, and the server log shows `/releases`, the `.sig`, then the installer.
4. `likh ctl install-update`: `{"ok":true}`, the old PID exits, and a new likh from `%LOCALAPPDATA%\Programs\likh` answers `ctl status` with `update.state` `idle` (not due again for a day).
5. Clean up: `ctl quit`, `unins000.exe /VERYSILENT`, delete the `LIKH_HOME`, stop the server by its PID. Check there's no Programs folder, Start-menu shortcut or uninstall entry left.

## Gotchas

- Installing puts likh in the person's Programs folder, Start menu and Apps list. Always uninstall when done, and say you did.
- Never push a tag to test: a published release reaches every installed likh within a day.
- PowerShell doesn't wait for `likh.exe ctl ...` (it's a GUI-subsystem exe). Start it with redirected output, as `scripts/e2e-dictate.ps1` does.
- The installer keeps the exe's timestamp, so "the file changed" isn't evidence of an update. The new PID and the `ctl status` answer are.
