// lib/capture.js — Captura de recursos: contexto de petición, detección
// multimedia y envío al puente de Fluxor.
// La memoria de contexto (requestById/recentByUrl) se alimenta desde
// onBeforeSendHeaders y se consulta al detectar un medio o una descarga.
// queueCandidate centraliza: deduplicación (queuedKeys), resolución del
// nombre, cookies y la llamada al bridge. Los detalles del nombre viven
// en lib/naming.js; los logs, en lib/log.js; la persistencia, en lib/store.js.

const BRIDGE_URL = "http://127.0.0.1:17846";
const requestById = new Map();
const recentByUrl = new Map();
const queuedKeys = new Map();
const lastMediaLog = new Map();

function headerValue(headers, name) {
  return headers?.find((header) => header.name.toLowerCase() === name)?.value || "";
}

function parseCookies(value) {
  return value.split(";").map((part) => part.trim()).filter(Boolean).flatMap((part) => {
    const separator = part.indexOf("=");
    if (separator <= 0) return [];
    return [{ name: part.slice(0, separator).trim(), value: part.slice(separator + 1).trim() }];
  });
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
  if (recentByUrl.size > 500) recentByUrl.delete(recentByUrl.keys().next().value);
}

function requestContext(details) {
  return requestById.get(details.requestId) || recentByUrl.get(details.url) || {};
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

function mediaResponse(details) {
  const contentType = headerValue(details.responseHeaders, "content-type").toLowerCase();
  const location = headerValue(details.responseHeaders, "location");
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
    queuedKeys.set(key, Date.now());
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
