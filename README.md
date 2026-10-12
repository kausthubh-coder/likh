# likh

**Talk. It types.** Native dictation for Windows, with macOS in alpha. Hold <kbd>Ctrl</kbd> + <kbd>Space</kbd>, say it, let go, and your words land wherever you're typing.

- **Uses the ChatGPT plan you already have.** likh transcribes through your Codex sign-in, so there's no API key and nothing extra to pay. Codex doesn't need to be running.
- **Native and light.** Rust, drawn on the CPU, with no browser engine. Idle, it uses about 11 MB of memory and no CPU.
- **Local and observable.** Nothing leaves your machine except the audio you dictate, which goes to ChatGPT. `likh ctl status` shows what it's doing.

> **Status:** early development. Nothing is released yet. **Codex sign-in:** likh uses an internal ChatGPT endpoint the same way the Codex app does, and OpenAI can change it at any time. See [docs/codex.md](docs/codex.md).

## Develop

You need Rust (via rustup), on Windows the Visual Studio Build Tools with the C++ workload, and [bun](https://bun.sh).

```sh
bun run setup     # site deps, cargo fetch
bun run doctor    # is everything installed and signed in?
bun run dev       # run likh (onboarding opens the first time)
bun run check     # what CI runs
```

Start with [AGENTS.md](AGENTS.md) and [docs/plan.md](docs/plan.md). The brand lives in [docs/brand](docs/brand/BRAND.md).

## Windows installer

Install [Inno Setup](https://jrsoftware.org/isinfo.php), then run:

```powershell
bun run build:installer
# If ISCC.exe is somewhere else:
powershell -NoProfile -File scripts/build-installer.ps1 -Compiler 'C:\tools\Inno Setup\ISCC.exe'
```

The installer is `target/installer/likh-setup-windows-x64.exe`. It installs to `%LOCALAPPDATA%\Programs\likh`, adds a Start menu shortcut and an uninstall entry, and offers a desktop shortcut. No administrator rights are needed. Finish the installer with **Open likh** checked to launch it.

likh runs in the tray without a terminal. The first launch opens setup; **Finish** opens settings. Closing settings leaves dictation running. Open the shortcut again or double-click the tray icon to open settings. Right-click the tray icon for **Set up again…** or **Quit likh**. **Start with Windows** is a choice in settings.

Settings, history, and setup completion stay in `%LOCALAPPDATA%\likh` across upgrades and uninstall. To start fresh, quit likh, delete that folder, then reopen the installed app. This does not change the Codex sign-in.

## Credits

likh builds on ideas from [Handy](https://github.com/cjpais/Handy) (MIT), and on macOS uses its author's [handy-keys](https://crates.io/crates/handy-keys) for the shortcut. The Geist fonts are © Vercel, under the SIL Open Font License.

## License

MIT
