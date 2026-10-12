# Status CLI

`murmur status` prints one line saying whether murmur can transcribe right now. `murmur status --json` gives agents the same thing as JSON. Neither ever prints a token or a full email address.

## Sub-features

- `status-human` prints `murmur <version> · <title> · <detail>` using the onboarding copy.
- `status-json` prints `{"version", "codex": {"state", "email_hint", "plan", "expires_at"}}`.
- `status-no-secrets` never shows token text (`eyJ…`), `refresh_token`, `account_id` or a full email.

## How to get to it (user POV)

- Run `murmur status` or `murmur status --json` in a terminal.

## Driving it with scripts

Preconditions:

- `cargo build -p murmur` has succeeded.

- **Each state.** For each of `not-installed`, `signed-out`, `connected` and `expired`, run `CODEX_HOME="$(bun scripts/fake-codex-home.mjs <state>)" ./target/debug/murmur.exe status --json`. The exit code is 0, and `codex.state` is `not_installed`, `signed_out`, `connected` or `expired` respectively.
- **Human line.** Run the same with `status` in place of `status --json`. You get one line whose title matches the onboarding card.
- **No secrets.** Pipe the connected output through `grep -E "eyJ|fake-refresh|fake-account|tester@"`. Nothing should match.

## Gotchas

- Without `CODEX_HOME`, `status` reads the developer's real `~/.codex`. That's fine to read (it prints only the masked hint), but never save it as evidence in a public place.
