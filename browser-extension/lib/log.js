// lib/log.js — Registro de diagnóstico de la extensión.
// La cola (logQueue) serializa las escrituras a storage.local y limita
// el historial a MAX_LOGS entradas. Todo borrado de "logs" (clearLogs)
// debe encadenarse a logQueue para no saltarse la serialización.

const MAX_LOGS = 80;
let logQueue = Promise.resolve();

function logEvent(level, event, message, details = {}) {
  const entry = {
    at: new Date().toISOString(),
    level,
    event,
    message,
    details,
  };
  const method = level === "error" ? "error" : level === "warning" ? "warn" : "info";
  console[method](`[Fluxor] ${event}: ${message}`, details);
  logQueue = logQueue.then(async () => {
    const saved = await extensionApiCall(api.storage.local.get.bind(api.storage.local), ["logs"]);
    const logs = [entry, ...(saved.logs || [])].slice(0, MAX_LOGS);
    await extensionApiCall(api.storage.local.set.bind(api.storage.local), { logs });
  }).catch((error) => console.error("[Fluxor] No se pudo guardar el log", error));
  return logQueue;
}
