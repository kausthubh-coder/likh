// Writes a throwaway CODEX_HOME in one sign-in state, so likh can be driven
// through every state without touching the real ~/.codex. Prints the path.
// Usage: bun scripts/fake-codex-home.mjs <not-installed|signed-out|connected|expired>
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const state = process.argv[2];
const states = ["not-installed", "signed-out", "connected", "expired"];
if (!states.includes(state)) {
  console.error(`usage: bun scripts/fake-codex-home.mjs <${states.join("|")}>`);
  process.exit(2);
}

const dir = join(import.meta.dir, "..", ".agents", "qa", `codex-${state}`);
rmSync(dir, { recursive: true, force: true });

const jwt = (claims) =>
  [{ alg: "none" }, claims].map((part) => Buffer.from(JSON.stringify(part)).toString("base64url")).join(".") + ".sig";
const now = Math.floor(Date.now() / 1000);

if (state !== "not-installed") {
  mkdirSync(dir, { recursive: true });
  if (state !== "signed-out") {
    const exp = state === "expired" ? now - 3600 : now + 7 * 24 * 3600;
    const auth = {
      auth_mode: "chatgpt",
      tokens: {
        id_token: jwt({ email: "tester@example.com", "https://api.openai.com/auth": { chatgpt_plan_type: "plus" } }),
        access_token: jwt({ exp }),
        refresh_token: "fake-refresh",
        account_id: "fake-account",
      },
    };
    writeFileSync(join(dir, "auth.json"), JSON.stringify(auth, null, 2));
  }
}
console.log(dir);
