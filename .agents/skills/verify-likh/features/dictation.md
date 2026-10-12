# Dictation

The product: hold Ctrl+Space, talk, let go, and the text pastes where the cursor is. The pill at the bottom of the screen shows listening, thinking, a countdown in the last minute, and errors. `likh ctl` drives the same path without keys.

## Sub-features

- `dict-paste` pastes the transcript into the focused app and puts the old clipboard text back.
- `dict-tap` drops a tap shorter than 300 ms quietly: no error, nothing pasted.
- `dict-cancel` Esc while recording (or `likh ctl cancel`) throws the audio away. Esc never reaches the app while recording.
- `dict-errors` shows a sentence in the pill for each failure: expired or missing Codex sign-in, ChatGPT refusing it, offline, too long, rate-limited, a busy clipboard, no microphone.
- `dict-cap` counts down the last minute in the pill and stops at 9:55.
- `dict-status` `likh ctl status` reports the phase, the transcriber, the dictation count and the last one's audio length, latency and error. Never the text.
- `pill-idle` the pill is hidden and nothing (pill, shortcut, dictation) uses CPU while idle.

## How to get to it (user POV)

- Run `likh`. Hold Ctrl+Space in any app, talk, let go.

## Driving it with scripts

Preconditions:

- A release build (`cargo build -p likh --release`).
- A fake `CODEX_HOME` (`bun scripts/fake-codex-home.mjs connected`) and, for repeatable runs, a speech WAV: `powershell -File scripts/tts-wav.ps1 -Text "Remind me to send the deck tomorrow at nine." -Out .agents/qa/speech.wav`.
- likh started in a background shell with the fakes, and its PID captured by path:
  ```sh
  LIKH_AUDIO="$(pwd)/.agents/qa/speech.wav" LIKH_TRANSCRIBER="echo:Remind me to send the deck tomorrow at nine." \
    CODEX_HOME="$(pwd)/.agents/qa/codex-connected" ./target/release/likh.exe run --skip-onboarding
  ```

Recipes:

- **End to end.** `powershell -File scripts/e2e-dictate.ps1 -Likh target/release/likh.exe -Seconds 3.4 -Expected "Remind me to send the deck tomorrow at nine."` prints JSON; `matches` must be true. It clicks into its own box first; your mouse moves.
- **The pill on screen.** Start `powershell -File scripts/screenshot-pill.ps1 -Out .agents/qa/pill/run -Count 10 -DelayMs 1500` in the background, then run the end-to-end recipe. Open the frames: a 76 x 36 capsule with peach bars while listening, grey bars while thinking.
- **Without a screen.** `likh ctl start`, wait, `likh ctl stop`, then `likh ctl status`: `dictations` goes up and `last.audio_seconds` matches the WAV.
- **Tap and cancel.** `ctl start` then `ctl stop` within 0.1 s, or `ctl start` then `ctl cancel`: `dictations` doesn't change.
- **Idle cost.** `powershell -File scripts/measure-idle.ps1 -ProcessId <pid> -Window 60`: expect about 2 MB private, about 11 MB working set and 0 ms CPU.
- **Stop it:** `likh ctl quit`.

## Gotchas

- The real shortcut can't be scripted: likh ignores injected keys on purpose (so its own Ctrl+V paste can't trigger it). Ask a person to press Ctrl+Space, or use `likh ctl`.
- On a locked screen, pastes fail with "The native clipboard is not accessible" and screen grabs fail. Check for a `LogonUI` process before blaming likh.
- Never leave likh running with `LIKH_TRANSCRIBER=echo:…` when you hand the machine back: the person's next Ctrl+Space would paste your test sentence. `likh ctl quit` first.
- A real transcription (no `LIKH_TRANSCRIBER`) sends audio to ChatGPT with the person's own Codex sign-in. Ask before doing that.
