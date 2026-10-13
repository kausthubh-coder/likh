// Runs the site's dev server on this checkout's port: PORT in the root
// .env.local, written by scripts/setup.mjs. Bun loads that file into this
// process; `bun run` alone doesn't pass it on to a package.json script, and
// next dev only reads PORT from its environment.
// Usage: bun scripts/site.mjs
import { spawnSync } from "node:child_process";
import { join } from "node:path";

if (!process.env.PORT) console.warn("No PORT in .env.local, so next dev picks one. Run bun run setup.");
const result = spawnSync("bun", ["run", "dev"], { cwd: join(import.meta.dir, "..", "site"), stdio: "inherit", env: process.env });
process.exit(result.status ?? 1);
