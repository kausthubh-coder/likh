# murmur

**Talk. It types.** Native dictation for Windows, with macOS in alpha. Hold <kbd>Ctrl</kbd> + <kbd>Win</kbd>, say it, let go, and your words land wherever you're typing.

- **Uses the ChatGPT plan you already have.** murmur transcribes through your Codex sign-in, so there's no API key and nothing extra to pay. Codex doesn't need to be running.
- **Native and light.** Rust, drawn on the CPU, with no browser engine. Idle, it uses about 11 MB of memory and no CPU.
- **Local and observable.** Nothing leaves your machine except the audio you dictate, which goes to ChatGPT. `murmur ctl status` shows what it's doing.

> **Status:** early development. Nothing is released yet. **Codex sign-in:** murmur uses an internal ChatGPT endpoint the same way the Codex app does, and OpenAI can change it at any time. See [docs/codex.md](docs/codex.md).

## Develop

You need Rust (via rustup), on Windows the Visual Studio Build Tools with the C++ workload, and [bun](https://bun.sh).

```sh
bun run setup     # site deps, cargo fetch
bun run doctor    # is everything installed and signed in?
bun run dev       # run murmur (onboarding opens the first time)
bun run check     # what CI runs
```

Start with [AGENTS.md](AGENTS.md) and [docs/plan.md](docs/plan.md). The brand lives in [docs/brand](docs/brand/BRAND.md).

## Credits

murmur builds on ideas from [Handy](https://github.com/cjpais/Handy) (MIT), and on macOS uses its author's [handy-keys](https://crates.io/crates/handy-keys) for the shortcut. The Geist fonts are © Vercel, under the SIL Open Font License.

## License

MIT
