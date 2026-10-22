---
name: ship-likh
description: Take verified likh work to production through a PR, the Vercel site deploy and, when app code changed, a release, then leave main and the worktree list clean. Use only when I say ship, deploy, release, or merge and deploy.
---

# Ship likh

likh is public (`kausthubh-coder/likh`), so work lands through a PR with CI. Two things reach people:

- **The site** (`getlikh.vercel.app`): Vercel builds every push to `main`. Merging deploys it.
- **The app**: only a `v<version>` tag ships it. `release.yml` builds, signs and publishes a prerelease with the Windows installer, then adds the macOS DMG (`scripts/build-mac-app.sh`), and every installed likh finds it within a day. When the change touches what goes into the app (`app/src/`, `app/assets/`, the dependencies in `app/Cargo.toml`, `installer/`), the release is part of the ship: do it in steps 1 and 4, don't ask about it at the end. Skip it only when I say so ("no release"), and say in the report that it was skipped. Tooling, docs, skills and site-only changes have no release.

## Rules

- Production needs my go in this conversation. "Ship" is that go for every step below, release and cleanup included: don't stop to ask between steps.
- When a step fails because of this change (a failing check, a CI job, a deploy), fix it, rerun the checks and carry on. Stop and tell me only when the cause is outside the change or the fix needs my decision.
- Never end the turn while a CI run, deploy or release build you started is queued or running. Watch it to the end (each step gives its bound).
- Never force-push, skip a check, or raise a warning cap.
- For a dry run, run the read-only steps and print each command that pushes, merges, tags or deploys instead of running it.

Below, `<main>` is `C:/Users/kaust/OneDrive/Documents/dev/murmur`, `<branch>` this worktree's branch, `<version>` the one in `app/Cargo.toml`.

## From a cloud session

A cloud session (Linux VM, see `## Cloud sessions` in AGENTS.md) ships the same way with three differences:

- `gh pr` doesn't work there (no GraphQL). Use the REST calls under each step marked **Cloud**.
- There's no `<main>` checkout and no worktree to remove: skip step 6 and the worktree half of step 7.
- Releases run from my machine: the installer is built on Windows for `RELEASE` in `site/app/page.tsx`. When the change needs a release, land it, then make the report's first line `Release not done: run ship-likh for the release on Windows`, with the version it should get. That line replaces the release, it isn't a question.

## 1. Ready

- `git status --short` is empty.
- `git fetch origin`, then `git rev-list --count HEAD..origin/main` is 0. If not, `git merge origin/main` (a rebase of a pushed branch would need a force-push).
- `bun run check` passes.
- Every changed feature was driven with `verify-likh`, with evidence in `.agents/qa/`. Site changes: a screenshot from `bun run dev:site`.
- `git log --oneline origin/main..HEAD` is what's going out.
- **Release** (app changed, on Windows): in this branch, bump `version` in `app/Cargo.toml` to the next `0.1.0-alpha.N` and run `cargo build -p likh` so `Cargo.lock` follows. Then update `RELEASE` in `site/app/page.tsx` (`version`, the `windows` URL, `size` from `bun run build:installer`, the size of `target/installer/likh-setup-windows-x64.exe` in MB, and `mac`: `${REPO}/releases/download/v<version>/likh-macos.dmg`). Commit, and rerun `bun run check`.

## 2. Rollback, written down before anything moves

- Site: `git revert -m 1 <merge sha>` through a new PR, or promote the previous production deployment in Vercel.
- Release: `gh release edit v<version> --draft`. The updater skips drafts, so no more installs pick it up. Copies that already updated stay on it until the next alpha fixes them.

## 3. Land

```sh
git push -u origin <branch>
gh pr create --base main --title "<what changed, plain>" --body "<what and why, the evidence>"
gh pr checks <number> --watch --fail-fast    # bounded: give up after 20 minutes
```

**Cloud:**

```sh
git push -u origin <branch>
gh api repos/kausthubh-coder/likh/pulls -f head=<branch> -f base=main -f title="<title>" -f body="<body>" --jq .number
gh api repos/kausthubh-coder/likh/commits/<head sha>/check-runs --jq '.check_runs[] | [.name, .status, .conclusion] | @tsv'
```

Repeat the check-runs call about once a minute, bounded to 20 minutes, until every run is `completed`.

CI is `app (windows-latest)`, `app (macos-latest)`, `site`, and the Vercel preview. A red check caused by this change: fix it on the branch, rerun `bun run check`, push, and watch again. When all pass, my "ship" is the review; merge the way #1 to #5 went in, with a merge commit:

```sh
gh pr merge <number> --merge
git fetch origin && git rev-parse origin/main    # <merge sha>
```

**Cloud:** `gh api -X PUT repos/kausthubh-coder/likh/pulls/<number>/merge -f merge_method=merge --jq .sha` prints `<merge sha>`.

## 4. Deploy

- **Site:** Vercel deploys the merge on its own. Watch it, bounded to 10 minutes:
  `gh api "repos/kausthubh-coder/likh/deployments?environment=Production&sha=<merge sha>" --jq '.[0].id'`, then `gh api repos/kausthubh-coder/likh/deployments/<id>/statuses --jq '.[0].state'` until `success`. Works the same in the cloud.
- **Release** (app changed, on Windows): tag the merge commit and watch the workflow, bounded to 20 minutes (it took about 4):
  ```sh
  git tag v<version> <merge sha>
  git push origin v<version>
  gh run list --workflow release.yml --limit 1 --json databaseId --jq '.[0].databaseId'
  gh run watch <run id> --exit-status
  ```
  Watch both jobs: `windows` publishes, then `macos` adds the DMG. The macOS app is Developer ID signed and notarized only when the `MAC_CERTIFICATE`, `MAC_CERTIFICATE_PASSWORD`, `MAC_SIGNING_IDENTITY`, `MAC_NOTARY_APPLE_ID`, `MAC_NOTARY_TEAM_ID` and `MAC_NOTARY_PASSWORD` secrets exist; without them it's ad hoc signed and the release notes should say Mac users right-click, Open the first time. The workflow fails if the tag doesn't match `app/Cargo.toml`. If it fails on this change, fix it through a new PR (steps 1 to 3) and tag the next alpha; a published tag is never moved.

## 5. Check production, read-only

- Site: `curl -s https://getlikh.vercel.app` returns 200 and contains the shipped change (after a release, `v<version>` in the download link).
- Release: `gh release view v<version> --json isPrerelease,assets` lists `likh-setup-windows-x64.exe`, its `.sig` and `likh-macos.dmg`, and `curl -s "https://api.github.com/repos/kausthubh-coder/likh/releases?per_page=1"` (what the updater reads) leads with `v<version>`.
- Never install it. I update my own likh ("Restart to update").

## 6. Update the main checkout

`git -C <main> status --short` and `git -C <main> branch --show-current`. Clean and on `main`: `git -C <main> pull --ff-only`, then `bun run setup` in `<main>`. Dirty or on another branch: leave it and tell me.

## 7. Clean up

- `git push origin --delete <branch>`. **Cloud:** keep the session's own branch if the cloud named it for this session; it expects that branch pushed.
- If this thread runs in the worktree, leave it and say so. Otherwise, once no live thread uses it (`t3_thread_list`): remove it as `worktree-likh` says (Cleanup), with `git -C <main> branch -D <branch>` once `gh pr view <number> --json state` says `MERGED`.

## 8. Report

A few lines: what shipped (PR, merge sha, tag), where (site URL, release URL), the evidence, and anything skipped or left, like a release I said to skip or a main checkout I didn't pull. From the cloud, a release that's still owed is the first line.
