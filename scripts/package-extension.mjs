import { cp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { deflateRaw } from "node:zlib";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { promisify } from "node:util";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const source = join(root, "browser-extension");
const output = join(root, "dist-extension");
const version = JSON.parse(await readFile(join(root, "package.json"), "utf8")).version;
const requested = process.argv[2] || "all";
const browsers = requested === "all" ? ["chrome", "firefox"] : [requested];
const compress = promisify(deflateRaw);

if (!browsers.every((browser) => ["chrome", "firefox"].includes(browser))) {
  throw new Error("Uso: node scripts/package-extension.mjs [chrome|firefox|all]");
}

await rm(output, { recursive: true, force: true });
await mkdir(output, { recursive: true });

for (const browser of browsers) {
  const packageDirectory = join(output, `fluxor-extension-${browser}`);
  await mkdir(packageDirectory, { recursive: true });
  await cp(source, packageDirectory, { recursive: true });
  const manifestPath = join(packageDirectory, `manifest.${browser}.json`);
  const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  manifest.version = version;
  await writeFile(join(packageDirectory, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  await rm(manifestPath);
  await rm(join(packageDirectory, browser === "chrome" ? "manifest.firefox.json" : "manifest.chrome.json"));
  const archive = join(output, `fluxor-extension-${browser}-v${version}.zip`);
  await writeZip(packageDirectory, archive);
  await rm(packageDirectory, { recursive: true, force: true });
}

async function writeZip(directory, archive) {
  const files = await listFiles(directory);
  const entries = [];
  const central = [];
  let offset = 0;
  for (const file of files) {
    const name = file.slice(directory.length + 1).replaceAll("\\", "/");
    const input = await readFile(join(directory, file.slice(directory.length + 1)));
    const compressed = await compress(input);
    const checksum = crc32(input);
    const nameBytes = Buffer.from(name);
    const local = Buffer.alloc(30 + nameBytes.length);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(8, 8);
    local.writeUInt32LE(checksum, 14);
    local.writeUInt32LE(compressed.length, 18);
    local.writeUInt32LE(input.length, 22);
    local.writeUInt16LE(nameBytes.length, 26);
    nameBytes.copy(local, 30);
    entries.push(Buffer.concat([local, compressed]));

    const directoryEntry = Buffer.alloc(46 + nameBytes.length);
    directoryEntry.writeUInt32LE(0x02014b50, 0);
    directoryEntry.writeUInt16LE(20, 4);
    directoryEntry.writeUInt16LE(20, 6);
    directoryEntry.writeUInt16LE(8, 10);
    directoryEntry.writeUInt32LE(checksum, 16);
    directoryEntry.writeUInt32LE(compressed.length, 20);
    directoryEntry.writeUInt32LE(input.length, 24);
    directoryEntry.writeUInt16LE(nameBytes.length, 28);
    directoryEntry.writeUInt32LE(offset, 42);
    nameBytes.copy(directoryEntry, 46);
    central.push(directoryEntry);
    offset += local.length + compressed.length;
  }
  const centralBytes = Buffer.concat(central);
  const footer = Buffer.alloc(22);
  footer.writeUInt32LE(0x06054b50, 0);
  footer.writeUInt16LE(0, 8);
  footer.writeUInt16LE(files.length, 10);
  footer.writeUInt32LE(centralBytes.length, 12);
  footer.writeUInt32LE(offset, 16);
  await writeFile(archive, Buffer.concat([...entries, centralBytes, footer]));
}

async function listFiles(directory, current = directory) {
  const entries = await readdir(current, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const path = join(current, entry.name);
    if (entry.isDirectory()) files.push(...await listFiles(directory, path));
    else files.push(path);
  }
  return files;
}

function crc32(buffer) {
  let crc = 0xffffffff;
  for (const byte of buffer) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
  }
  return (crc ^ 0xffffffff) >>> 0;
}
