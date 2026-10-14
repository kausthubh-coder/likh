# Settings

The likh window, opened from the tray (`Open likh`, or a double click on the icon). It's borderless, 720 x 580, with six tabs down the left and the version and update button at their foot. Every setting is one row in a panel, and every tab fits without scrolling except a long History. Every change saves at once to `settings.json` in likh's data folder, and the next dictation uses it.

## Sub-features

- `set-shortcut` Dictation: the shortcut as key caps beside a small keyboard with its keys lit. `Change` listens: each key you press lights on the keyboard and flies into a slot as a cap; the saved shortcut pulses once and the line under it turns green. `Cancel` or Esc keeps the old one. It asks for frames only while a cap is moving.
- `set-mouse` Dictation: a mouse button picked from Off · Middle · Back · Forward works like the shortcut. Off by default. Picking one installs the mouse hook; Off removes it.
- `set-mode` Dictation: Hold or tap · Hold · Toggle, with one line under `Mode` saying what the picked one does.
- `set-silence` Dictation: Stop after quiet, picked from Never · 3 s · 5 s · 10 s (5 s by default). Hands-free only.
- `set-switches` Dictation: Sounds, Pause music and videos (Windows only). History: Keep history. App: Start with Windows, Check for updates automatically. The whole row flips its switch.
- `set-engines` Transcription: one panel per engine (ChatGPT, Groq), each always showing its own setup under its name: the Codex sign-in, or the Groq key field. Picking one moves only the peach edge, radio and glyph; nothing else moves. Opening the tab checks Codex.
- `set-groq` Transcription, Groq's panel: a hidden key field and `Save key` (peach while Groq is picked; refuses anything not starting `gsk_`, otherwise asks Groq, then saves it to the credential store) and `Get a free key`. Once saved: `Use another key` and `Remove`. Its messages show in the header.
- `set-commands` Commands: one panel, the master switch first, then four groups (Lines and lists, Press a key, Take it back, Punctuation by name), each with its own switch and its phrases as capsules. Punctuation starts off. With the master switch off, the groups are hidden and a line says everything is typed as said.
- `set-history` History: Keep history, then the dictations newest first in one panel; click to copy, and `Clear` asks once (for 3 s).
- `set-app` App: startup and update switches, where updating stands (`Restart to update` once one is downloaded), this session's dictation count and latency, then Website, Source on GitHub and `Run setup again`.
- `set-update` Sidebar foot, on every tab: the version, and next to it one button that checks for updates, spins while checking, and turns peach (`v… ready`) once there's one to restart into.

## How to get to it (user POV)

- Right-click the tray icon and choose `Open likh`, or double-click the icon. After onboarding's `Finish` it opens by itself.

## Driving it with scripts

Preconditions:

- A test instance with its own `LIKH_HOME` and a fake `CODEX_HOME` (see `../SKILL.md`, Launch), its PID captured by path.
- Open the window: `LIKH_HOME=... ./target/release/likh.exe ctl open` from Bash (or `Start-Process -Wait` in PowerShell).

Logical coordinates in the 720 x 580 window:

- **Tabs:** x 92; Dictation y 84, Transcription y 116, Commands y 149, Dictionary y 182, History y 215, App y 248.
- **Update button:** (154, 542).
- **Microphone** (Dictation): Test (413, 415), the list (560, 415). Test opens the mic (Windows shows its mic icon) until Stop or another tab.
- **Engines** (Transcription): ChatGPT (400, 135), Groq (400, 266).
- **Change** (Dictation): (405, 150).
- After each click, wait about 600 ms, then `scripts/screenshot-window.ps1 -ProcessId <pid> -Out .agents/qa/settings/<tab>.png`.
- **Saved?** Read `$LIKH_HOME/settings.json`: `engine`, `mouse`, `mode`, `silence_stop` (`"off"`, `"3s"`, `"5s"`, `"10s"`), `pause_media`, `command_groups`, `shortcut`.
- **Compare** with `docs/brand/panel/*.png`, rendered from the same views by `cargo test -p likh brand_board_panel -- --ignored` (including `dictation-picking` and `dictation-saved`).

## Gotchas

- Don't pick a mouse button unless the person asks: the hook blocks that button's normal click everywhere until it's set back to Off or likh quits.
- Don't save a new shortcut in a test instance unless that's what you're proving: it registers system-wide and takes the keys from the person's own likh.
- Don't press `Save key` with a real-looking key, and don't press `Remove`: both change the person's credential store. Prove refusal with a key that doesn't start `gsk_`.
- `Start with Windows` writes the person's Run key, even from a `LIKH_HOME` instance. Leave it alone.
- `Get a free key`, `Get Codex` and App's links open the browser. Click them only when proving that link.
