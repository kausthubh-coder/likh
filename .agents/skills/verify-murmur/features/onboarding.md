# Onboarding

The first-run window greets the user, then checks whether Codex is installed and signed in. For every state it explains what's wrong and offers the fix. While it isn't connected it re-checks every 2 s, so signing in to Codex in another window flips it to connected without a click.

## Sub-features

- `onb-welcome` shows the logo mark, "Talk. It types." and a peach `Get started` button, with step 1 of 6 marked.
- `onb-codex-not-installed` shows "Codex isn't installed" with `Check again` and `Get Codex`. `Get Codex` opens https://developers.openai.com/codex.
- `onb-codex-signed-out` shows "Codex isn't signed in" with `Check again`.
- `onb-codex-expired` shows "Your Codex sign-in expired" with `Check again`.
- `onb-codex-connected` shows "Connected through Codex", the masked email and the plan, with `Continue`.
- `onb-codex-live` turns not-connected into connected on its own within 2 s once a valid `auth.json` appears.
- `onb-back` takes the user from step 2 back to step 1.

## How to get to it (user POV)

- Launch murmur with no arguments, or with `murmur onboarding`.
- (Phase 2: on first launch, and from the tray menu.)

## Driving it with scripts

Preconditions:

- A fake `CODEX_HOME` for the state you're proving.
- The app was launched with that `CODEX_HOME`, and you have its PID.

Coordinates are logical client pixels in the 560 x 640 window.

- **Welcome.** Capture `welcome.png`. The mark is centred, "Talk. It types." sits above a peach `Get started`, and the first dot is the wide one.
- **Get started.** Click (280, 576). After 600 ms capture `codex.png`. The step label reads `2 / 6` and the second dot is the wide one.
- **Connected.** The primary button at (280, 533) reads `Continue`, and `Back` is at (280, 576). The card shows `t•••@example.com · ChatGPT Plus` for the fake account.
- **Not installed.** `Check again` is at (140, 533) and `Get Codex` at (420, 533). Clicking `Get Codex` opens the default browser, so only click it when proving that link.
- **Signed out / expired.** One `Check again` at (280, 533).
- **Live re-check.** Launch with the `signed-out` folder, go to step 2, then run `bun scripts/fake-codex-home.mjs connected` and copy `.agents/qa/codex-connected/auth.json` into the signed-out folder. Within 2 s the card reads "Connected through Codex".
- **Back.** Click (280, 576) on step 2 and capture. Step 1 is showing again.

## Gotchas

- `Continue` doesn't go anywhere yet; steps 3 to 6 land in phase 3. Don't report that as a bug.
- The "Not installed" check is based on the folder alone. Point `CODEX_HOME` at a path that doesn't exist; the helper does this for you.
- Screenshots include the title bar (about 30 logical px). Subtract it before turning screenshot pixels into click coordinates.
- Expiry is read from the token's `exp`. A fake `connected` home expires 7 days after it was made, so regenerate it rather than reusing an old one.
