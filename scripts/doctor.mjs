// Is this checkout healthy? Run before trusting a failure. Prints one line per
// check, each failure with the command that fixes it, and exits 1 if a required
// check fails. Read-only. Never prints token values.
import { execSync } from "node:child_process";
import { cargoEnv } from "./cargo.mjs";
import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { basename, join, resolve } from "node:path";

const root = resolve(import.meta.dir, "..");
let failed = false;

const sh = (cmd) => execSync(cmd, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], env: cargoEnv }).trim();
const read = (file) => (existsSync(file) ? readFileSync(file, "utf8") : "");
const check = (name, fn, { required = true } = {}) => {
  try {
    const detail = fn();
    console.log(`ok    ${name}${detail ? ` · ${detail}` : ""}`);
  } catch (error) {
    if (required) failed = true;
    console.log(`${required ? "FAIL" : "warn"}  ${name} · ${String(error.message ?? error).split("\n")[0]}`);
  }
};
const others = sh("git worktree list --porcelain")
  .split("\n")
  .filter((line) => line.startsWith("worktree "))
  .map((line) => resolve(line.slice("worktree ".length)))
  .filter((path) => path.toLowerCase() !== root.toLowerCase());

check("rust toolchain", () => sh("rustc --version"));
check("MSVC linker (Windows)", () => {
  if (process.platform !== "win32") return "not needed";
  const vswhere = "C:/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe";
  if (!existsSync(vswhere)) throw new Error("Install VS Build Tools with the C++ workload");
  const path = sh(`"${vswhere}" -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`);
  if (!path) throw new Error("VS Build Tools found but the C++ workload is missing");
  return path;
});
check("bun", () => sh("bun --version"));
check("site deps", () => {
  if (!existsSync(join(root, "site", "node_modules", ".setup-stamp"))) throw new Error("fix: bun run setup");
  return "installed";
});
check("Cargo.lock committed", () => {
  if (!existsSync(join(root, "Cargo.lock"))) throw new Error("run cargo build once and commit Cargo.lock");
  return "present";
});
check(
  "debug build",
  () => {
    const exe = join(root, "target", "debug", process.platform === "win32" ? "likh.exe" : "likh");
    if (!existsSync(exe)) throw new Error("not built yet. fix: cargo build -p likh (about 3 minutes the first time)");
    return "target/debug/likh";
  },
  { required: false },
);
check("this checkout's LIKH_HOME", () => {
  const home = read(join(root, ".env.local")).match(/^LIKH_HOME=(.+)$/m)?.[1]?.trim();
  if (!home) throw new Error("not set, so bun run dev would use the installed likh's settings. fix: bun run setup");
  if (!resolve(home).toLowerCase().startsWith(root.toLowerCase())) throw new Error(`${home} is outside this checkout. fix: delete the LIKH_HOME line in .env.local, then bun run setup`);
  const twin = others.find((path) => read(join(path, ".env.local")).includes(`/${basename(home)}\n`));
  if (twin) throw new Error(`${twin} uses the same folder name, so the same likh ctl pipe. fix: bun run setup in both`);
  return `${home.slice(root.length + 1)} (pipe likh-${process.env.USERNAME ?? process.env.USER ?? ""}-${basename(home)})`;
});
check("site port", () => {
  const portIn = (path) => read(join(path, ".env.local")).match(/^SITE_PORT=(\d+)/m)?.[1];
  const port = portIn(root);
  if (!port) throw new Error("fix: bun run setup");
  const twin = others.find((path) => portIn(path) === port);
  if (twin) throw new Error(`${port} is also ${twin}'s port. fix: bun run setup`);
  return `${port} (SITE_PORT in .env.local)`;
});
check(
  "installed likh (leave it alone)",
  () => {
    if (process.platform !== "win32") return "not checked on this platform";
    const pids = sh(
      `powershell -NoProfile -Command "(Get-Process likh -ErrorAction SilentlyContinue | Where-Object { $_.Path -like '*\\Programs\\likh\\*' }).Id"`,
    );
    return pids ? `running, PID ${pids.split(/\s+/).join(", ")}: it owns Ctrl+Space, drive yours with likh ctl` : "not running: your likh may take Ctrl+Space";
  },
  { required: false },
);
check(
  "Codex sign-in (needed to dictate)",
  () => {
    const home = process.env.CODEX_HOME || join(homedir(), ".codex");
    if (!existsSync(home)) throw new Error(`no ${home}: install Codex`);
    const auth = join(home, "auth.json");
    if (!existsSync(auth)) throw new Error("Codex is installed but not signed in");
    const tokens = JSON.parse(readFileSync(auth, "utf8")).tokens;
    if (!tokens?.access_token) throw new Error("no ChatGPT sign-in in auth.json");
    const exp = JSON.parse(Buffer.from(tokens.access_token.split(".")[1], "base64url")).exp;
    if (exp && exp * 1000 < Date.now()) throw new Error("sign-in expired: open Codex once");
    return exp ? `valid until ${new Date(exp * 1000).toISOString()}` : "signed in";
  },
  { required: false },
);
check(
  "behind origin/main",
  () => {
    sh("git fetch --quiet origin main");
    return `${sh("git rev-list --count HEAD..origin/main")} commits`;
  },
  { required: false },
);

process.exit(failed ? 1 : 0);
