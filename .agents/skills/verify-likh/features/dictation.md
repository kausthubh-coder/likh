# Dictation

The product: hold Ctrl+Space, talk, let go, and the text pastes where the cursor is. The pill at the bottom of the screen shows listening, thinking, a countdown in the last minute, and errors. `likh ctl` drives the same path without keys.

## Sub-features

- `dict-paste` pastes the transcript into the focused app and puts the old clipboard text back.
- `dict-tap` drops a tap shorter than 300 ms quietly: no error, nothing pasted.
- `dict-cancel` Esc while recording (or `likh ctl cancel`) throws the audio away. Esc never reaches the app while recording.
- `dict-errors` shows a sentence in the pill for each failure: expired or missing Codex sign-in, ChatGPT refusing it, offline, too long, rate-limited, a busy clipboard, no microphone.
- `dict-cap` counts down the last minute in the pill and stops at 9:55.
- `dict-status` `likh ctl status` reports the phase, the transcriber (`codex`, `groq`, `windows` or `echo`), the dictation count and the last one's audio length, latency and error. Never the text.
- `dict-silence-stop` in hands-free, once something above about -37 dBFS was heard, likh stops and pastes after the chosen quiet (3, 5 or 10 s; 5 by default). A held shortcut never stops by itself.
- `dict-commands` "new line", "new paragraph", "new bullet" anywhere; "press enter" / "press tab" at the end; "scratch that" alone undoes the last paste, after a sentence it drops that sentence; punctuation by name ("comma", "question mark") only when switched on. Each group can be switched off.
- `dict-engine` the engine in settings is read at every key-down: Codex, Groq (key from the credential store) or Windows speech (listens live; with Online speech recognition off the pill says how to turn it on).
- `dict-pause-media` with "Pause music and videos" on (the Windows default), what's playing pauses at key-down and plays again at key-up. Only what likh paused.
- `dict-mouse` a picked mouse button (middle, back, forward) works like the shortcut and its normal click is blocked.
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

- **End to end, quietly (the default).** Add `LIKH_PASTE="$(pwd)/.agents/qa/pasted.jsonl"` when starting likh, then `powershell -File scripts/e2e-dictate.ps1 -Likh target/release/likh.exe -Seconds 3.4 -Expected "Remind me to send the deck tomorrow at nine." -PasteFile .agents/qa/pasted.jsonl`. It prints JSON; `matches` must be true, and `pressed` lists keys likh would have pressed. Nothing on screen changes.
- **End to end, through the real clipboard.** The same without `LIKH_PASTE` and `-PasteFile`: it clicks into its own box first, so the person's mouse moves. Only when the clipboard path itself is what you're proving.
- **The pill on screen.** Start `powershell -File scripts/screenshot-pill.ps1 -Out .agents/qa/pill/run -Count 10 -DelayMs 1500` in the background, then run the end-to-end recipe. Open the frames: a 76 x 36 capsule with peach bars while listening, grey bars while thinking.
- **Silence stop and commands.** Make the audio: `scripts/tts-wav.ps1 -Text "Meet at two. Scratch that. Meet at three, press tab." -Out .agents/qa/speech.wav`, then append 8 s of silence (Python's `wave` module: copy the frames, then add zero bytes). Put `{ "mode": "tap", "silence_stop": "3s", "pause_media": false, "sounds": false }` in `$LIKH_HOME/settings.json`. Start likh with that WAV and `LIKH_TRANSCRIBER="echo:Meet at two. Scratch that. Meet at three, press tab."`, plus `LIKH_PASTE`, then `scripts/e2e-dictate.ps1 -Likh target/release/likh.exe -Seconds 15 -Expected "Meet at three" -LetSilenceStop -PasteFile <the log>`. Expect `matches: true`, `pressed: ["Tab"]` and `start_to_stop_ms` about 3 s past the speech (8.2 s for this sentence).
- **Groq for real.** With a key the person gave you for it: `GROQ_API_KEY=... LIKH_TEST_WAV="$(pwd)/.agents/qa/speech.wav" LIKH_TEST_TEXT="Remind me to send the deck to Priya tomorrow at 9." cargo test -p likh groq_live -- --ignored --nocapture`. It checks the key, refuses a made-up one, and transcribes the WAV with likh's own request code (about 0.5 to 0.7 s for 3.6 s of speech, 2026-10-12). Write numbers as digits in the expected text: Whisper does.
- **Without a screen.** `likh ctl start`, wait, `likh ctl stop`, then `likh ctl status`: `dictations` goes up and `last.audio_seconds` matches the WAV.
- **Tap and cancel.** `ctl start` then `ctl stop` within 0.1 s, or `ctl start` then `ctl cancel`: `dictations` doesn't change.
- **Idle cost.** `powershell -File scripts/measure-idle.ps1 -ProcessId <pid> -Window 60`: expect about 2 MB private, about 11 MB working set and 0 ms CPU.
- **Stop it:** `likh ctl quit`.

## Gotchas

- The real shortcut can't be scripted: likh ignores injected keys on purpose (so its own Ctrl+V paste can't trigger it). Ask a person to press Ctrl+Space, or use `likh ctl`.
- On a locked screen, pastes fail with "The native clipboard is not accessible" and screen grabs fail. Check for a `LogonUI` process before blaming likh.
- Never leave likh running with `LIKH_TRANSCRIBER=echo:…` when you hand the machine back: the person's next Ctrl+Space would paste your test sentence. `likh ctl quit` first.
- A real transcription (no `LIKH_TRANSCRIBER`) sends audio to ChatGPT with the person's own Codex sign-in. Ask before doing that.
- Windows speech can't be faked with `LIKH_AUDIO`: it opens the real mic itself. `LIKH_TRANSCRIBER=echo:` still replaces it. `cargo test -p likh windows_speech -- --ignored --nocapture` starts and stops a real session (needs Online speech recognition on, which is the person's setting to change).
- Pause media pauses the person's own music. Keep `"pause_media": false` in your `LIKH_HOME` settings unless that's what you're proving, and ask first. `cargo test -p likh media -- --ignored --nocapture` only lists the sessions.
- Never pick a mouse button in a running likh unless asked: the hook blocks that click system-wide. Injected clicks are ignored by design, so it can't be scripted anyway.
