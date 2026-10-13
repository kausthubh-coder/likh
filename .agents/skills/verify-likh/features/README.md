# likh verification map

This folder is the maintained list of what a user can do with likh and how to prove each one. Read it before driving the app, then use the matching feature file as the recipe.

## Baseline preconditions

- `cargo build -p likh` succeeded on this checkout.
- A fake `CODEX_HOME` from `bun scripts/fake-codex-home.mjs <state>`. Never the real `~/.codex`.
- The app was launched by this run, and its PID was captured by path.
- Yours runs with this checkout's `LIKH_HOME` from `bun run setup`, whether or not the person's own likh is running (see `../SKILL.md`, Launch).

## Driving conventions

- Click with `scripts/click-window.ps1` using logical coordinates from the feature file.
- Wait about 600 ms after a click before capturing (step transitions are springs).
- Treat copy as literal. It must match the table in `docs/brand/BRAND.md`.

## Proof and skip reporting

- Save the before and after screenshots, plus the action between them, under `.agents/qa/<feature>/`.
- CLI proof is the command, its stdout and its exit code.
- If a path can't be reached, report the command you tried and the precondition that wasn't met. Never count a skipped entry point as verified through another one.

## Feature entry contract

Each file has an H1 and one paragraph, then four H2s in this order: `Sub-features`, `How to get to it (user POV)`, `Driving it with scripts`, `Gotchas`.

## Features

- [Dictation](./dictation.md): hold Ctrl+Space, talk, let go; silence stop; spoken commands; pause media; the pill; `likh ctl`; idle cost.
- [Onboarding](./onboarding.md): the five first-run steps, picking an engine (ChatGPT, Groq, Windows) and its states, the mic and the practice dictation.
- [Settings](./settings.md): the likh window from the tray: General, Transcription, Commands, History, About.
- [Updates](./updates.md): finding, verifying and installing a newer release; the release workflow.
- [Status CLI](./status-cli.md): `likh status` and `likh status --json`.
