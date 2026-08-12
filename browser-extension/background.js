const api = globalThis.browser ?? globalThis.chrome;
const BRIDGE_URL = "http://127.0.0.1:17846";
const MAX_CANDIDATES = 20;
const requestById = new Map();
const recentByUrl = new Map();
const queuedKeys = new Map();

const DEFAULTS = {
  token: "",
  autoCapture: true,
  overlay: true,
  candidates: [],
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

function fileNameFromUrl(value) {
  try {
    const url = new URL(value);
    const disposition = url.searchParams.get("response-content-disposition") || "";
    const dispositionName = disposition.match(/filename\*?=(?:UTF-8''|[\"]?)([^;\"]+)/i)?.[1];
    const queryName = url.searchParams.get("filename") || url.searchParams.get("file_name");
    const pathName = decodeURIComponent(url.pathname.split("/").filter(Boolean).pop() || "");
    return safeFileName(dispositionName || queryName || pathName || `descarga-${url.hostname}`);
  } catch {
    return "descarga";
  }
}

function headerValue(headers, name) {
  return headers?.find((header) => header.name.toLowerCase() === name)?.value || "";
}

function rememberRequest(details) {
  const headers = details.requestHeaders || [];
  const previous = requestById.get(details.requestId) || {};
  const request = {
    originalUrl: previous.originalUrl || details.url,
    pageUrl: details.initiator || previous.pageUrl || "",
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
  if (!validUrl(candidate.url)) return { ok: false, error: "El enlace multimedia no es HTTP/HTTPS." };
  const key = `${candidate.tabId || 0}:${candidate.url}`;
  const previous = queuedKeys.get(key);
  if (previous && Date.now() - previous < 15_000) return { ok: true, duplicate: true };
  queuedKeys.set(key, Date.now());
  const current = await settings();
  if (!current.token) {
    await setBadge("!");
    return { ok: false, error: "Configura el token de Fluxor desde el popup." };
  }
  try {
    const response = await fetch(`${BRIDGE_URL}/v1/downloads`, {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-Fluxor-Token": current.token },
      body: JSON.stringify({ input: {
        url: candidate.url,
        fileName: candidate.fileName || fileNameFromUrl(candidate.url),
        pageUrl: candidate.pageUrl || "",
        referrer: candidate.referrer || "",
        userAgent: candidate.userAgent || "",
        cookies: candidate.cookies || [],
        forceSingleStream: Boolean(candidate.forceSingleStream),
      } }),
    });
    const body = await response.json().catch(() => ({}));
    if (!response.ok || !body.ok) throw new Error(body.error || `Fluxor respondió HTTP ${response.status}.`);
    await rememberCandidate({ ...candidate, sentAt: new Date().toISOString(), ok: true });
    await setBadge("");
    return { ok: true, item: body.data };
  } catch (error) {
    await rememberCandidate({ ...candidate, sentAt: new Date().toISOString(), ok: false, error: error.message });
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

api.webRequest.onBeforeSendHeaders.addListener(
  rememberRequest,
  { urls: ["<all_urls>"] },
  ["requestHeaders"],
);

api.webRequest.onHeadersReceived.addListener(
  async (details) => {
    if (!mediaResponse(details)) return;
    const context = requestContext(details);
    await rememberCandidate({
      url: context.originalUrl || details.url,
      fileName: fileNameFromUrl(responseHeader(details.responseHeaders, "location") || details.url),
      pageUrl: context.pageUrl,
      referrer: context.referrer,
      userAgent: context.userAgent,
      cookies: context.cookies,
      tabId: details.tabId,
      forceSingleStream: true,
      detectedAt: new Date().toISOString(),
    });
  },
  { urls: ["<all_urls>"] },
  ["responseHeaders"],
);

api.downloads.onCreated.addListener(async (download) => {
  const current = await settings();
  if (!current.autoCapture || !validUrl(download.url)) return;
  const context = recentByUrl.get(download.url) || {};
  const result = await queueCandidate({
    url: download.url,
    fileName: safeFileName(download.filename?.split(/[\\/]/).pop()) || fileNameFromUrl(download.url),
    pageUrl: context.pageUrl,
    referrer: download.referrer || context.referrer,
    userAgent: context.userAgent,
    cookies: context.cookies,
    tabId: download.tabId,
    forceSingleStream: signedUrl(download.url),
  });
  if (result.ok) await extensionApiCall(api.downloads.cancel.bind(api.downloads), download.id);
});

api.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  (async () => {
    if (message.type === "getState") return settings();
    if (message.type === "saveState") {
      const next = { ...DEFAULTS, ...(message.state || {}) };
      await extensionApiCall(api.storage.local.set.bind(api.storage.local), next);
      return next;
    }
    if (message.type === "captureCandidate") return queueCandidate(message.candidate || {});
    if (message.type === "testConnection") {
      const current = await settings();
      if (!current.token) return { ok: false, error: "Falta el token." };
      const response = await fetch(`${BRIDGE_URL}/v1/status`, { headers: { "X-Fluxor-Token": current.token } });
      return response.ok ? { ok: true } : { ok: false, error: `Fluxor respondió HTTP ${response.status}.` };
    }
    return { ok: false, error: "Mensaje no reconocido." };
  })().then(sendResponse).catch((error) => sendResponse({ ok: false, error: error.message }));
  return true;
});
