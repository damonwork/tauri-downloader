// verify-extension.mjs — Verificación de la extensión del navegador.
// 1) Sintaxis de todos los scripts (node --check).
// 2) Carga simulada de los módulos en el orden de Chrome (importScripts)
//    y de Firefox (background.scripts del manifest) con un api stub.
// 3) Fixtures de naming espejo de src-tauri (browser.rs): si un caso
//    esperado cambia aquí o allá, este script falla y se detecta el drift.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import assert from "node:assert";
import vm from "node:vm";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const extension = join(root, "browser-extension");

const scripts = ["background.js", "content.js", "popup.js", "lib/log.js", "lib/url.js", "lib/naming.js", "lib/store.js", "lib/capture.js"];
for (const script of scripts) {
  execFileSync(process.execPath, ["--check", join(extension, script)]);
}

function browserStub(initial = {}) {
  return {
    runtime: { lastError: null, onMessage: { addListener() {} } },
    contextMenus: {
      remove(_id, callback) { if (callback) callback(); },
      create() {},
      onClicked: { addListener() {} },
    },
    storage: {
      local: {
        get() { return Promise.resolve(initial); },
        set() { return Promise.resolve(); },
      },
    },
    action: { setBadgeText() { return Promise.resolve(); } },
    webRequest: {
      onBeforeSendHeaders: { addListener() {} },
      onHeadersReceived: { addListener() {} },
    },
    downloads: { onCreated: { addListener() {} }, cancel() { return Promise.resolve(); } },
    tabs: { get() { return Promise.resolve(null); } },
    cookies: { getAll() { return Promise.resolve([]); } },
  };
}

const order = ["lib/log.js", "lib/url.js", "lib/naming.js", "lib/store.js", "lib/capture.js", "background.js"];
const webGlobals = { URL, URLSearchParams, decodeURIComponent, encodeURIComponent, setTimeout, clearTimeout, AbortController, console, fetch };

for (const browser of ["chrome", "firefox"]) {
  const context = vm.createContext({ globalThis: {}, ...webGlobals });
  context.globalThis.browser = browserStub();
  if (browser === "chrome") {
    const loaded = [];
    context.globalThis.importScripts = (...files) => {
      for (const file of files) {
        vm.runInContext(readFileSync(join(extension, file), "utf8"), context, { filename: file });
        loaded.push(file);
      }
    };
    vm.runInContext(readFileSync(join(extension, "background.js"), "utf8"), context, { filename: "background.js" });
    assert.deepStrictEqual(loaded, ["lib/log.js", "lib/url.js", "lib/naming.js", "lib/store.js", "lib/capture.js"], "chrome: importScripts carga los módulos de lib/");
  } else {
    for (const script of order) {
      vm.runInContext(readFileSync(join(extension, script), "utf8"), context, { filename: script });
    }
  }
  assert.strictEqual(vm.runInContext("typeof logEvent", context), "function", `${browser}: logEvent cargado`);
  assert.strictEqual(vm.runInContext("typeof queueCandidate", context), "function", `${browser}: queueCandidate cargado`);
  assert.strictEqual(vm.runInContext("typeof resolveFileName", context), "function", `${browser}: resolveFileName cargado`);
  assert.strictEqual(vm.runInContext("BRIDGE_URL", context), "http://127.0.0.1:17846", `${browser}: BRIDGE_URL compartido`);
}

const context = vm.createContext(webGlobals);
vm.runInContext(readFileSync(join(extension, "lib/url.js"), "utf8") + readFileSync(join(extension, "lib/naming.js"), "utf8"), context);
const resolve = (candidate) => vm.runInContext(`resolveFileName(${JSON.stringify(candidate)})`, context);
const cases = [
  [{
    url: "https://re.ironhentai.com/hugging.php",
    fileName: "",
    pageUrl: "https://animefenix2.tv/ver/youjo-senki-s2-6",
    pageTitle: "Ver episodio 6 de Youjo Senki II - MonosChinos",
    mediaType: "video",
  }, "Youjo Senki II Episodio 6.mp4"],
  [{
    url: "https://site.example/hugging.php",
    fileName: "hugging.php",
    pageUrl: "https://site.example/ver/one-piece-1050",
    pageTitle: "One Piece Episodio 1050 - Subs",
    mediaType: "video",
  }, "One Piece Episodio 1050.mp4"],
  [{
    url: "https://site.example/hugging.php",
    fileName: "hugging.php",
    pageUrl: "https://site.example/ver/aot-12",
    pageTitle: "Attack on Titan Episodio 12 (1080p)",
    mediaType: "video",
  }, "Attack on Titan Episodio 12.mp4"],
  [{
    url: "https://site.example/hugging.php",
    fileName: "hugging.php",
    pageUrl: "https://site.example/ver/boku-3",
    pageTitle: "Boku no Hero Academia Episodio 3 v2",
    mediaType: "video",
  }, "Boku no Hero Academia Episodio 3.mp4"],
  [{
    url: "https://productionresultssa7.blob.core.windows.net/actions-results/fbf0da1c/artifacts/06490543b20b4ab2ebe8dbe95104ba60fcc1e767e3b66b007de379510ef23632.zip",
    fileName: "06490543b20b4ab2ebe8dbe95104ba60fcc1e767e3b66b007de379510ef23632.zip",
    pageUrl: "https://github.com/damonwork/tauri-downloader/actions/runs/123",
    pageTitle: "Workflow run · damonwork/tauri-downloader",
    mediaType: "",
  }, "06490543b20b4ab2ebe8dbe95104ba60fcc1e767e3b66b007de379510ef23632.zip"],
  [{
    url: "https://cdn-lfs.huggingface.co/repos/5f/8b/64/abc/gemma-4-E2B-it-qat-GGUF-MTP-Q4_K_M.gguf?download=1",
    fileName: "gemma-4-E2B-it-qat-GGUF-MTP-Q4_K_M.gguf",
    pageUrl: "https://huggingface.co/unsloth/gemma-4-E2B-it-qat-GGUF/tree/main/MTP",
    pageTitle: "MTP · gemma-4-E2B-it-qat-GGUF · Hugging Face",
    mediaType: "",
  }, "gemma-4-E2B-it-qat-GGUF-MTP-Q4_K_M.gguf"],
  [{
    url: "https://cdn.example.com/v/12345.mp4",
    fileName: "12345.mp4",
    pageUrl: "https://animefenix2.tv/ver/shingeki-5",
    pageTitle: "Ver episodio 5 de Shingeki no Kyojin - Subs",
    mediaType: "video",
  }, "12345.mp4"],
  [{
    url: "https://re.ironhentai.com/hugging.php",
    fileName: "hugging.php",
    pageUrl: "https://animefenix2.tv/guia/descargas",
    pageTitle: "Guía de descargas y tutoriales",
    mediaType: "",
  }, "hugging.php"],
  [{
    url: "https://example.com/files/Informe Final 2026.pdf",
    fileName: "",
    pageUrl: "https://example.com/files",
    pageTitle: "Informes y documentos",
    mediaType: "",
  }, "Informe Final 2026.pdf"],
];

for (const [candidate, expected] of cases) {
  const resolved = resolve(candidate);
  assert.strictEqual(typeof resolved, "string", "resolveFileName debe devolver string siempre");
  assert.strictEqual(resolved, expected);
}

const captureContext = vm.createContext({ globalThis: {}, ...webGlobals });
captureContext.globalThis.browser = browserStub();
for (const script of ["lib/log.js", "lib/url.js", "lib/naming.js", "lib/store.js", "lib/capture.js"]) {
  vm.runInContext(readFileSync(join(extension, script), "utf8"), captureContext, { filename: script });
}
const pushArtifact = (page, name, href) => vm.runInContext(`pushPendingArtifact(${JSON.stringify(page)}, ${JSON.stringify(name)}, ${JSON.stringify(href || "")})`, captureContext);
const popArtifact = (page, url, href) => vm.runInContext(`popPendingArtifact(${JSON.stringify(page)}, ${JSON.stringify(url)}, ${JSON.stringify(href || "")})`, captureContext);
const runPage = "https://github.com/damonwork/tauri-downloader/actions/runs/31662997823";
const blobUrl = "https://productionresultssa14.blob.core.windows.net/actions-results/x/artifacts/ca2167386ed0db94c66eb93e02e8ba549861dc73094911a0a20f56e3e8f15549.zip";
const artifactHref = "https://github.com/damonwork/tauri-downloader/actions/runs/31662997823/artifacts/9166984953";
const artifactCases = [
  ["clic anuncia el nombre y la descarga lo consume por href exacto", () => {
    pushArtifact(runPage, "fluxor-browser-extensions", artifactHref);
    return popArtifact(runPage, blobUrl, artifactHref) === "fluxor-browser-extensions";
  }],
  ["href y recurso no-artifact no consumen el pendiente", () => {
    pushArtifact(runPage, "fluxor-linux-x64", artifactHref);
    return popArtifact("https://github.com/otro/repo/actions/runs/1", "https://example.com/files/reporte.zip", "https://example.com/files/reporte.zip") === "";
  }],
  ["dos clics seguidos en la misma página: se consume el más antiguo (orden de descargas)", () => {
    pushArtifact(runPage, "fluxor-linux-x64", `${artifactHref}1`);
    pushArtifact(runPage, "fluxor-macos-x64", `${artifactHref}2`);
    return popArtifact(runPage, blobUrl, "") === "fluxor-linux-x64";
  }],
  ["sin página emparejada, el blob de actions-results consume el pendiente de github más antiguo", () => {
    vm.runInContext("pendingArtifacts.length = 0", captureContext);
    pushArtifact("https://github.com/damonwork/tauri-downloader/actions/runs/2", "fluxor-windows-x64", "");
    return popArtifact("", blobUrl, "") === "fluxor-windows-x64";
  }],
  ["pendientes expirados no se consumen", () => {
    vm.runInContext("pendingArtifacts.length = 0", captureContext);
    pushArtifact(runPage, "fluxor-macos-x64", artifactHref);
    vm.runInContext("pendingArtifacts[pendingArtifacts.length - 1].at = 0", captureContext);
    return popArtifact(runPage, blobUrl, artifactHref) === "";
  }],
  ["el nombre del artifact se conserva en la resolución con .zip", () => {
    return resolve({
      url: blobUrl,
      fileName: "fluxor-linux-x64.zip",
      pageUrl: runPage,
      pageTitle: "Actions · damonwork/tauri-downloader",
      mediaType: "",
    }) === "fluxor-linux-x64.zip";
  }],
];
for (const [name, check] of artifactCases) {
  assert.strictEqual(check(), true, name);
}

const dedupContext = vm.createContext({ globalThis: {}, ...webGlobals });
dedupContext.api = browserStub({ token: "t" });
dedupContext.fetch = async () => ({ ok: true, json: async () => ({ ok: true, data: {} }) });
for (const script of ["lib/log.js", "lib/url.js", "lib/naming.js", "lib/store.js", "lib/capture.js"]) {
  vm.runInContext(readFileSync(join(extension, script), "utf8"), dedupContext, { filename: script });
}
const enqueue = () => vm.runInContext('queueCandidate({ url: "https://example.com/video.mp4", fileName: "video.mp4" })', dedupContext);
const firstSend = await enqueue();
const secondSend = await enqueue();
assert.strictEqual(firstSend.ok, true, "dedup: el primer envío es aceptado");
assert.strictEqual(secondSend.duplicate, true, "dedup: el segundo envío del mismo recurso se descarta");

console.log(`verify-extension: OK (${scripts.length} scripts, ${cases.length} fixtures de naming, ${artifactCases.length} fixtures de artifacts, dedup)`);
