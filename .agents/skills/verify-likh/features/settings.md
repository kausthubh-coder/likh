# Settings

The likh window, opened from the tray (`Open likh`, or a double click on the icon). It's borderless, 720 x 520, with five tabs down the left. Every change saves at once to `settings.json` in likh's data folder, and the next dictation uses it.

## Sub-features

- `set-shortcut` General shows the shortcut as key caps; `Change` captures a new one, and Esc keeps the old one.
- `set-mouse` General: a mouse button picked from Off · Middle · Back · Forward works like the shortcut. Off by default. Picking one installs the mouse hook; Off removes it.
- `set-mode` General: Hold or tap, Hold only, Tap to toggle.
- `set-silence` General: Stop after quiet, picked from Never · 3 s · 5 s · 10 s (5 s by default). Hands-free only.
- `set-switches` General: Sounds, Pause music and videos (Windows only), Keep history, Start with Windows.
- `set-engines` Transcription: one card per engine (ChatGPT, Groq, and Windows on Windows), each with its glyph and a word on its state ("Connected", "Needs a key", "Turned off"...). Opening the tab checks all of them. The picked card has the peach edge, and the state, fix and actions for it show underneath.
- `set-groq` Transcription with Groq: a hidden key field, `Save key` (refuses anything not starting `gsk_`, otherwise asks Groq, then saves it to the credential store) and `Get a free key`. Once saved: `Use another key` and `Remove key`.
- `set-windows` Transcription with Windows: `Open speech settings` and `Check again`.
- `set-commands` Commands: a master switch, then four groups (Lines and lists, Press a key, Take it back, Punctuation by name), each with its own switch and its phrases as capsules. Punctuation starts off. With the master switch off, the groups are hidden and a line says everything is typed as said.
- `set-history` History: newest first, click to copy, and `Clear` asks once.
- `set-about` About: the engine in use and its state, the session's dictation count and latency, links, and `Run setup again`.

## How to get to it (user POV)

- Right-click the tray icon and choose `Open likh`, or double-click the icon. After onboarding's `Finish` it opens by itself.

## Driving it with scripts

Preconditions:

- A test instance with its own `LIKH_HOME` and a fake `CODEX_HOME` (see `../SKILL.md`, Launch), its PID captured by path.
- Open the window: `LIKH_HOME=... ./target/release/likh.exe ctl open` from Bash (or `Start-Process -Wait` in PowerShell).

Logical coordinates in the 720 x 520 window:

- **Tabs:** x 92; General y 84, Transcription y 120, Commands y 157, History y 194, About y 231.
- **Engine cards** (Transcription): ChatGPT (289, 172), Groq (444, 172), Windows (598, 172).
- After each click, wait about 600 ms, then `scripts/screenshot-window.ps1 -ProcessId <pid> -Out .agents/qa/settings/<tab>.png`.
- **Saved?** Read `$LIKH_HOME/settings.json`: `engine`, `mouse`, `silence_stop` (`"off"`, `"3s"`, `"5s"`, `"10s"`), `pause_media`, `command_groups`.
- **Compare** with `docs/brand/panel/*.png`, rendered from the same views by `cargo test -p likh brand_board_panel -- --ignored`.

## Gotchas

- Don't pick a mouse button unless the person asks: the hook blocks that button's normal click everywhere until it's set back to Off or likh quits.
- Don't press `Save key` with a real-looking key, and don't press `Remove key`: both change the person's credential store. Prove refusal with a key that doesn't start `gsk_`.
- `Start with Windows` writes the person's Run key, even from a `LIKH_HOME` instance. Leave it alone.
- `Get a free key`, `Open speech settings`, `Get Codex` and the About links open the browser or Windows Settings. Click them only when proving that link.
