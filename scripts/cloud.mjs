// Makes a cloud session (Claude Code on the web, Codex Cloud) ready to work on
// likh: my global skills and rules, the Linux libraries the Rust crate links
// against, then the usual setup. Claude Code runs it as a SessionStart hook
// (.claude/settings.json); Codex Cloud as its setup script: CLOUD=1 node scripts/cloud.mjs
//
// Does nothing outside a cloud VM, so local sessions on Windows never notice it.
// Plain node and only built-ins: it runs before anything is installed.
// stdout becomes the session's context, so only the rules and the last line go there.
import { spawnSync } from "node:child_process";
import { join } from "node:path";

const cloud = process.env.CLAUDE_CODE_REMOTE === "true" || process.env.CODEX_CI || process.env.CLOUD === "1";
if (!cloud) process.exit(0);

const root = join(import.meta.dirname, "..");
const toStderr = { cwd: root, stdio: ["ignore", 2, 2] };
const failures = [];

// 1. Global skills and rules. Its stdout is the rules, for this session's context.
const session = spawnSync(
  "bash",
  ["-c", "curl -fsSL https://raw.githubusercontent.com/kausthubh-coder/skills/main/cloud/session.sh | bash"],
  { cwd: root, stdio: ["ignore", "inherit", 2] },
);
if (session.status !== 0) failures.push("global skills (cloud/session.sh)");

// 2. Linux libraries: tray-icon links GTK and appindicator, cpal links ALSA,
// enigo links libxdo. Without them the crate doesn't compile here.
const libs = ["libgtk-3-dev", "libasound2-dev", "libxdo-dev", "libayatana-appindicator3-dev", "pkg-config"];
const missing = libs.filter((lib) => spawnSync("dpkg", ["-s", lib], { stdio: "ignore" }).status !== 0);
if (missing.length > 0) {
  const isRoot = process.getuid?.() === 0;
  const apt = (args) =>
    spawnSync(isRoot ? "apt-get" : "sudo", isRoot ? args : ["-n", "apt-get", ...args], {
      ...toStderr,
      env: { ...process.env, DEBIAN_FRONTEND: "noninteractive" },
    }).status;
  if (apt(["update", "-qq"]) !== 0 || apt(["install", "-y", "-qq", ...missing]) !== 0) failures.push(`apt install ${missing.join(" ")}`);
}

// 3. The repo's own setup: site deps, cargo fetch, this checkout's LIKH_HOME and port, the doctor.
const setup = spawnSync("bun", ["scripts/setup.mjs"], toStderr);
if (setup.error) failures.push("bun is not installed (https://bun.sh)");
else if (setup.status !== 0) failures.push("bun run setup (rerun it to see why)");

console.log(
  failures.length === 0
    ? "cloud: likh ready. Read `## Cloud sessions` in AGENTS.md for what works here."
    : `cloud: NOT READY, failed: ${failures.join("; ")}`,
);
