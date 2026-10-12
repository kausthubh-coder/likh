# How likh transcribes with Codex

likh does what the Codex desktop app does when you dictate into it. It reads the ChatGPT sign-in Codex already saved and uploads the recording to ChatGPT's transcription endpoint. Codex doesn't have to be running. Everything here was confirmed against the open-source Codex CLI (`codex-rs/tui/src/voice.rs`, removed 2026-03-29) and the installed Codex desktop app (26.1007.2314.0). The working prototype is `dev/sandbox/handy-codex/src-tauri/src/codex_transcription.rs`.

## The request

- **Sign-in:** `$CODEX_HOME/auth.json`, else `~/.codex/auth.json`. Use `tokens.access_token` and `tokens.account_id`. Read the file fresh on every request.
- **Endpoint:** `POST https://chatgpt.com/backend-api/transcribe`
- **Headers:**
  - `Authorization: Bearer <access_token>`
  - `ChatGPT-Account-Id: <account_id>`
  - `originator: Codex Desktop`
  - `User-Agent: Codex Desktop/<version> (<os>)`
- **Body:** multipart with one `file` part (`audio/wav`, 16 kHz mono PCM16), plus an optional `language` part. There is no `model` field; the server picks the model.
- **Response:** `{ "text": "..." }`
- **Speed:** about 0.9 s for 7 s of speech in the prototype.

## Rules likh follows

- **Never refresh or write the tokens.** Codex rotates its refresh token, so a second writer would sign the user out of Codex.
  - When the access token's `exp` has passed, show "Your Codex sign-in expired. Open Codex once to refresh it."
  - Treat a 401 or 403 the same way: "ChatGPT turned down the sign-in. Sign in to Codex again."
- **Token values never reach logs, metrics, screenshots or `likh status --json`.** Tests in `codex_auth/tests.rs` enforce it. The request credentials (`codex_auth::Credentials`) redact both values in `Debug`, enforced by a test.
- **Recordings are capped at 9m55s**, the same as the Codex app (595,000 ms), with a warning at 9:00. The server's own cap is unknown. Its errors include `file_above_max_size`, `input_too_large`, `audio_too_short` and `rate_limit_exceeded`; map each to a sentence in the UI.
- **The HTTP timeout scales with the recording length.** The prototype's fixed 25 s timeout would cut off long uploads.

## Limits worth knowing

| Where | Limit |
|---|---|
| Codex desktop app, record and upload | stops at 595,000 ms (about 9m55s) |
| Codex streaming (`/backend-api/dictation/stream`) | 5-minute session TTL, 30 s per utterance, server VAD |
| The handy-codex prototype | 120 s cap in our own `hands_free.rs`. That was the "random" stop |
| Wispr Flow, for comparison | 20 minutes per session |

**Later:** split long dictations at pauses into chunks well under the cap, upload each chunk while recording continues, and join the text. That removes the cap without relying on the streaming endpoint.

## Plan access and usage, checked 2026-10-12

OpenAI's [pricing page](https://learn.chatgpt.com/docs/pricing) includes Codex
with Free, Go, Plus, Pro, Business, Enterprise and Edu. Its voice dictation
feature table lists Plus, Pro, Business and Enterprise/Edu; Free and Go are
absent from that table. This does not prove that either plan is rejected by
`/backend-api/transcribe`. A real account test is required for each plan.

The same page's $0.05/minute price is for desktop Voice and discusses GPT-Live
conversations. It does not document billing for this internal transcription
endpoint. Do not apply that price to likh or promise unlimited transcription.
A 401 or 403 alone does not identify a plan restriction.

The developer reported that their separate Free-account sign-in attempt did
not work. No request reached the transcriber with verified Free credentials,
so Free transcription remains unverified. The desktop app reportedly runs on
Go, but Go transcription has not been tested. Existing paid-account dictations
worked in the prior live verification.

## Risk, accepted

This is an internal endpoint. OpenAI can change or block it at any time, and the user accepted that it is the product's differentiator. If it changes, ship an update. If accounts start getting flagged, take the project down. Keep the transcriber behind one interface (`transcribe(audio) -> text`) so a replacement is one file.

Worth watching: OpenAI's "Sign in with ChatGPT" program for third-party apps (launched 2026-09-29). If it covers transcription, likh could have its own sign-in.

## Not used: Claude

Claude Code's `/voice` streams 16 kHz PCM over a WebSocket to Anthropic's speech service, which runs on Deepgram. Anthropic's terms forbid third-party apps from using Claude subscription sign-ins, so likh does not support it. The research is in the T3 thread "Debug Voice-to-Text Shortcut".
