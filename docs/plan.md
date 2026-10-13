# Plan

What we're building, in what order, and how we'll know it works. Decisions made with the user on 2026-10-12.

## Decisions

- **Product:** native dictation. Hold Ctrl+Space, talk, let go, and the text pastes at the cursor in under a second. It must idle at near-zero CPU and low RAM so it can run 24/7.
- **Transcription:** Codex by default (see `docs/codex.md`). Claude is out. Since 2026-10-12 the user can pick a free alternative, in onboarding and in settings:
  - **Groq** with the user's own free key (whisper-large-v3-turbo, about 2,000 requests a day free). The key lives in the OS credential store, never in settings.json.
  - **Windows speech was removed** (2026-10-12, after 0.1.0-alpha.5): it didn't work for the developer in practice. A settings.json that still says `"engine": "windows"` loads as ChatGPT.
- **UI:** Rust + iced 0.14 with the tiny-skia software renderer, no web view and no GPU context. We chose iced over GPUI for headless screenshot tests that run on Windows and a stabler API, and pay for it with our own spring helper and per-OS overlay code.
- **The pill is solid, not glass** (2026-10-12). Real refraction needs a GPU renderer and continuous screen capture, and the GPU renderer alone idled at 229 to 313 MB against 7 MB. The pill is its own layered Win32 window drawn with tiny-skia (`app/src/pill/`), because iced's software renderer can't make a transparent, never-focused window and resizing an OS window per frame looked laggy.
- **The shortcut is a system hotkey on Windows** (2026-10-12), Ctrl+Space by default. handy-keys woke its hook thread every 10ms and hooked the mouse too. Our own low-level hook fixed that, but Windows silently removes low-level hooks that answer slowly once (it happened during heavy builds), and the shortcut died. `RegisterHotKey` can't time out and swallows the keys for us; letting go is noticed by checking the keys every 15ms, only while held.
- **Platforms:** Windows first. macOS ships labelled alpha; nobody on the team can test it on a Mac yet.
- **Release:** a public repo (MIT) with a landing page in `site/`. Unsigned builds on GitHub Releases. Pull requests with CI before `main`.
- **Local only:** logs, metrics and crash reports stay on the machine. "Copy diagnostics" produces a redacted bundle for GitHub issues.

## Phases (all ship in the first public release)

| Phase | What | Done when |
|---|---|---|
| 0 | Brand pack, scaffold, Codex sign-in detection, first onboarding screens | ✅ |
| 1 | **Spikes:** idle cost of the running app; refraction cost on Windows | ✅ idle measured below; refraction dropped for a solid pill |
| 2 | Hold the hotkey, record, transcribe, paste. The pill. Fake mic plus fake transcriber. `likh ctl` | ✅ `scripts/e2e-dictate.ps1` pastes into a throwaway box with the fakes. A real Codex transcription and the physical Ctrl+Space still need a person (injected keys are ignored by design) |
| 3 | Full onboarding and every pill state | ✅ five steps (Try it is folded into the shortcut step inside the app); pill: listening, thinking, countdown, error, done. Hands-free waits for phase 4 |
| 4 | Hands-free, Esc cancel, history, sounds, voice commands, start with Windows, a settings window, a changeable shortcut, 9m55s cap | ✅ hands-free is "Hold or tap" (tap once, tap again) instead of double-tap; silence stop, mic picker and paste-last are still to do |
| 5 | Landing page with downloads, the release workflow | `v0.1.0` on GitHub Releases |
| 6 | Silence stop, a mouse button, more spoken commands, pause media, Groq and Windows speech, all in settings | ✅ silence stop proven end to end (`e2e-dictate.ps1 -LetSilenceStop`). A real Groq key, Windows speech with Online speech recognition on, pausing something that plays, and a physical mouse button still need a person |
| later | Chunked long dictation, dictionary, snippets, cleanup (fillers, "actually 3"), command mode, styles, stats | |

## Measured so far

The onboarding window, idle on the Welcome step, release build. Windows 11, measured 2026-10-12. Each figure is taken after 20 s to settle, then over 30 s:

| Renderer | Private memory | Working set | Idle CPU over 30 s | Threads |
|---|---|---|---|---|
| wgpu, default (DX12) | 312.6 MB | 255.7 MB | 1,109 ms | |
| wgpu, `WGPU_BACKEND=vulkan` | 228.8 MB | 176.4 MB | 328 ms | 61 |
| tiny-skia, `ICED_BACKEND=tiny-skia` | **7.2 MB** | **30.5 MB** | **0 ms** | 11 |
| tiny-skia only (wgpu compiled out), 2026-10-12 | 6.9 MB | 20.6 MB | 0 ms | 9 |

The running app, idle (no window open, pill hidden, shortcut armed), release build, `scripts/measure-idle.ps1` over 60 s:

| Build | Private memory | Working set | Idle CPU over 60 s | Threads |
|---|---|---|---|---|
| with handy-keys for the shortcut | 3.0 MB | 13.7 MB | 203 ms | 12 |
| with likh's own hook (`hotkey/windows.rs`) | **2.0 MB** | **10.8 MB** | **0 ms** | 6 |
| phase 6 (engines, silence stop, pause media; default settings, mouse button off), 2026-10-12 | 2.3 MB | 11.9 MB | **0 ms** | 8 |
| 0.1.0-alpha.5, side by side with alpha.6 (same `LIKH_HOME`, settled 10 s), 2026-10-12 | 2.4 MB | 13.8 MB | **0 ms** | 8 |
| 0.1.0-alpha.6 (no Windows speech, quiet countdown, update line), 2026-10-12 | 2.4 MB | 13.8 MB | **0 ms** | 8 |

A whole dictation, `scripts/benchmark.ps1` (its own likh, real microphone, hands-free for 30 s, fake transcriber), release build, 2026-10-13. Idle is measured over 20 s before and after:

| Build | Idle | Recording | Idle after |
|---|---|---|---|
| alpha.6 silence stop (a -37 dBFS line), two runs | 0 ms, 2.4 MB | 6.1 to 6.7 % of one core, 12.0 MB peak | 0 ms, 3.3 to 3.6 MB |
| the voice detector (earshot) | 0 ms, 2.4 MB | 8.6 % of one core, 12.3 MB peak | 0 ms, 3.3 MB |

The differences while recording are run-to-run noise: the mic and the pill's bars cost far more than the detector, which takes 49 ms of CPU per minute of audio (`scripts/silence-eval.ps1`). After the first dictation, private memory settles about 1 MB higher than before it and stays there.

- **Silence stop hears a voice, not loudness** (2026-10-13). The alpha.6 line (about -37 dBFS) never noticed one word on a quiet mic, so hands-free never stopped, and a loud fan kept it going. A line that learned the room's noise heard typing as talking. earshot (pure Rust, +74 KB, 8 KB while recording) passed all 11 scenes in `silence/eval.rs`. Another person talking nearby still keeps hands-free going: that is a voice too.

A picked mouse button installs a low-level mouse hook: no cost while the mouse is still, one wake per mouse event while it moves.

The exe shrank from 11.4 MB to 5.1 MB when wgpu was compiled out (7.6 MB with everything in phase 2).

The two renderers look identical (`.agents/qa/renderer/`). **Implication:** the always-on process should draw with tiny-skia and open a GPU context only if refraction proves worth it. The spikes below now compare against 7 MB, not against Handy's 139 MB.

## Features (Wispr Flow's list, triaged)

- **First release:**
  - push-to-talk
  - hands-free with silence stop
  - solid pill with level bars
  - paste with clipboard restore
  - tray
  - launch at login
  - onboarding
  - Esc cancel
  - mic picker
  - paste-last
  - history
  - sounds
  - usage panel
  - Codex sign-in states
- **Done in phase 6:**
  - hands-free silence stop (Never, 3, 5 or 10 s; 5 s by default)
  - mouse-button trigger (middle, back, forward)
  - pause media while you talk (Windows; Wispr mutes instead, and polls the output device every 2 s to do it)
  - lists ("new bullet"), "press tab", "scratch that" after a sentence, punctuation by name
  - Groq as a free engine
- **Updates (2026-10-12):** from GitHub Releases through the existing Inno installer, Ed25519-signed in `release.yml`, checked at launch and after dictations (never on a timer), applied when the user picks "Restart to update". Velopack would add delta updates but replaces the installer; not worth it at 5 MB. macOS only links to the release.
- **Next:**
  - chunking past 9m55s
  - personal dictionary and fix-up rules
  - snippets
  - filler and self-correction cleanup (with a small model through "Sign in with ChatGPT" or similar)
  - offline local model
- **Later:**
  - command mode (voice-edit the selection)
  - transforms
  - per-app styles
  - context from the text around the cursor
  - IDE file tagging
  - words-per-minute and streak stats
  - language picker
  - whisper mode
- **Skipped:**
  - accounts
  - cloud sync
  - teams, SSO and admin
  - Notetaker
  - mobile apps
  - MCP connectors
  - Focus mode
  - paid plans

## How we test a voice app

1. **Pure logic** (state machine, VAD, resampling, sign-in, springs): unit tests next to the code, with WAV fixtures.
2. **Views:** `iced_test` simulator tests click through screens and find text. They run headless on Windows CI. The pill's drawing is tested on its pixels (`pill/tests.rs`).
3. **Fake mic:** `LIKH_AUDIO=path.wav` makes recordings replay that file in real time instead of opening the mic (`microphone::Recording::start_from`).
4. **Fake transcriber:** `LIKH_TRANSCRIBER=echo:<text>` returns fixed text, so UX tests never touch ChatGPT.
5. **Control CLI:** `likh ctl start|stop|cancel|status|quit` talks to the running app over a local named pipe and prints one JSON line, so agents drive the real app without pressing keys.
6. **End to end** (`scripts/e2e-dictate.ps1`):
   - `scripts/tts-wav.ps1` speaks a known sentence into a WAV with Windows' built-in voice.
   - likh runs with that WAV as its mic. The script opens a throwaway text box, clicks into it, and drives `likh ctl start` / `stop`.
   - It reads the box back and compares. It refuses to start unless its box has focus and cancels if focus leaves, so a paste never lands in a real app. Not Notepad: Windows 11 Notepad keeps tabs between sessions.
   - Still to add: the real transcriber with a word error rate under 10%, which needs a signed-in Codex.
7. **Visual:** the brand board's pill and onboarding pictures are rendered from the real code (`cargo test -p likh brand_board -- --ignored`); compare against them.

## Observability (all local)

- **Logs:** JSON lines via `tracing`, rolled daily, in `%LOCALAPPDATA%/likh/logs`.
- **Usage database:** SQLite in `%LOCALAPPDATA%/likh/usage.db`, one row per dictation:
  - key-up to paste latency
  - audio seconds
  - upload bytes
  - HTTP status
  - error
- **Health:** idle RAM and CPU are sampled every 5 minutes.
- **Interfaces:**
  - `likh status --json` and `likh ctl status --json` print all of it for agents.
  - The usage panel shows the same data to people.
