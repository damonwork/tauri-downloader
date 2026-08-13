const api = globalThis.browser ?? globalThis.chrome;
const BRIDGE_URL = "http://127.0.0.1:17846";
const MAX_CANDIDATES = 20;
const MAX_LOGS = 80;
const requestById = new Map();
const recentByUrl = new Map();
const queuedKeys = new Map();
let logQueue = Promise.resolve();

const DEFAULTS = {
  token: "",
  autoCapture: true,
  overlay: true,
  candidates: [],
  logs: [],
};

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

function diagnosticUrl(value) {
  try {
    const url = new URL(value);
    return `${url.origin}${url.pathname}`;
  } catch {
    return String(value || "");
  }
}

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

function validUrl(value) {
  try {
    const url = new URL(value);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

function signedUrl(value) {
  try {
    const url = new URL(value);
    return ["expires", "signature", "policy", "x-amz-signature", "xet-cas-uid"].some((name) => url.searchParams.has(name));
  } catch {
    return false;
  }
}

function safeFileName(value) {
  const fallback = "descarga";
  if (!value) return fallback;
  const name = String(value).split(/[\\/]/).pop().replace(/[<>:"/\\|?*\u0000-\u001f]/g, "_").trim();
  return (name.replace(/[. ]+$/g, "") || fallback).slice(0, 180);
}

function optionalFileName(value) {
  return value ? safeFileName(value) : "";
}

function decodeUrlComponent(value) {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function fileNameFromDisposition(value) {
  if (!value) return "";
  const extended = value.match(/filename\*\s*=\s*(?:UTF-8'')?([^;]+)/i)?.[1];
  if (extended) return safeFileName(decodeUrlComponent(extended.trim().replace(/^"|"$/g, "")));
  return optionalFileName(value.match(/filename\s*=\s*"([^"]+)"/i)?.[1]
    || value.match(/filename\s*=\s*([^;]+)/i)?.[1]?.trim());
}

function fileNameFromUrl(value) {
  try {
    const url = new URL(value);
    const disposition = url.searchParams.get("response-content-disposition") || "";
    const dispositionName = fileNameFromDisposition(disposition);
    const queryName = url.searchParams.get("filename") || url.searchParams.get("file_name");
    const pathName = decodeUrlComponent(url.pathname.split("/").filter(Boolean).pop() || "");
    return safeFileName(dispositionName || queryName || pathName || `descarga-${url.hostname}`);
  } catch {
    return "descarga";
  }
}

function fileExtension(value) {
  const extension = safeFileName(value).match(/\.([a-z0-9]{2,5})$/i)?.[1]?.toLowerCase();
  return extension || "";
}

function isTechnicalFileName(value) {
  const name = safeFileName(value);
  const extension = fileExtension(name);
  const stem = name.replace(/\.[^.]+$/, "").toLowerCase();
  return ["php", "asp", "aspx", "cgi", "html", "htm"].includes(extension)
    || ["download", "file", "get", "index", "redirect", "face", "vt", "player", "embed"].includes(stem);
}

function titleFileName(pageTitle, mediaType, fallbackName) {
  const title = String(pageTitle || "").replace(/\s+/g, " ").trim();
  let name = "";
  const episodePage = title.match(/^ver\s+episodio\s+(.+?)\s+de\s+(.+?)(?:\s+-\s+.+)?$/i);
  const episodeTitle = title.match(/^(.+?)\s+episodio\s+(\d+)(?:\s+.*)?(?:\s+-\s+.+)?$/i);
  if (episodePage) name = `${episodePage[2].trim()} Episodio ${episodePage[1].trim()}`;
  else if (episodeTitle) name = `${episodeTitle[1].trim()} Episodio ${episodeTitle[2]}`;
  if (!name) return "";
  if (!isTechnicalFileName(fallbackName) && mediaType !== "video" && mediaType !== "audio") return "";
  const extension = (!isTechnicalFileName(fallbackName) && fileExtension(fallbackName))
    || (mediaType === "audio" ? "mp3" : mediaType === "video" ? "mp4" : "");
  return safeFileName(extension ? `${name}.${extension}` : name);
}

function slugFileName(pageUrl, mediaType, fallbackName) {
  try {
    const url = new URL(pageUrl);
    const slug = url.pathname.split("/").filter(Boolean).pop() || "";
    const slugEpisode = slug.match(/^(.+?)-(\d+)$/);
    if (!slugEpisode) return "";
    const name = `${slugEpisode[1].replace(/[-_]+/g, " ")} Episodio ${slugEpisode[2]}`;
    const extension = fileExtension(fallbackName)
      || (mediaType === "audio" ? "mp3" : mediaType === "video" ? "mp4" : "");
    return safeFileName(extension ? `${name}.${extension}` : name);
  } catch {
    return "";
  }
}

function resolveFileName(candidate) {
  const urlName = fileNameFromUrl(candidate.url);
  const candidateName = optionalFileName(candidate.fileName);
  const plausible = (name) => fileExtension(name) && !isTechnicalFileName(name);
  const detected = (candidateName && plausible(candidateName))
    || (plausible(urlName) ? urlName : candidateName || urlName);
  const fromTitle = titleFileName(candidate.pageTitle, candidate.mediaType, detected);
  if (fromTitle) return fromTitle;
  if (isTechnicalFileName(detected)) {
    return slugFileName(candidate.pageUrl, candidate.mediaType, detected) || detected;
  }
  return detected;
}

function headerValue(headers, name) {
  return headers?.find((header) => header.name.toLowerCase() === name)?.value || "";
}

function rememberRequest(details) {
  const headers = details.requestHeaders || [];
  const previous = requestById.get(details.requestId) || {};
  const request = {
    originalUrl: previous.originalUrl || details.url,
    pageUrl: previous.pageUrl || details.initiator || "",
    referrer: headerValue(headers, "referer"),
    userAgent: headerValue(headers, "user-agent"),
    cookies: parseCookies(headerValue(headers, "cookie")),
  };
  requestById.set(details.requestId, request);
  recentByUrl.set(details.url, request);
  if (requestById.size > 500) requestById.delete(requestById.keys().next().value);
}

function parseCookies(value) {
  return value.split(";").map((part) => part.trim()).filter(Boolean).flatMap((part) => {
    const separator = part.indexOf("=");
    if (separator <= 0) return [];
    return [{ name: part.slice(0, separator).trim(), value: part.slice(separator + 1).trim() }];
  });
}

async function cookiesForUrl(url) {
  if (!api.cookies?.getAll) return [];
  try {
    const cookies = await extensionApiCall(api.cookies.getAll.bind(api.cookies), { url });
    return (cookies || []).map(({ name, value }) => ({ name, value }));
  } catch (error) {
    await logEvent("warning", "cookies", "No se pudieron leer las cookies del recurso.", {
      url: diagnosticUrl(url),
      error: error.message,
    });
    return [];
  }
}

function responseHeader(headers, name) {
  return headers?.find((header) => header.name.toLowerCase() === name)?.value || "";
}

function mediaResponse(details) {
  const contentType = responseHeader(details.responseHeaders, "content-type").toLowerCase();
  const location = responseHeader(details.responseHeaders, "location");
  return details.type === "media"
    || contentType.startsWith("video/")
    || contentType.startsWith("audio/")
    || /\.(mp4|m4v|mkv|webm|mov|avi|mp3|m4a|wav)(?:$|[?#])/i.test(details.url)
    || /\.(mp4|m4v|mkv|webm|mov|avi|mp3|m4a|wav)(?:$|[?#])/i.test(location);
}

async function queueCandidate(candidate) {
  if (!validUrl(candidate.url)) {
    await logEvent("error", "capture", "El enlace multimedia no es HTTP/HTTPS.");
    return { ok: false, error: "El enlace multimedia no es HTTP/HTTPS." };
  }
  const current = await settings();
  if (!current.token) {
    await setBadge("!");
    await logEvent("error", "bridge", "Falta el token de conexión con Fluxor.", {
      url: diagnosticUrl(candidate.url),
    });
    return { ok: false, error: "Falta el token. Abre la extensión y configura Fluxor." };
  }
  const key = `${candidate.tabId || 0}:${candidate.url}`;
  const previous = queuedKeys.get(key);
  if (previous && Date.now() - previous < 15_000) return { ok: true, duplicate: true };
  queuedKeys.set(key, Date.now());
  const fileName = resolveFileName(candidate);
  const cookies = candidate.cookies?.length ? candidate.cookies : await cookiesForUrl(candidate.url);
  const effectiveCandidate = { ...candidate, fileName, cookies };
  try {
    await logEvent("info", "bridge", "Enviando recurso a Fluxor.", {
      url: diagnosticUrl(candidate.url),
      fileName,
      hasReferrer: Boolean(candidate.referrer || candidate.pageUrl),
      hasUserAgent: Boolean(candidate.userAgent),
      cookieCount: cookies.length,
      forceSingleStream: Boolean(candidate.forceSingleStream),
    });
    const response = await fetch(`${BRIDGE_URL}/v1/downloads`, {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-Fluxor-Token": current.token },
      body: JSON.stringify({ input: {
        url: candidate.url,
        fileName,
        pageUrl: candidate.pageUrl || "",
        pageTitle: candidate.pageTitle || "",
        referrer: candidate.referrer || "",
        userAgent: candidate.userAgent || "",
        cookies,
        forceSingleStream: Boolean(candidate.forceSingleStream),
      } }),
    });
    const body = await response.json().catch(() => ({}));
    if (!response.ok || !body.ok) throw new Error(body.error || `Fluxor respondió HTTP ${response.status}.`);
    await rememberCandidate({ ...effectiveCandidate, sentAt: new Date().toISOString(), ok: true });
    await logEvent("info", "bridge", "Fluxor aceptó la descarga.", {
      fileName: body.data?.fileName || fileName,
      url: diagnosticUrl(candidate.url),
    });
    await setBadge("");
    return { ok: true, item: body.data };
  } catch (error) {
    queuedKeys.delete(key);
    await rememberCandidate({ ...effectiveCandidate, sentAt: new Date().toISOString(), ok: false, error: error.message });
    await logEvent("error", "bridge", "No se pudo enviar la descarga a Fluxor.", {
      error: error.message,
      fileName,
      url: diagnosticUrl(candidate.url),
    });
    await setBadge("!");
    return { ok: false, error: error.message || "No se pudo contactar con Fluxor." };
  }
}

async function rememberCandidate(candidate) {
  const current = await settings();
  const candidates = [candidate, ...(current.candidates || [])]
    .filter((entry, index, all) => index === all.findIndex((other) => other.url === entry.url))
    .slice(0, MAX_CANDIDATES);
  await extensionApiCall(api.storage.local.set.bind(api.storage.local), { candidates });
}

async function setBadge(text) {
  if (!api.action?.setBadgeText) return;
  await extensionApiCall(api.action.setBadgeText.bind(api.action), { text });
}

function requestContext(details) {
  return requestById.get(details.requestId) || recentByUrl.get(details.url) || {};
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
    const location = responseHeader(details.responseHeaders, "location");
    const contentType = responseHeader(details.responseHeaders, "content-type").split(";", 1)[0].toLowerCase();
    const candidate = {
      url: details.url,
      fileName: fileNameFromDisposition(responseHeader(details.responseHeaders, "content-disposition"))
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
    mediaType: context.mediaType,
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
      return queueCandidate({
        ...candidate,
        pageTitle: sender.tab?.title || candidate.pageTitle,
        pageUrl: sender.tab?.url || candidate.pageUrl,
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
