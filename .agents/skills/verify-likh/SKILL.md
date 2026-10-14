---
name: verify-likh
description: Launch, drive and capture evidence from likh, the native Windows dictation app (Rust + iced) in this repo. Use whenever a change to the app needs proof it works the way a user sees it, such as a dictation pasting, the pill, the onboarding window, Codex sign-in states, `likh ctl` or idle cost, rather than just passing tests.
---

# Verify likh

likh is a desktop app. The user runs `likh`, holds Ctrl+Space anywhere to dictate and watches the pill; the first time, the onboarding window opens. Agents drive the running app with `likh ctl` and can open onboarding alone with `likh onboarding`.

Read `features/README.md`, then the feature file you're proving.

## Launch

1. Build: `cargo build -p likh`. The binary is `target/debug/likh.exe`. `bun run setup` has given this checkout its own `LIKH_HOME` (in `.env.local`; `bun run doctor` prints it).
2. Pick the sign-in state to test. Never use the real `~/.codex` for UI proofs:
   ```sh
   bun scripts/fake-codex-home.mjs expired    # or not-installed | signed-out | connected
   ```
   It prints a folder under `.agents/qa/`. Pass that folder as `CODEX_HOME`.
3. Start the window in a background shell that outlives your command. A PowerShell tool session kills its child processes when the command returns, so start the app from a background Bash job:
   ```sh
   CODEX_HOME="<printed path>" ./target/debug/likh.exe onboarding
   ```
   Always run yours with this checkout's `LIKH_HOME`: `bun run dev` and `bun run likh <args>` set it for you; for `./target/debug/likh.exe` set it yourself from `.env.local`. Without it, `likh run` opens the person's own likh window, or runs on their settings, history and update check. It gets its own settings, history, onboarding marker and `likh ctl` pipe (named after the folder, so a scratch home needs a folder name no other worktree uses). Set the same `LIKH_HOME` for every `likh ctl` call. When the person's likh is running, Ctrl+Space stays theirs, so drive yours with `likh ctl`.
4. **Ready signal:** a visible top-level window titled `likh` appears, owned by the new PID. Capture the PID by this checkout's path, never by name alone:
   ```powershell
   (Get-Process likh | Where-Object { $_.Path -like "$PWD\target\*" }).Id
   ```
   If that lists more than one PID, one of them isn't yours. Stop and find out whose it is before driving anything.

## Doctor

Run `bun run doctor` (toolchain, MSVC linker, site deps, Cargo.lock, Codex sign-in). Then make sure the window exists:

```powershell
powershell -NoProfile -File scripts/screenshot-window.ps1 -ProcessId <pid> -Out .agents/qa/doctor.png
```

If it throws "No visible window titled 'likh'", the process died or is still starting.

## Drive

The person is usually working on this computer while you test. Pick the quietest way that proves the feature:

1. **Views:** the `iced_test` renders (`cargo test -p likh brand_board -- --ignored`, and the view tests). No window at all.
2. **Dictations:** start likh with `LIKH_PASTE=<file>` and run `scripts/e2e-dictate.ps1 ... -PasteFile <file>`. No window, no clipboard, no keys, no cursor.
3. **Screenshots** of a live window (`scripts/screenshot-window.ps1`) capture the window itself, even behind others, without bringing it forward.
4. **Clicks** (`scripts/click-window.ps1`) still focus the window and move the real cursor: iced ignored clicks posted as window messages (tried 2026-10-12). Opening a likh window also takes focus. Use these last, and say so.

`likh.exe` is a GUI-subsystem app (it lives in the tray), so PowerShell neither waits for `& likh.exe ctl ...` nor keeps its output. Call it from Bash, or with `Start-Process -Wait`, or through the `Ctl` helper in `scripts/e2e-dictate.ps1`.

For dictation, drive with `likh ctl` (see `features/dictation.md`). For the onboarding window there is no accessibility tree yet, so drive by logical client coordinates. These are the same numbers the iced layout uses; the 560 x 640 window is fixed size and has no title bar. The helper handles DPI scaling and focus:

```powershell
powershell -NoProfile -File scripts/click-window.ps1 -ProcessId <pid> -X <x> -Y <y>
```

Each feature file lists the coordinates for its buttons. After every click, wait about 600 ms (the step transition is a 0.4 s spring), then take a screenshot.

## Evidence

- **Screenshots:** `scripts/screenshot-window.ps1 -ProcessId <pid> -Out .agents/qa/<feature>/<step>.png` captures the window's own pixels, without bringing it forward.
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
| `scripts/screenshot-window.ps1 -ProcessId -Out [-Title]` | PNG of the window's own pixels (PrintWindow), even when covered; never brings it forward |
| `scripts/click-window.ps1 -ProcessId -X -Y [-Title]` | clicks at logical client coordinates |
| `scripts/tts-wav.ps1 -Text -Out` | speaks a sentence into a 16 kHz WAV for `LIKH_AUDIO` |
| `scripts/e2e-dictate.ps1 -Likh -Seconds [-Expected] [-PasteFile] [-LetSilenceStop]` | one dictation, read back as JSON (`pasted`, `pressed`, latency). `-PasteFile` reads likh's `LIKH_PASTE` log instead of a throwaway text box; `-LetSilenceStop` never sends stop and fails unless likh stops by itself |
| `scripts/screenshot-pill.ps1 -Out [-Count -EveryMs -DelayMs]` | the bottom centre of the screen, where the pill is |
| `scripts/measure-idle.ps1 -ProcessId [-Settle -Window]` | memory and CPU of a running build while nobody touches it |
