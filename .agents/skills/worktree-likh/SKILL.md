---
name: worktree-likh
description: Make a likh checkout work on its own, apart from my installed likh and every other worktree. Use at the start of every task in a new worktree, when deps, the build, the site port or LIKH_HOME look wrong, and before giving a helper agent its own checkout.
---

# Worktree likh

## Setup

```sh
bun run setup    # safe to rerun; seconds when nothing is missing
bun run doctor   # read-only; every FAIL line ends with its fix
```

Healthy ends with every doctor line `ok` (a `warn` on Codex sign-in is fine for UI work) and one summary line:

```
ready: <checkout> · site port 3796 · LIKH_HOME .agents/qa/home-3d510e78 (pipe likh-kaust-home-3d510e78)
```

What setup gives this checkout:

- **Site deps** in `site/node_modules`, reinstalled only when `site/bun.lock` is newer.
- **Its own `target/debug/likh.exe`.** Rust intermediates go to `~/.cargo/likh-build`, shared by every checkout (an untracked `.cargo/config.toml`), so a new worktree compiles only likh itself. The binary stays per checkout, so a running likh never blocks another worktree's build. Builds in two checkouts take turns on the shared dir's lock; "Blocking waiting for file lock on build directory" means wait.
- **Its own `LIKH_HOME`** at `.agents/qa/home-<hash of the path>`, written to `.env.local`, which bun loads for every `bun run`. The app keeps its settings, history, onboarding marker and `update-checked` there, and names its `likh ctl` pipe after the folder. Setup seeds it with `auto_update: false`, so nothing downloads by itself.
- **Its own site port** in `site/.env.local`, one no other worktree claims.

There is no backend to isolate. Dictation reads the real `~/.codex/auth.json`, read-only; use a fake `CODEX_HOME` for UI proofs (`verify-likh`).

## Running it

- `bun run dev`: the app with this checkout's `LIKH_HOME`. Start it in a background Bash job; it holds the shell until quit.
- `bun run likh <args>`: any `likh` command with the same home, e.g. `bun run likh ctl status`, `bun run likh run --skip-onboarding`.
- Calling `./target/debug/likh.exe` directly: set `LIKH_HOME` from `.env.local` first, for the app and for every `ctl` call.
- `bun run dev:site`: the landing page on the port from `site/.env.local`.
- Two checkouts can run at once. Their pipes and homes differ, and the first likh to start takes Ctrl+Space; the others say so and still answer `likh ctl start`.

## My installed likh

It lives in `%LOCALAPPDATA%\Programs\likh`, keeps its data in `%LOCALAPPDATA%\likh`, and starts from the `likh` value in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. Agents leave all three alone:

- Run every likh of yours with this checkout's `LIKH_HOME`. Without it, `likh run` opens my likh's window or runs on my settings and update check.
- Leave the Start with Windows switch alone, even in your own instance: it writes my Run key.
- Install nothing: no `likh ctl install-update`, no installer from `target/installer`. Either replaces my app.
- Stop only the PID you started, found by this checkout's path: `(Get-Process likh | Where-Object { $_.Path -like "$PWD\target\*" }).Id`.

When my likh is running, it owns Ctrl+Space; drive yours with `likh ctl`.

## Main checkout

`C:\Users\kaust\OneDrive\Documents\dev\murmur` (the folder still has the old name; `git rev-parse --git-common-dir` finds it). Never edit or pull it from a task. `ship-likh` updates it.

## Helpers

One checkout per agent, never two agents in one:

```sh
git -C C:/Users/kaust/OneDrive/Documents/dev/murmur fetch origin
git -C C:/Users/kaust/OneDrive/Documents/dev/murmur worktree add C:/Users/kaust/.t3/worktrees/murmur/<name> -b <branch> origin/main
cd C:/Users/kaust/.t3/worktrees/murmur/<name> && bun run setup
```

## Cleanup

- `bun run likh ctl quit`, then check your PID is gone; `Stop-Process -Id <pid>` if not. Never stop likh by name.
- Remove a worktree only when its work is merged or I say so, and never one a live thread is using (`t3_thread_list`, its `worktreePath`): `git -C <main> worktree remove <path>`, then `git -C <main> branch -d <branch>`. Its `LIKH_HOME` goes with it.
