// Makes a fresh checkout or worktree work on its own: site deps, a debug build,
// a site port no other worktree claims, and a LIKH_HOME so the likh it runs has
// its own settings, history, update check and `likh ctl` pipe, apart from the
// installed likh and from every other worktree. T3 Code runs this on worktree
// create. Safe to rerun; when nothing is missing it takes seconds.
//
// Nothing is copied from the main checkout: the app reads no env files, and the
// site's only one holds its port. Never copy a Vercel token between checkouts.
import { execSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, statSync, symlinkSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { basename, join, resolve } from "node:path";
import { cargoEnv } from "./cargo.mjs";

const root = resolve(import.meta.dir, "..");
const run = (cmd, cwd = root) => {
  console.log(`> ${cmd}`);
  // cargoEnv finds cargo even in a shell opened before rustup was installed.
  execSync(cmd, { cwd, stdio: "inherit", env: cargoEnv });
};
const read = (file) => (existsSync(file) ? readFileSync(file, "utf8") : "");
/** Sets KEY=value in an env file, keeping its other lines. */
const setEnv = (file, key, value) => {
  const lines = read(file).split(/\r?\n/).filter((line) => line && !line.startsWith(`${key}=`));
  writeFileSync(file, `${[...lines, `${key}=${value}`].join("\n")}\n`);
};
// Every worktree of this repo, the main checkout included.
const checkouts = execSync("git worktree list --porcelain", { cwd: root, encoding: "utf8" })
  .split("\n")
  .filter((line) => line.startsWith("worktree "))
  .map((line) => resolve(line.slice("worktree ".length)));
const others = checkouts.filter((path) => path.toLowerCase() !== root.toLowerCase());
const id = createHash("sha1").update(root.toLowerCase()).digest("hex").slice(0, 8);

// Claude Code reads .claude/skills, Codex reads .agents/skills: one folder, linked.
// A junction on Windows needs no admin rights.
const skillsLink = join(root, ".claude", "skills");
if (!existsSync(skillsLink)) {
  mkdirSync(join(root, ".claude"), { recursive: true });
  symlinkSync(join(root, ".agents", "skills"), skillsLink, "junction");
  console.log("linked .claude/skills -> .agents/skills");
}

// Site deps, only when missing or older than the lockfile.
const site = join(root, "site");
const stamp = join(site, "node_modules", ".setup-stamp");
if (!existsSync(stamp) || statSync(join(site, "bun.lock")).mtimeMs > statSync(stamp).mtimeMs) {
  run("bun install", site);
  writeFileSync(stamp, "bun install finished\n");
}

// Rust intermediates go to one build dir shared by every checkout, so a new
// worktree only compiles likh itself. Each checkout still gets its own
// target/debug/likh.exe: a running likh in one worktree never blocks or swaps
// the binary of another. Untracked, so CI and other machines are unaffected.
const cargoConfig = join(root, ".cargo", "config.toml");
const marker = "# Written by scripts/setup.mjs";
if (!existsSync(cargoConfig)) {
  mkdirSync(join(root, ".cargo"), { recursive: true });
  writeFileSync(cargoConfig, `${marker}: Rust intermediates shared by every likh checkout.\n[build]\nbuild-dir = "{cargo-cache-home}/likh-build"\n`);
} else if (!read(cargoConfig).startsWith(marker)) {
  console.log("warn: .cargo/config.toml isn't setup's; leaving it alone (no shared build dir)");
}
run("cargo build -p likh");

// This checkout's likh home. `likh ctl` names its pipe after the folder, so the
// folder name carries a hash of the checkout path. Bun loads .env.local for
// every `bun run`, so `bun run dev` and `bun run likh` use it.
const home = join(root, ".agents", "qa", `home-${id}`).replaceAll("\\", "/");
mkdirSync(home, { recursive: true });
// No silent update downloads from an agent's build. `likh ctl update` still checks.
if (!existsSync(join(home, "settings.json"))) writeFileSync(join(home, "settings.json"), '{ "auto_update": false }\n');
setEnv(join(root, ".env.local"), "LIKH_HOME", home);

// The site's port: kept if no other worktree claims it, otherwise derived from
// the checkout path and moved up past ports that are claimed or in use.
const portOf = (file) => Number(read(file).match(/^PORT=(\d+)/m)?.[1]) || null;
const claimed = new Set(others.map((path) => portOf(join(path, "site", ".env.local"))).filter(Boolean));
const free = (port) =>
  new Promise((done) => {
    const server = createServer();
    server.once("error", () => done(false));
    server.listen(port, "127.0.0.1", () => server.close(() => done(true)));
  });
const siteEnv = join(site, ".env.local");
let port = portOf(siteEnv);
if (!port || claimed.has(port)) {
  port = 3100 + (Number.parseInt(id, 16) % 900);
  while (claimed.has(port) || !(await free(port))) port += 1;
  setEnv(siteEnv, "PORT", port);
}

console.log("");
const doctor = spawnSync("bun", ["scripts/doctor.mjs"], { cwd: root, stdio: "inherit", env: cargoEnv });
const user = process.env.USERNAME ?? process.env.USER ?? "";
console.log(
  `\n${doctor.status === 0 ? "ready" : "NOT READY"}: ${basename(root)} · site port ${port} · LIKH_HOME .agents/qa/home-${id} (pipe likh-${user}-home-${id})`,
);
process.exit(doctor.status ?? 1);
