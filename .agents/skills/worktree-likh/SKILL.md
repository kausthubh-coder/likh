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

Healthy ends with no `FAIL` lines and one summary line. A `warn` on the debug build means nothing is built yet; on Codex sign-in it's fine for UI work.

```
ready: <checkout> · site port 3796 · LIKH_HOME .agents/qa/home-3d510e78 (pipe likh-kaust-home-3d510e78)
```

What setup gives this checkout:

- **Site deps** in `site/node_modules`, reinstalled only when `site/bun.lock` is newer.
- **Rust deps** downloaded (`cargo fetch`), not built. The first `cargo build -p likh` or `bun run dev` in a checkout takes about 3 minutes; doctor warns until then. Each checkout builds into its own `target/`. Keep it that way: with a shared `CARGO_TARGET_DIR` or `build-dir`, every checkout's binary is the same `deps/likh.exe` on Windows, and one worktree's build hands another its `likh.exe`.
- **Its own `LIKH_HOME`** at `.agents/qa/home-<hash of the path>`, in the root `.env.local`. The app keeps its settings, history, onboarding marker and `update-checked` there, and names its `likh ctl` pipe after the folder. Setup seeds it with `auto_update: false`, so nothing downloads by itself.
- **Its own site port**, `PORT` in the root `.env.local`, one no other worktree claims. `bun run dev:site` (`scripts/site.mjs`) hands it to `next dev`; a `PORT` in `site/.env.local` is ignored.

There is no backend to isolate. Dictation reads the real `~/.codex/auth.json`, read-only; use a fake `CODEX_HOME` for UI proofs (`verify-likh`).

## Running it

- `bun run dev`: the app with this checkout's `LIKH_HOME`. Start it in a background Bash job; it holds the shell until quit.
- `bun run likh <args>`: any `likh` command with the same home, e.g. `bun run likh ctl status`, `bun run likh run --skip-onboarding`.
- Calling `./target/debug/likh.exe` directly: set `LIKH_HOME` from `.env.local` first, for the app and for every `ctl` call.
- `bun run dev:site`: the landing page on `http://localhost:<PORT>`, also in a background job.
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
- The site: stop the `node.exe` processes whose command line has this checkout's `site` path (`Get-CimInstance Win32_Process`), by PID.
- Remove a worktree only when its work is merged or I say so, and never one a live thread is using (`t3_thread_list`, its `worktreePath`): `git -C <main> worktree remove <path>`, then `git -C <main> branch -d <branch>`. Its `LIKH_HOME` goes with it. Git leaves the `.claude\skills` junction behind: `cmd /c rmdir <path>\.claude\skills` removes the link only, then delete the empty folder.
