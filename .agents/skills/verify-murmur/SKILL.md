---
name: verify-murmur
description: Launch, drive and capture evidence from murmur, the native Windows dictation app (Rust + iced) in this repo. Use whenever a change to the app needs proof it works the way a user sees it, such as a dictation pasting, the pill, the onboarding window, Codex sign-in states, `murmur ctl` or idle cost, rather than just passing tests.
---

# Verify murmur

murmur is a desktop app. The user runs `murmur`, holds Ctrl+Win anywhere to dictate and watches the pill; the first time, the onboarding window opens. Agents drive the running app with `murmur ctl` and can open onboarding alone with `murmur onboarding`.

Read `features/README.md`, then the feature file you're proving.

## Launch

1. Build once: `cargo build -p murmur`. The binary is `target/debug/murmur.exe`.
2. Pick the sign-in state to test. Never use the real `~/.codex` for UI proofs:
   ```sh
   bun scripts/fake-codex-home.mjs expired    # or not-installed | signed-out | connected
   ```
   It prints a folder under `.agents/qa/`. Pass that folder as `CODEX_HOME`.
3. Start the window in a background shell that outlives your command. A PowerShell tool session kills its child processes when the command returns, so start the app from a background Bash job:
   ```sh
   CODEX_HOME="<printed path>" ./target/debug/murmur.exe onboarding
   ```
4. **Ready signal:** a visible top-level window titled `murmur` appears, owned by the new PID. Capture the PID by path, never by name alone:
   ```powershell
   (Get-Process murmur | Where-Object { $_.Path -like "*dev\murmur\target*" }).Id
   ```
   If that lists more than one PID, one of them isn't yours. Stop and find out whose it is before driving anything.

## Doctor

Run `bun run doctor` (toolchain, MSVC linker, site deps, Cargo.lock, Codex sign-in). Then make sure the window exists:

```powershell
powershell -NoProfile -File scripts/screenshot-window.ps1 -ProcessId <pid> -Out .agents/qa/doctor.png
```

If it throws "No visible window titled 'murmur'", the process died or is still starting.

## Drive

For dictation, drive with `murmur ctl` (see `features/dictation.md`). For the onboarding window there is no accessibility tree yet, so drive by logical client coordinates. These are the same numbers the iced layout uses; the 560 x 640 window is fixed size and has no title bar. The helper handles DPI scaling and focus:

```powershell
powershell -NoProfile -File scripts/click-window.ps1 -ProcessId <pid> -X <x> -Y <y>
```

Each feature file lists the coordinates for its buttons. After every click, wait about 600 ms (the step transition is a 0.4 s spring), then take a screenshot.

## Evidence

- **Screenshots:** `scripts/screenshot-window.ps1 -ProcessId <pid> -Out .agents/qa/<feature>/<step>.png` captures the window as it appears on screen.
- **CLI proof:** the exact command plus its stdout and exit code, saved to `.agents/qa/<feature>/<name>.txt`.
- Capture the state before the action, the action itself (command and coordinates) and the state after.
- Open every screenshot and compare it with `docs/brand/brand.html` §07 and the copy table in `docs/brand/BRAND.md`. A screenshot you didn't look at isn't proof.
- `.agents/qa/` is gitignored and survives cleanup.

## Cleanup

- Stop only the PID you captured: `Stop-Process -Id <pid>`. Never stop processes by name.
- Delete the fake Codex folders: `Remove-Item -Recurse .agents/qa/codex-*`.
- Leave the screenshots and transcripts in place.

## Helpers

| Script | Does |
|---|---|
| `scripts/fake-codex-home.mjs <state>` | writes a throwaway `CODEX_HOME` and prints its path |
| `scripts/screenshot-window.ps1 -ProcessId -Out [-Title]` | PNG of the window as it appears on screen |
| `scripts/click-window.ps1 -ProcessId -X -Y [-Title]` | clicks at logical client coordinates |
| `scripts/tts-wav.ps1 -Text -Out` | speaks a sentence into a 16 kHz WAV for `MURMUR_AUDIO` |
| `scripts/e2e-dictate.ps1 -Murmur -Seconds [-Expected]` | one dictation into a throwaway text box, read back as JSON |
| `scripts/screenshot-pill.ps1 -Out [-Count -EveryMs -DelayMs]` | the bottom centre of the screen, where the pill is |
| `scripts/measure-idle.ps1 -ProcessId [-Settle -Window]` | memory and CPU of a running build while nobody touches it |
