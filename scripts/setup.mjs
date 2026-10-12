// Prepares a fresh checkout or worktree: site deps, Rust deps, and a free port
// for the site so parallel worktrees don't collide. T3 Code runs this on
// worktree create. Safe to run again.
import { execSync } from "node:child_process";
import { existsSync, mkdirSync, symlinkSync, writeFileSync, readFileSync } from "node:fs";
import { createServer } from "node:net";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const run = (cmd, cwd = root) => {
  console.log(`> ${cmd}`);
  execSync(cmd, { cwd, stdio: "inherit" });
};

const freePort = (port) =>
  new Promise((resolve) => {
    const server = createServer();
    server.once("error", () => resolve(freePort(port + 1)));
    server.listen(port, "127.0.0.1", () => server.close(() => resolve(port)));
  });

// Claude Code reads .claude/skills, Codex reads .agents/skills: one folder, linked.
// A junction on Windows needs no admin rights.
const skillsLink = join(root, ".claude", "skills");
if (!existsSync(skillsLink)) {
  mkdirSync(join(root, ".claude"), { recursive: true });
  symlinkSync(join(root, ".agents", "skills"), skillsLink, "junction");
  console.log("linked .claude/skills -> .agents/skills");
}

run("bun install", join(root, "site"));
run("cargo fetch");

const envFile = join(root, "site", ".env.local");
const current = existsSync(envFile) ? readFileSync(envFile, "utf8") : "";
if (!/^PORT=/m.test(current)) {
  const port = await freePort(3100);
  writeFileSync(envFile, `${current}${current && !current.endsWith("\n") ? "\n" : ""}PORT=${port}\n`);
  console.log(`site port: ${port} (site/.env.local)`);
}

console.log("\nReady. Next: bun run doctor, then bun run dev.");
