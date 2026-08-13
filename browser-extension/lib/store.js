// lib/store.js — Persistencia de la extensión (storage.local).
// Define las claves por defecto y las operaciones compartidas de lectura/
// escritura (settings/rememberCandidate/setBadge). lib/log.js escribe la
// clave "logs" directamente, pero siempre a través de su cola serializada.

const DEFAULTS = {
  token: "",
  autoCapture: true,
  overlay: true,
  candidates: [],
  logs: [],
};

const MAX_CANDIDATES = 20;
let candidateQueue = Promise.resolve();

function extensionApiCall(method, ...args) {
  const result = method(...args);
  return result && typeof result.then === "function"
    ? result
    : new Promise((resolve, reject) => {
        const error = api.runtime.lastError;
        if (error) reject(new Error(error.message));
        else resolve(result);
      });
}

async function settings() {
  const saved = await extensionApiCall(api.storage.local.get.bind(api.storage.local), Object.keys(DEFAULTS));
  return { ...DEFAULTS, ...saved };
}

function rememberCandidate(candidate) {
  const operation = candidateQueue.then(async () => {
    const current = await settings();
    const candidates = [candidate, ...(current.candidates || [])]
      .filter((entry, index, all) => index === all.findIndex((other) => other.url === entry.url))
      .slice(0, MAX_CANDIDATES);
    await extensionApiCall(api.storage.local.set.bind(api.storage.local), { candidates });
  });
  candidateQueue = operation.catch(() => {});
  return operation;
}

async function setBadge(text) {
  if (!api.action?.setBadgeText) return;
  await extensionApiCall(api.action.setBadgeText.bind(api.action), { text });
}
