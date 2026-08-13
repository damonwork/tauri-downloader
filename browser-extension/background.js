// background.js — Punto de entrada de la extensión.
// Solo conecta los listeners con los módulos de lib/:
//   lib/url.js     -> utilidades de URL
//   lib/naming.js  -> resolución de nombres de archivo
//   lib/log.js     -> registro de diagnóstico
//   lib/store.js   -> persistencia (storage.local)
//   lib/capture.js -> contexto de petición, detección y envío al puente
// Firefox carga estos módulos vía manifest (background.scripts);
// Chrome los carga aquí con importScripts. La guarda evita doble carga.
const api = globalThis.browser ?? globalThis.chrome;

if (typeof logEvent === "undefined") {
  globalThis.importScripts(
    "lib/log.js",
    "lib/url.js",
    "lib/naming.js",
    "lib/store.js",
    "lib/capture.js",
  );
}

try {
  api.webRequest.onBeforeSendHeaders.addListener(
    rememberRequest,
    { urls: ["<all_urls>"] },
    ["requestHeaders", "extraHeaders"],
  );
} catch (error) {
  api.webRequest.onBeforeSendHeaders.addListener(
    rememberRequest,
    { urls: ["<all_urls>"] },
    ["requestHeaders"],
  );
  void logEvent("warning", "headers", "El navegador no admite extraHeaders; algunos sitios pueden requerir cookies manuales.", {
    error: error.message,
  });
}

api.webRequest.onHeadersReceived.addListener(
  async (details) => {
    if (!mediaResponse(details)) return;
    const context = requestContext(details);
    const tab = details.tabId >= 0 && api.tabs?.get
      ? await extensionApiCall(api.tabs.get.bind(api.tabs), details.tabId).catch(() => null)
      : null;
    const location = headerValue(details.responseHeaders, "location");
    const contentType = headerValue(details.responseHeaders, "content-type").split(";", 1)[0].toLowerCase();
    const candidate = {
      url: details.url,
      fileName: fileNameFromDisposition(headerValue(details.responseHeaders, "content-disposition"))
        || fileNameFromUrl(location || details.url),
      mediaType: contentType.startsWith("audio/") ? "audio" : contentType.startsWith("video/") ? "video" : "",
      pageTitle: tab?.title || context.pageTitle,
      pageUrl: tab?.url || context.pageUrl,
      referrer: context.referrer,
      userAgent: context.userAgent,
      cookies: context.cookies,
      tabId: details.tabId,
      forceSingleStream: true,
      detectedAt: new Date().toISOString(),
    };
    candidate.fileName = resolveFileName(candidate);
    const mediaKey = `${details.tabId || 0}:${candidate.url}`;
    const previous = lastMediaLog.get(mediaKey);
    if (previous && previous.fileName === candidate.fileName) return;
    lastMediaLog.set(mediaKey, { fileName: candidate.fileName });
    if (lastMediaLog.size > 200) lastMediaLog.delete(lastMediaLog.keys().next().value);
    await rememberCandidate(candidate);
    await logEvent("info", "media", "Recurso multimedia detectado.", {
      fileName: candidate.fileName,
      url: diagnosticUrl(candidate.url),
      statusCode: details.statusCode,
      hasReferrer: Boolean(candidate.referrer),
      cookieCount: candidate.cookies?.length || 0,
    });
  },
  { urls: ["<all_urls>"] },
  ["responseHeaders"],
);

api.downloads.onCreated.addListener(async (download) => {
  const current = await settings();
  if (!current.autoCapture || !validUrl(download.url)) return;
  const url = download.finalUrl || download.url;
  const context = recentByUrl.get(url) || recentByUrl.get(download.url) || {};
  let tab = null;
  if (download.tabId >= 0 && api.tabs?.get) {
    tab = await extensionApiCall(api.tabs.get.bind(api.tabs), download.tabId).catch(() => null);
  }
  const result = await queueCandidate({
    url,
    fileName: optionalFileName(download.filename?.split(/[\\/]/).pop()) || fileNameFromUrl(url),
    pageTitle: tab?.title || context.pageTitle,
    pageUrl: tab?.url || context.pageUrl || download.referrer,
    referrer: download.referrer || context.referrer,
    userAgent: context.userAgent,
    cookies: context.cookies,
    mediaType: context.mediaType || mediaTypeFromUrl(url),
    tabId: download.tabId,
    forceSingleStream: signedUrl(url),
  });
  if (result.ok) await extensionApiCall(api.downloads.cancel.bind(api.downloads), download.id);
});

api.runtime.onMessage.addListener((message, sender, sendResponse) => {
  (async () => {
    if (message.type === "getState") return settings();
    if (message.type === "saveState") {
      const next = { ...(await settings()), ...(message.state || {}) };
      await extensionApiCall(api.storage.local.set.bind(api.storage.local), next);
      return next;
    }
    if (message.type === "captureCandidate") {
      const candidate = message.candidate || {};
      const context = recentByUrl.get(candidate.url) || {};
      return queueCandidate({
        ...candidate,
        referrer: candidate.referrer || context.referrer || "",
        userAgent: context.userAgent || "",
        cookies: candidate.cookies?.length ? candidate.cookies : context.cookies,
        pageTitle: sender.tab?.title || candidate.pageTitle || context.pageTitle || "",
        pageUrl: sender.tab?.url || candidate.pageUrl || context.pageUrl || "",
      });
    }
    if (message.type === "clearLogs") {
      logQueue = logQueue.then(async () => {
        await extensionApiCall(api.storage.local.set.bind(api.storage.local), { logs: [] });
      }).catch(() => {});
      await logQueue;
      return { ok: true };
    }
    if (message.type === "clearCandidates") {
      queuedKeys.clear();
      lastMediaLog.clear();
      await extensionApiCall(api.storage.local.set.bind(api.storage.local), { candidates: [] });
      await logEvent("info", "captures", "Se limpió el historial de capturas.");
      return { ok: true };
    }
    if (message.type === "testConnection") {
      const current = await settings();
      if (!current.token) return { ok: false, error: "Falta el token." };
      try {
        const response = await fetch(`${BRIDGE_URL}/v1/status`, { headers: { "X-Fluxor-Token": current.token } });
        const result = response.ok ? { ok: true } : { ok: false, error: `Fluxor respondió HTTP ${response.status}.` };
        await logEvent(result.ok ? "info" : "error", "connection", result.ok ? "Conexión con Fluxor correcta." : result.error);
        return result;
      } catch (error) {
        await logEvent("error", "connection", "Fluxor no está accesible en el bridge local.", { error: error.message });
        return { ok: false, error: error.message || "No se pudo contactar con Fluxor." };
      }
    }
    return { ok: false, error: "Mensaje no reconocido." };
  })().then(sendResponse).catch((error) => sendResponse({ ok: false, error: error.message }));
  return true;
});
