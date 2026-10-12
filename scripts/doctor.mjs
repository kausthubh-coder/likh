// Is this checkout healthy? Run before trusting a failure. Prints one line per
// check and exits 1 if any required check fails. Never prints token values.
import { execSync } from "node:child_process";
import { cargoEnv } from "./cargo.mjs";
import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
let failed = false;

const sh = (cmd) => execSync(cmd, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], env: cargoEnv }).trim();
const check = (name, fn, { required = true } = {}) => {
  try {
    const detail = fn();
    console.log(`ok    ${name}${detail ? ` · ${detail}` : ""}`);
  } catch (error) {
    if (required) failed = true;
    console.log(`${required ? "FAIL" : "warn"}  ${name} · ${String(error.message ?? error).split("\n")[0]}`);
  }
};

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
  if (!existsSync(join(root, "site", "node_modules"))) throw new Error("run bun run setup");
  return "installed";
});
check("Cargo.lock committed", () => {
  if (!existsSync(join(root, "Cargo.lock"))) throw new Error("run cargo build once and commit Cargo.lock");
  return "present";
});
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
