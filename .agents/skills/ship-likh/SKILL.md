---
name: ship-likh
description: Take verified likh work to production through a PR, the Vercel site deploy and, when asked, a release, then leave main and the worktree list clean. Use only when I say ship, deploy, release, or merge and deploy.
---

# Ship likh

likh is public (`kausthubh-coder/likh`), so work lands through a PR with CI. Two things reach people:

- **The site** (`getlikh.vercel.app`): Vercel builds every push to `main`. Merging deploys it.
- **The app**: only a `v<version>` tag ships it. `release.yml` builds, signs and publishes a prerelease, and every installed likh finds it within a day. Tag only when I said release (as in "create a release and deploy it"). If I said ship and `app/` changed without a word about a release, merge and ask about the release in the report.

Production needs my go in this conversation. Stop at the first failure. Never force-push, skip a check, or raise a warning cap. For a dry run, run the read-only steps and print each command that pushes, merges, tags or deploys instead of running it.

Below, `<main>` is `C:/Users/kaust/OneDrive/Documents/dev/murmur`, `<branch>` this worktree's branch, `<version>` the one in `app/Cargo.toml`.

## 1. Ready

- `git status --short` is empty.
- `git fetch origin`, then `git rev-list --count HEAD..origin/main` is 0. If not, `git merge origin/main` (a rebase of a pushed branch would need a force-push).
- `bun run check` passes.
- Every changed feature was driven with `verify-likh`, with evidence in `.agents/qa/`. Site changes: a screenshot from `bun run dev:site`.
- `git log --oneline origin/main..HEAD` is what's going out.
- **Release only:** in this branch, bump `version` in `app/Cargo.toml` to the next `0.1.0-alpha.N` and run `cargo build -p likh` so `Cargo.lock` follows. Then update `RELEASE` in `site/app/page.tsx` (`version`, the `windows` URL, and `size` from `bun run build:installer`, the size of `target/installer/likh-setup-windows-x64.exe` in MB). Commit.

## 2. Rollback, written down before anything moves

- Site: `git revert -m 1 <merge sha>` through a new PR, or promote the previous production deployment in Vercel.
- Release: `gh release edit v<version> --draft`. The updater skips drafts, so no more installs pick it up. Copies that already updated stay on it until the next alpha fixes them.

## 3. Land

```sh
git push -u origin <branch>
gh pr create --base main --title "<what changed, plain>" --body "<what and why, the evidence>"
gh pr checks <number> --watch --fail-fast    # bounded: give up after 20 minutes
```

CI is `app (windows-latest)`, `app (macos-latest)`, `site`, and the Vercel preview. When all pass, my "ship" is the review; merge the way #1 to #5 went in:

```sh
gh pr merge <number> --merge
git fetch origin && git rev-parse origin/main    # <merge sha>
```

## 4. Deploy

- **Site:** Vercel deploys the merge on its own. Watch it, bounded to 10 minutes:
  `gh api "repos/kausthubh-coder/likh/deployments?environment=Production&sha=<merge sha>" --jq '.[0].id'`, then `gh api repos/kausthubh-coder/likh/deployments/<id>/statuses --jq '.[0].state'` until `success`.
- **Release:** tag the merge commit and watch the workflow, bounded to 20 minutes (it took about 4):
  ```sh
  git tag v<version> <merge sha>
  git push origin v<version>
  gh run list --workflow release.yml --limit 1 --json databaseId --jq '.[0].databaseId'
  gh run watch <run id> --exit-status
  ```
  The workflow fails if the tag doesn't match `app/Cargo.toml`.

## 5. Check production, read-only

- Site: `curl -s https://getlikh.vercel.app` returns 200 and contains the shipped change (after a release, `v<version>` in the download link).
- Release: `gh release view v<version> --json isPrerelease,assets` lists `likh-setup-windows-x64.exe` and its `.sig`, and `curl -s "https://api.github.com/repos/kausthubh-coder/likh/releases?per_page=1"` (what the updater reads) leads with `v<version>`.
- Never install it. I update my own likh ("Restart to update").

## 6. Update the main checkout

`git -C <main> status --short` and `git -C <main> branch --show-current`. Clean and on `main`: `git -C <main> pull --ff-only`, then `bun run setup` in `<main>`. Dirty or on another branch: leave it and tell me.

## 7. Clean up

- `git push origin --delete <branch>`.
- If this thread runs in the worktree, leave it and say so. Otherwise, once no live thread uses it (`t3_thread_list`): remove it as `worktree-likh` says (Cleanup), with `git -C <main> branch -D <branch>` once `gh pr view <number> --json state` says `MERGED`.

## 8. Report

A few lines: what shipped (PR, merge sha, tag), where (site URL, release URL), the evidence, and anything skipped or left, like a release I didn't tag or a main checkout I didn't pull.
