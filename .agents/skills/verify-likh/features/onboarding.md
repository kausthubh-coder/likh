# Onboarding

The first-run window: five steps in one borderless 560 x 640 window. It opens with the flock landing on the logo, checks Codex live, shows the mic working, teaches the shortcut and ends with the flock spelling likh. Inside the running app the shortcut step is a real first dictation into a practice box.

## Sub-features

- `onb-welcome` plays the flock into the mark, then "Talk. It types." and a peach `Get started`, with the first of five dots wide. The window then draws nothing until something changes.
- `onb-codex-not-installed` titles the step "Codex isn't installed", with `Check again` (plain text) and `Get Codex`. `Get Codex` opens https://developers.openai.com/codex.
- `onb-codex-signed-out` titles it "Codex isn't signed in", with `Check again`.
- `onb-codex-expired` titles it "Your Codex sign-in expired", with `Check again`.
- `onb-codex-connected` titles it "Connected through Codex", shows the masked email and plan, and offers `Continue`. The link in the art turns green.
- `onb-codex-live` turns not-connected into connected on its own within 2 s once a valid `auth.json` appears.
- `onb-engine` the step opens on the engine in settings (ChatGPT for a new install) with a ChatGPT · Groq · Windows pick above the art; the pick stays in place whichever is chosen. Picking one saves it at once.
- `onb-groq` "Add a free Groq key", a hidden key field, `Get a free key` (opens console.groq.com/keys) and `Save key`. A key not starting `gsk_` is refused before any request; Groq's 401 says "Groq turned down that key". A saved key reads "Groq is ready" with `Continue`.
- `onb-mic` shows nine live bars and the device name; with no mic, "Can't hear a microphone" and `Open settings`.
- `onb-shortcut` lights the `Ctrl` `Space` key caps. Inside the app: holding the real shortcut records, and the words land in the practice box.
- `onb-done` spells likh in dots and `Finish` closes the window. likh keeps running.
- `onb-back` returns to the step before from anywhere but welcome and done.
- `onb-drag` moves the window by dragging any empty spot (there is no title bar).

## How to get to it (user POV)

- The first time `likh` runs. After `Finish` (or Alt+F4) it never opens again; delete `%LOCALAPPDATA%\likh\onboarded` to see it again.
- `likh onboarding` opens it on its own, without the app (the shortcut step then only checks the keys).

## Driving it with scripts

Preconditions:

- A fake `CODEX_HOME` for the state you're proving.
- The window was launched with that `CODEX_HOME` (`likh onboarding`), and you have its PID.

Coordinates are logical client pixels in the 560 x 640 window; there is no title bar to subtract.

- **Welcome.** Wait 2 s for the flock, then capture `welcome.png`.
- **Primary button** (Get started, Continue, Sounds good, Check again, Finish): (280, 557). After 600 ms capture.
- **Back:** (64, 40).
- **Not installed.** `Check again` is left of `Get Codex`, on the same row as the primary button. Only click `Get Codex` when proving that link (it opens the browser).
- **Live re-check.** Launch with the `signed-out` folder, go to step 2, then run `bun scripts/fake-codex-home.mjs connected` and copy `.agents/qa/codex-connected/auth.json` into the signed-out folder. Within 2 s the title reads "Connected through Codex".
- **Microphone.** Reaching step 3 opens the real default mic; Windows shows its mic-in-use icon until you leave the step. Expect your own room noise on the bars.
- **Engine pick:** ChatGPT and Groq, one segmented pick centred at y 147 on step 2 (two segments since 0.1.0-alpha.6; read the x from `docs/brand/onboarding/2-groq-key.png`). Compare `docs/brand/onboarding/2-groq-key`.
- **Compare** with the renders in `docs/brand/onboarding/`, which come from the same views.

## Gotchas

- The microphone step opens the real mic, the only step that touches hardware. Leave it promptly.
- The "Not installed" check is based on the folder alone. Point `CODEX_HOME` at a path that doesn't exist; the helper does this for you.
- Expiry is read from the token's `exp`. A fake `connected` home expires 7 days after it was made, so regenerate it rather than reusing an old one.
- The screenshot helper finds the window by its title, `likh`, which it still has without a title bar.
- `Save key` with a real-looking key writes it to the person's credential store. Only prove saving with a key they give you for it; prove refusal with a key that doesn't start `gsk_`.
- `likh onboarding` on its own reads the person's real settings for the starting engine, but saves nothing.
