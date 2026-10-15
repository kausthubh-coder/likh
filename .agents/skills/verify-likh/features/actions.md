# Actions

Hold the action key (Ctrl+Shift+Space by default, or the mouse button picked for it), say what to do, and likh does it: opens a link, presses keys, pastes text, asks ChatGPT, or runs a program. Each action is one TOML file in `LIKH_HOME/actions/`; the format is in `app/src/actions/starters/README.md`, which likh also writes into the folder.

## Sub-features

- `act-match` the words said (case, punctuation and apostrophes ignored) match an action's phrase exactly; a `{slot}` that isn't a list catches the rest and must come last; a `{slot}` named after a `[lists.x]` matches one of its names and becomes its value. Spelled-out words beat slots. Action speech never goes through cleanup or spoken commands; dictionary words get their spelling back in slot text.
- `act-router` no phrase matched: ChatGPT (`gpt-6-luna`, tools on `/backend-api/codex/responses`) picks from actions with `router = true`, or none ("No action for “…”" note). About 1.1 to 1.7 s (2026-10-15).
- `act-steps` `paste`, `keys` (chords like `ctrl+shift+t`, media keys like `next_track`), `open` (what was said is URL-encoded, list values aren't), `wait`, `ask` (ChatGPT's answer becomes `{result}`), `run` (no shell, no slots; `LIKH_TEXT` and the rest in the environment and as JSON on stdin; stdout is the result; 30 s timeout).
- `act-output` `paste`, `replace` (over the selection it read with Ctrl+C), `notify` (the pill's check and words for 2 s), `none`. If the window in front changed since the key went down, a paste goes to the clipboard: "Copied. The window changed."
- `act-trust` an action with a `run` step runs only after Approve on the Actions tab, and again after any change to its file or a script it runs. Otherwise the pill says "<file> isn't approved yet, so it won't run." with Review.
- `act-confirm` `confirm = true` (or a `run` the router picked): the pill asks "Run <name>?" with a peach ring; the action key again runs it, Esc or 5 s cancels.
- `act-busy` one action at a time: the action key meanwhile shows "Busy: <name>"; Esc stops a running one.
- `act-tab` the Actions tab: the action key (Change, Turn off; says when another app owns it) and its mouse button, every action with its phrases and a switch (writes `enabled` in the file), broken files with file and line, Review (the diff since approval) and Approve, New action (paste, open, keys or ask).
- `act-cli` `likh actions check` and `likh actions test "<phrase>"` work without likh running and never run a step.
- `act-history` actions go to history with their name; only paste and replace count for paste last.

## How to get to it (user POV)

- Hold Ctrl+Shift+Space, say "new thread fix the login bug", let go: T3 Code opens with that as a draft.
- Tray, Open likh, Actions.

## Driving it with scripts

- **One action end to end, quietly.** `.agents/qa/actions/run-one.sh` (kept in the gitignored qa folder; recreate it from this recipe if missing): start likh with `LIKH_AUDIO=<wav>`, `LIKH_TRANSCRIBER="echo:<what was said>"`, `LIKH_PASTE=<log>` and a fake `CODEX_HOME`, then `likh ctl action-start`, wait the WAV's length, `likh ctl action-stop`, wait 3 s, read the log. With `LIKH_PASTE` set, `open` logs `{"open": …}`, `keys` logs `{"press": …}`, the selection is `LIKH_SELECTION` (logged as `{"press":"copy"}`), and `LIKH_ASK=<text>` answers every `ask`.
- 2026-10-15, debug build, fakes:
  - "New thread, fix the flaky login test." gave `{"open":"t3code://new?prompt=fix%20the%20flaky%20login%20test"}`.
  - "Spotify, play rainy day jazz." gave `{"open":"spotify:search:rainy%20day%20jazz"}`.
  - "Next song." gave `{"press":"next_track"}`.
  - "Make this formal." with a selection gave `{"press":"copy"}` then `{"paste":…}`.
  - "Put on some lo-fi beats in Spotify." with the real `CODEX_HOME` (text only to ChatGPT) gave `{"open":"spotify:search:some%20lo-fi%20beats"}`.
  - `confirm = true`: a second `ctl action-start` pasted; left alone, nothing after 5 s.
  - An unapproved `run`: nothing ran, and the pill showed the Review problem.
- **Router alone.** `cargo test -p likh router_live -- --ignored --nocapture` and `ask_live`.
- **Views.** `cargo test -p likh brand_board -- --ignored` renders `docs/brand/pill/action-*.png` and `docs/brand/panel/actions*.png`.
- **Pill on screen.** `scripts/screenshot-pill.ps1` during a run. It captures the person's screen around the pill: delete the frames once you've looked.

## Gotchas

- The person's installed likh doesn't use the action key, but other apps might: on the dev machine Ctrl+Shift+Space was taken by another app (2026-10-15). The log says "the action key won't work", and the Actions tab says so under Action key.
- An approval can't be given from `likh ctl` on purpose. To prove approve-then-run in the live app, click Review and Approve in the window (`scripts/click-window.ps1`; it moves the person's cursor), or rely on the panel test `approving_a_script_after_reading_it_lets_it_run` plus `a_program_gets_what_you_said_and_its_output_is_the_result`.
- `printf` into a TOML file turns `\n` into a real newline inside a quoted string, which breaks the file; use a heredoc with `"""`.
