import { readFile } from "node:fs/promises";

const [packageJson, tauriConfig, cargoManifest] = await Promise.all([
  readFile(new URL("../package.json", import.meta.url), "utf8").then(JSON.parse),
  readFile(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8").then(JSON.parse),
  readFile(new URL("../src-tauri/Cargo.toml", import.meta.url), "utf8"),
]);

const cargoVersion = cargoManifest.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const versions = new Map([
  ["package.json", packageJson.version],
  ["tauri.conf.json", tauriConfig.version],
  ["Cargo.toml", cargoVersion],
]);
const uniqueVersions = new Set(versions.values());

if (uniqueVersions.size !== 1 || uniqueVersions.has(undefined)) {
  throw new Error(`Versiones inconsistentes: ${JSON.stringify(Object.fromEntries(versions))}`);
}

const version = versions.get("package.json");
const tag = process.argv[2];
if (tag && tag !== `v${version}`) {
  throw new Error(`El tag ${tag} no coincide con la versión v${version}`);
}

console.log(`Versiones consistentes: ${version}`);
