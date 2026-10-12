# Plan

What we're building, in what order, and how we'll know it works. Decisions made with the user on 2026-10-12.

## Decisions

- **Product:** native dictation. Hold Ctrl+Win, talk, let go, and the text pastes at the cursor in under a second. It must idle at near-zero CPU and low RAM so it can run 24/7.
- **Transcription:** Codex only (see `docs/codex.md`). Claude is out.
- **UI:** Rust + iced 0.14, drawn on the GPU with wgpu and no web view. The look is liquid glass (`docs/brand/`). We chose iced over GPUI for its public custom-shader path (real glass), headless screenshot tests that run on Windows, and a stabler API. We pay for that with our own spring helper and per-OS overlay code.
- **Platforms:** Windows first. macOS ships labelled alpha; nobody on the team can test it on a Mac yet.
- **Release:** a public repo (MIT) with a landing page in `site/`. Unsigned builds on GitHub Releases. Pull requests with CI before `main`.
- **Local only:** logs, metrics and crash reports stay on the machine. "Copy diagnostics" produces a redacted bundle for GitHub issues.

## Phases (all ship in the first public release)

| Phase | What | Done when |
|---|---|---|
| 0 | Brand pack, scaffold, Codex sign-in detection, first onboarding screens | ✅ this commit |
| 1 | **Spikes:** idle memory of an iced tray app plus the pill; refraction cost on Windows (below) | numbers recorded in this file |
| 2 | Hold the hotkey, record, transcribe, paste. Minimal pill. Fake mic plus fake transcriber. `murmur ctl` | the e2e test passes in Notepad |
| 3 | Full onboarding (6 steps) and the glass pill with every state | screenshots match `docs/brand/brand.html` |
| 4 | Hands-free (double-tap) with silence stop, Esc cancel, mic picker, paste-last, history, sounds, launch at login, usage panel, 9m55s cap | each has a test, and the verify skill covers it |
| 5 | Landing page with downloads, the release workflow | `v0.1.0` on GitHub Releases |
| later | Chunked long dictation, dictionary, snippets, cleanup, command mode, styles, stats | |

## Measured so far

The onboarding window, idle on the Welcome step, release build. Windows 11, measured 2026-10-12. Each figure is taken after 20 s to settle, then over 30 s:

| Renderer | Private memory | Working set | Idle CPU over 30 s | Threads |
|---|---|---|---|---|
| wgpu, default (DX12) | 312.6 MB | 255.7 MB | 1,109 ms | |
| wgpu, `WGPU_BACKEND=vulkan` | 228.8 MB | 176.4 MB | 328 ms | 61 |
| tiny-skia, `ICED_BACKEND=tiny-skia` | **7.2 MB** | **30.5 MB** | **0 ms** | 11 |

The two renderers look identical (`.agents/qa/renderer/`). **Implication:** the always-on process should draw with tiny-skia and open a GPU context only if refraction proves worth it. The spikes below now compare against 7 MB, not against Handy's 139 MB.

## Spikes to run before building on assumptions

1. **Idle cost.** A minimal iced daemon with a tray icon and the pill window hidden, then shown idle. Record private bytes, working set, CPU % and GPU memory after 10 minutes, and compare with handy-codex (139 MB private across 8 processes, 0.03% CPU). If iced/wgpu idles above about 80 MB, try the `tiny-skia` software renderer for the pill.
2. **Refraction on Windows.** The pill at 148x40 over a busy, moving background, in three modes:
   - OS Acrylic blur only.
   - Continuous `Windows.Graphics.Capture` of the strip behind the pill, with a refraction shader. The pill is excluded from the capture via `WDA_EXCLUDEFROMCAPTURE`.
   - Capture only while it's visible.

   Measure CPU %, GPU %, GPU memory, power (Task Manager "power usage"), and the latency between the background moving and the lens updating. The user wants refraction always on, not a few seconds at a time, so the number that decides it is the always-on cost.

## Features (Wispr Flow's list, triaged)

- **First release:**
  - push-to-talk
  - hands-free with silence stop
  - glass pill with level bars
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
- **Next:**
  - chunking past 9m55s
  - personal dictionary and fix-up rules
  - snippets
  - filler and self-correction cleanup
  - lists
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
  - auto-mute media
  - mouse-button triggers
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
2. **Views:** `iced_test` simulator tests click through screens and find text. They run headless on Windows CI. Snapshot tests come with the glass pill.
3. **Fake mic:** an `AudioSource` with `Mic` and `WavFile` implementations. `MURMUR_AUDIO=path.wav` makes the real app "hear" a file.
4. **Fake transcriber:** `MURMUR_TRANSCRIBER=echo:<text>` returns fixed text, so UX tests never touch ChatGPT.
5. **Control CLI:** `murmur ctl start|stop|cancel|status|screenshot --json` talks to the running app over a local named pipe, so agents drive the real app without pressing keys.
6. **End to end:**
   - Windows text-to-speech (SAPI) generates a WAV of a known sentence.
   - The fake mic feeds it in while the real transcriber runs, dictating into Notepad.
   - UI Automation reads the text back, and the word error rate is scored (it must stay under 10%).
7. **Visual:** screenshots of the pill and onboarding in each state, compared against baselines and against `brand.html`.

## Observability (all local)

- **Logs:** JSON lines via `tracing`, rolled daily, in `%LOCALAPPDATA%/murmur/logs`.
- **Usage database:** SQLite in `%LOCALAPPDATA%/murmur/usage.db`, one row per dictation:
  - key-up to paste latency
  - audio seconds
  - upload bytes
  - HTTP status
  - error
- **Health:** idle RAM and CPU are sampled every 5 minutes.
- **Interfaces:**
  - `murmur status --json` and `murmur ctl status --json` print all of it for agents.
  - The usage panel shows the same data to people.
