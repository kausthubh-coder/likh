# Onboarding

The first-run window: five steps in one borderless 560 x 640 window. It opens with the flock landing on the logo, checks Codex live, shows the mic working, teaches the shortcut and ends with the flock spelling murmur. Inside the running app the shortcut step is a real first dictation into a practice box.

## Sub-features

- `onb-welcome` plays the flock into the mark, then "Talk. It types." and a peach `Get started`, with the first of five dots wide. The window then draws nothing until something changes.
- `onb-codex-not-installed` titles the step "Codex isn't installed", with `Check again` (plain text) and `Get Codex`. `Get Codex` opens https://developers.openai.com/codex.
- `onb-codex-signed-out` titles it "Codex isn't signed in", with `Check again`.
- `onb-codex-expired` titles it "Your Codex sign-in expired", with `Check again`.
- `onb-codex-connected` titles it "Connected through Codex", shows the masked email and plan, and offers `Continue`. The link in the art turns green.
- `onb-codex-live` turns not-connected into connected on its own within 2 s once a valid `auth.json` appears.
- `onb-mic` shows nine live bars and the device name; with no mic, "Can't hear a microphone" and `Open settings`.
- `onb-shortcut` lights the `Ctrl` `Space` key caps. Inside the app: holding the real shortcut records, and the words land in the practice box.
- `onb-done` spells murmur in dots and `Finish` closes the window. murmur keeps running.
- `onb-back` returns to the step before from anywhere but welcome and done.
- `onb-drag` moves the window by dragging any empty spot (there is no title bar).

## How to get to it (user POV)

- The first time `murmur` runs. After `Finish` (or Alt+F4) it never opens again; delete `%LOCALAPPDATA%\murmur\onboarded` to see it again.
- `murmur onboarding` opens it on its own, without the app (the shortcut step then only checks the keys).

## Driving it with scripts

Preconditions:

- A fake `CODEX_HOME` for the state you're proving.
- The window was launched with that `CODEX_HOME` (`murmur onboarding`), and you have its PID.

Coordinates are logical client pixels in the 560 x 640 window; there is no title bar to subtract.

- **Welcome.** Wait 2 s for the flock, then capture `welcome.png`.
- **Primary button** (Get started, Continue, Sounds good, Check again, Finish): (280, 557). After 600 ms capture.
- **Back:** (64, 40).
- **Not installed.** `Check again` is left of `Get Codex`, on the same row as the primary button. Only click `Get Codex` when proving that link (it opens the browser).
- **Live re-check.** Launch with the `signed-out` folder, go to step 2, then run `bun scripts/fake-codex-home.mjs connected` and copy `.agents/qa/codex-connected/auth.json` into the signed-out folder. Within 2 s the title reads "Connected through Codex".
- **Microphone.** Reaching step 3 opens the real default mic; Windows shows its mic-in-use icon until you leave the step. Expect your own room noise on the bars.
- **Compare** with the renders in `docs/brand/onboarding/`, which come from the same views.

## Gotchas

- The microphone step opens the real mic, the only step that touches hardware. Leave it promptly.
- The "Not installed" check is based on the folder alone. Point `CODEX_HOME` at a path that doesn't exist; the helper does this for you.
- Expiry is read from the token's `exp`. A fake `connected` home expires 7 days after it was made, so regenerate it rather than reusing an old one.
- The screenshot helper finds the window by its title, `murmur`, which it still has without a title bar.
