# likh

Native dictation for Windows (macOS in alpha). Hold Ctrl+Space, talk, let go, and the text pastes at the cursor, transcribed through the user's ChatGPT plan via Codex's sign-in. Users care most that it is instant, reliable, and costs nothing to leave running 24/7. Stack: Rust + iced 0.14 (software renderer, no web view) in `app/`, and a Next.js landing page in `site/`. Run with bun, deployed on Vercel.

## Words we use

- **you**: the agent reading this file.
- **me**: the developer you are working for.
- **user**: the person who uses likh. Not me.
- **pill**: the small solid capsule at the bottom of the screen during a dictation. It never takes focus.
- **dictation**: one hold (or hands-free session) from key-down to paste.
- **Codex sign-in**: `~/.codex/auth.json`, written by Codex. We only ever read it.
- **transcriber** (or **engine**): whatever turns audio into text. The user picks one: ChatGPT through Codex's `/backend-api/transcribe` (the default), or Groq's Whisper with their own free key.

## Map

| Where | What lives there |
|---|---|
| `app/src/<feature>/` | one feature: `mod.rs` with its logic and UI, `tests.rs` beside it |
| `app/src/dictation/` | one dictation, key-down to paste: the decision table and the thread that runs it. Up to 30 minutes: past 3, the audio goes up in pieces while you talk (cut in a pause by `microphone`) and the text is joined |
| `app/src/hotkey/` | the shortcut (Ctrl+Space by default), paste last (Alt+Shift+Z by default; it waits for its keys to come up before pasting), an optional mouse button (a low-level hook, installed only while one is picked) and Esc: system hotkeys on Windows (`windows.rs`), picking a new one (`capture.rs`) |
| `app/src/settings/` | what the user chose (shortcut, paste last, mouse button, mode, silence stop, microphone, engine, language, sounds, pause media, commands, dictionary, history, start with Windows), one JSON file. `LIKH_HOME` moves the whole folder |
| `app/src/history/` | past dictations, one JSON line each, on this computer only. The newest is what paste last pastes after a restart |
| `app/src/commands/` | spoken commands in four groups: lines ("new line", "new bullet"), keys ("press enter", "press tab"), scratch ("scratch that"), punctuation by name (off by default) |
| `app/src/dictionary/` | the user's own words (project names, people): Groq hears them as its prompt, and `cleanup/` puts misheard ones right |
| `app/src/cleanup/` | one quick ChatGPT pass after any engine, through Codex's sign-in (`gpt-6-luna` on `/backend-api/codex/responses`): puts misheard dictionary words right, and with Clean up on (Transcription, off by default) drops "um"s, keeps the speaker's corrections and makes spoken lists into lists. Spoken commands must come through word for word, or likh pastes what it heard; so does a failure or a slow answer (4 s, more for long text) |
| `app/src/media/` | pauses music and videos while you talk and plays them again (Windows media controls; nothing on macOS) |
| `app/src/sounds/` | the start, stop and error cues, synthesized |
| `app/src/autostart/` | start with Windows (the Run key) |
| `app/src/diagnostics/` | what likh did and how long it took: each dictation step (microphone, transcribe, cleanup, paste, the whole dictation) and update checks as spans, plus every warning and error, one JSON line each in `<likh folder>/logs/trace.ndjson` (rolls at 1 MB, keeps 3 old files; never what was said). Summed up in the Diagnostics tab and `likh ctl diagnostics`. Log with `diagnostics::warn`/`error`/`info`, not `eprintln!`: the installed likh has no console |
| `app/src/panel/` | the likh window from the tray: Dictation, Transcription, Commands, Dictionary, History, App, Diagnostics; the shortcut's key caps and keyboard (`shortcut.rs`); its drawn icons (`icons.rs`) |
| `app/src/tray/` | the tray icon and the main thread's iced daemon that owns the windows |
| `app/src/microphone/` | levels and recordings from the picked mic (saved by Windows' id; Windows' default when none is picked or it's unplugged), the list of inputs, and whether Windows' privacy settings block the mic; `LIKH_AUDIO=file.wav` replays a file instead. `cargo test -p likh mic_pick_live -- --ignored` opens this machine's real inputs |
| `app/src/silence/` | silence stop: is someone talking? A small voice detector (earshot) hears a voice, not loudness; `eval.rs` scores it in quiet and noisy rooms |
| `app/src/transcriber/` | audio to text: Codex (`mod.rs`), Groq (`groq.rs`, key in the OS credential store), each engine's glyph (`glyph.rs`), the languages a user can pin (`language.rs`; auto-detect by default); `LIKH_TRANSCRIBER=echo:<text>` fakes all of them |
| `app/src/paste/` | clipboard, Ctrl+V, clipboard back; `LIKH_PASTE=<file>` logs pastes and key presses there instead |
| `app/src/pill/` | the overlay: animation and drawing (`mod.rs`), the Win32 window (`windows.rs`) |
| `app/src/ctl/` | `likh ctl`: drive the running app over a local pipe |
| `app/src/update/` | updates from GitHub Releases: check at launch and after dictations (once a day), download, verify the Ed25519 signature, run the installer silently on "Restart to update" (the foot of the settings sidebar, or the tray's update line). `LIKH_UPDATE_FEED=<url>` points it at a fake release list |
| `installer/windows.iss`, `.github/workflows/release.yml` | the per-user Inno Setup installer, and the workflow that builds, signs and publishes it when a `v*` tag is pushed |
| `app/src/codex_auth/` | is Codex installed and signed in; reading credentials safely |
| `app/src/onboarding/` | the first-run window and its flock |
| `app/src/shared/` | `theme.rs` (brand tokens), `spring.rs` (motion). Only code two features use |
| `app/assets/` | bundled fonts (Geist, OFL) |
| `site/` | the landing page (Next.js). Read `site/AGENTS.md` before touching it |
| `docs/brand/` | `BRAND.md` (the rules), `brand.html` (the rules moving), `logo/` (icon, mark, bars) |
| `docs/plan.md` | decisions, phases, test strategy, spikes |
| `docs/codex.md` | how Codex transcription works, its limits and risks |

Each feature lives in one folder with its UI, logic and tests. Adding a feature means adding a folder; deleting it means deleting the folder.

## Commands

- Every task in a new worktree starts with the `worktree-likh` skill. Ship only with `ship-likh`, when I say so.
- Install / fresh worktree: `bun run setup` (site deps, `cargo fetch`, this checkout's own `LIKH_HOME` and site port). Safe to rerun
- Doctor: `bun run doctor`. Run it first when anything looks off.
- Run the app: `bun run dev` (onboarding opens the first time). Agents: `bun run likh run --skip-onboarding`, then `bun run likh ctl status|start|stop|cancel|paste-last|copy-last|diagnostics|quit`. Both use this checkout's `LIKH_HOME` from `.env.local`: its own settings, history and `likh ctl` pipe, never my installed likh's
- One dictation end to end, with fakes: `scripts/tts-wav.ps1` makes the audio, `scripts/e2e-dictate.ps1` checks what got pasted. Prefer `LIKH_PASTE=<file>` with `-PasteFile <file>`: the paste goes to a log, so I can keep working while you test. Without it, the script clicks into a throwaway box and moves my cursor
- Groq for real: `GROQ_API_KEY=... LIKH_TEST_WAV=<absolute path> LIKH_TEST_TEXT=... cargo test -p likh groq_live -- --ignored --nocapture`. The key stays in the environment, never in a file
- Idle cost of a running build: `scripts/measure-idle.ps1 -ProcessId <pid>`
- What a build costs idle, recording and after: `scripts/benchmark.ps1 [-Label <text>]` on a release build. It runs its own likh and keeps every run in `.agents/qa/bench/results.jsonl`
- Silence stop scored on recordings (quiet mic, fans, music, typing): `scripts/silence-eval.ps1`, report in `.agents/qa/silence/report.md`. Run it when you touch `silence/` or the recorder
- Site: `bun run dev:site` (this checkout's `PORT` from the root `.env.local`)
- Test one module: `cargo test -p likh codex_auth`
- Lint the Rust you changed: `cargo clippy -p likh --all-targets -- -D warnings`
- Full check (CI runs this; run it only when asked): `bun run check`
- Release (only with my go): bump `version` in `app/Cargo.toml` and `RELEASE` in `site/app/page.tsx`, then push the tag `v<version>`. `release.yml` builds the installer, signs it with the `LIKH_UPDATE_SIGNING_KEY` secret (`scripts/sign-release.mjs`) and publishes a prerelease; installed copies find it within a day. A local installer: `scripts/build-installer.ps1` (Inno Setup 6)

## Proving a change works

- Done means you saw it work, not that it compiles. Run the app and look at it.
- Use the `verify-likh` skill to launch, drive and capture evidence.
- UI changes: compare against `docs/brand/brand.html` and `docs/brand/BRAND.md` (tokens, springs, copy). Say so if you shipped something different.
- Test with real-shaped input: real WAV recordings and a real-looking `auth.json` built in a temp dir, never the user's own.
- A test calls the code the way its users do and asserts the exact result. Delete tests that would pass if every function returned a default.
- Never edit or delete a test to make it pass. If the test is wrong, say so and explain why.
- A skipped check is not a pass. Report what you ran, what you saw, and what you did not check.
- Idle cost is a feature. If you touch rendering, audio or polling, measure idle CPU and RAM before and after.

## Working with me

- A question gets an answer, not an action.
- When I ask for a plan, plan. Don't start building until I say go.
- Extend what exists. Propose a rewrite only in a plan, and wait.
- Edit files with your edit tool, not shell one-liners (Windows quoting breaks them).

## How we write code

- Boring and explicit over clever. No hidden wiring or magic strings.
- One way to do each thing. Finish migrations and delete the old way in the same change.
- A wrapper with one caller is just another file to open. Inline it.
- Names you can grep.
- Comments say why, never what.
- Smallest change that solves the real problem.
- Views use `shared::theme` tokens and `shared::spring` specs only. No raw colors, sizes or durations in feature code.
- Nothing animates or polls while idle: request frames only while something moves.

## Rules and what enforces them

| Rule | Enforced by |
|---|---|
| No `unwrap()`, `todo!()`, `dbg!()`, `unsafe` | workspace lints in `Cargo.toml` |
| `unsafe` only for Win32 in `pill/windows.rs` and `hotkey/windows.rs`, each with a `SAFETY:` comment | the lint above; those two files opt out at the top, nothing else may |
| The pill and the shortcut cost nothing while idle (no timers, no polling) | `pill` and `onboarding` tests assert frames stop; `scripts/measure-idle.ps1` before and after |
| Formatting | `cargo fmt --check` in `bun run check` |
| Token values never appear in output | tests in `codex_auth/tests.rs` |
| Every Codex sign-in state shows its fix | tests in `onboarding/tests.rs` |
| Brand tokens match the board | `shared/theme.rs` tests (accent vs Chromium) |
| View tests hold `shared::test_ui::lock()` while using an iced_test simulator (parallel simulators crash) | nothing yet: copy the pattern in `onboarding/tests.rs` |
| A UI test never pastes into a real app | `scripts/e2e-dictate.ps1` only starts a dictation once its own box has focus, and cancels if focus leaves |

When I correct you for the same thing twice, add the rule here and make something enforce it.

## Safety

- Never write to, copy out of, or print `~/.codex/auth.json` or any token. Tests build their own fake `auth.json` in a temp dir.
- Never save a Groq key from a test or a run of yours: it would land in my credential store. Tests take the paths that don't save (a refused or rejected key).
- Don't switch on a mouse button in a running likh unless I ask: the hook blocks that button's normal click system-wide.
- Never touch my nightly Studi profile or other apps' data.
- Releases, tags and Vercel production deploys need my explicit go. A published release reaches every installed likh through the updater.
- The update signing key's private half is the `LIKH_UPDATE_SIGNING_KEY` secret, with my backup in `~/.likh-release/`. Never print it, copy it into the repo, or sign anything with it except a release. Changing `PUBLIC_KEY` in `update/` cuts off every installed likh from updates.
- Stop only processes you started, by the PID you captured. Never kill by name.
- No secrets in code, logs, screenshots or commits.
- Git: open a PR and wait for CI and my review. Never push to `main` directly.
