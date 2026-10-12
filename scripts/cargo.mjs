// Runs cargo with the given arguments, finding it even in a terminal opened
// before rustup was installed (rustup adds ~/.cargo/bin to PATH only for new
// shells). Every `bun run` script that needs cargo goes through here.
// Usage: bun scripts/cargo.mjs run -p murmur
import { spawnSync } from "node:child_process";
import { homedir } from "node:os";
import { delimiter, join } from "node:path";

export const cargoEnv = {
  ...process.env,
  PATH: `${join(homedir(), ".cargo", "bin")}${delimiter}${process.env.PATH ?? ""}`,
};

if (import.meta.main) {
  const result = spawnSync("cargo", process.argv.slice(2), { stdio: "inherit", env: cargoEnv, shell: false });
  if (result.error) {
    console.error(`Couldn't run cargo: ${result.error.message}. Install Rust from https://rustup.rs, then run bun run setup.`);
    process.exit(1);
  }
  process.exit(result.status ?? 1);
}
