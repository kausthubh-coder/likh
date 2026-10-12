# murmur

Native dictation for Windows (macOS in alpha). Hold Ctrl+Space, talk, let go, and the text pastes at the cursor, transcribed through the user's ChatGPT plan via Codex's sign-in. Users care most that it is instant, reliable, and costs nothing to leave running 24/7. Stack: Rust + iced 0.14 (software renderer, no web view) in `app/`, and a Next.js landing page in `site/`. Run with bun, deployed on Vercel.

## Words we use

- **you**: the agent reading this file.
- **me**: the developer you are working for.
- **user**: the person who uses murmur. Not me.
- **pill**: the small solid capsule at the bottom of the screen during a dictation. It never takes focus.
- **dictation**: one hold (or hands-free session) from key-down to paste.
- **Codex sign-in**: `~/.codex/auth.json`, written by Codex. We only ever read it.
- **transcriber**: whatever turns audio into text. Today that's Codex's `/backend-api/transcribe`.

## Map

| Where | What lives there |
|---|---|
| `app/src/<feature>/` | one feature: `mod.rs` with its logic and UI, `tests.rs` beside it |
| `app/src/dictation/` | one dictation, key-down to paste: the decision table and the thread that runs it |
| `app/src/hotkey/` | the shortcut (Ctrl+Space by default) and Esc: system hotkeys on Windows (`windows.rs`), picking a new one (`capture.rs`) |
| `app/src/settings/` | what the user chose (shortcut, mode, sounds, commands, history, start with Windows), one JSON file |
| `app/src/history/` | past dictations, one JSON line each, on this computer only |
| `app/src/commands/` | spoken commands: "new line", "press enter", "scratch that" |
| `app/src/sounds/` | the start, stop and error cues, synthesized |
| `app/src/autostart/` | start with Windows (the Run key) |
| `app/src/panel/` | the murmur window from the tray: General, History, About |
| `app/src/tray/` | the tray icon and the main thread's iced daemon that owns the windows |
| `app/src/microphone/` | levels and recordings; `MURMUR_AUDIO=file.wav` replays a file instead |
| `app/src/transcriber/` | audio to text through Codex; `MURMUR_TRANSCRIBER=echo:<text>` fakes it |
| `app/src/paste/` | clipboard, Ctrl+V, clipboard back |
| `app/src/pill/` | the overlay: animation and drawing (`mod.rs`), the Win32 window (`windows.rs`) |
| `app/src/ctl/` | `murmur ctl`: drive the running app over a local pipe |
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

- Install / fresh worktree: `bun run setup` (site deps, `cargo fetch`, picks a free site port)
- Doctor: `bun run doctor`. Run it first when anything looks off.
- Run the app: `bun run dev` (onboarding opens the first time). Agents: `murmur run --skip-onboarding`, then `murmur ctl status|start|stop|cancel|quit`
- One dictation end to end, with fakes: `scripts/tts-wav.ps1` makes the audio, `scripts/e2e-dictate.ps1` pastes into a throwaway box
- Idle cost of a running build: `scripts/measure-idle.ps1 -ProcessId <pid>`
- Site: `bun run dev:site` (port from `site/.env.local`)
- Test one module: `cargo test -p murmur codex_auth`
- Lint the Rust you changed: `cargo clippy -p murmur --all-targets -- -D warnings`
- Full check (CI runs this; run it only when asked): `bun run check`

## Proving a change works

- Done means you saw it work, not that it compiles. Run the app and look at it.
- Use the `verify-murmur` skill to launch, drive and capture evidence.
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
- Never touch my nightly Studi profile or other apps' data.
- Releases, tags and Vercel production deploys need my explicit go.
- Stop only processes you started, by the PID you captured. Never kill by name.
- No secrets in code, logs, screenshots or commits.
- Git: open a PR and wait for CI and my review. Never push to `main` directly.
