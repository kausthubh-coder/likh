// Signs a release file for likh's updater: writes <file>.sig, the base64 Ed25519
// signature that app/src/update checks before it runs a downloaded installer.
//
// The private key (PKCS#8 PEM) comes from LIKH_UPDATE_SIGNING_KEY, the repository
// secret, or from a file named by LIKH_UPDATE_SIGNING_KEY_FILE when signing by
// hand. It refuses to sign with a key whose public half isn't the one built into
// likh, because every copy of likh would then throw the update away.
//
// Usage: bun scripts/sign-release.mjs target/installer/likh-setup-windows-x64.exe
import { createPrivateKey, createPublicKey, sign, verify } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const file = process.argv[2];
if (!file) throw new Error("Pass the file to sign.");
const pem =
  process.env.LIKH_UPDATE_SIGNING_KEY ||
  (process.env.LIKH_UPDATE_SIGNING_KEY_FILE && readFileSync(process.env.LIKH_UPDATE_SIGNING_KEY_FILE, "utf8"));
if (!pem) throw new Error("Set LIKH_UPDATE_SIGNING_KEY (or LIKH_UPDATE_SIGNING_KEY_FILE).");

const privateKey = createPrivateKey(pem);
const publicKey = createPublicKey(privateKey);
const ours = Buffer.from(publicKey.export({ format: "jwk" }).x, "base64url").toString("base64");
const source = readFileSync(join(import.meta.dir, "..", "app", "src", "update", "mod.rs"), "utf8");
const builtIn = source.match(/const PUBLIC_KEY: &str = "([^"]+)";/)?.[1];
if (ours !== builtIn) {
  throw new Error(`This key's public half (${ours}) isn't the one in app/src/update/mod.rs (${builtIn}).`);
}

const bytes = readFileSync(file);
const signature = sign(null, bytes, privateKey);
if (!verify(null, bytes, publicKey, signature)) throw new Error("The signature didn't verify.");
writeFileSync(`${file}.sig`, `${signature.toString("base64")}\n`);
console.log(`signed ${file} (${bytes.length} bytes) -> ${file}.sig`);
