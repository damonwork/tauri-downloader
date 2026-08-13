const api = globalThis.browser ?? globalThis.chrome;
const BUTTON_CLASS = "fluxor-capture-button";

// validUrl y fileNameFromUrl viven en lib/url.js y lib/naming.js y se cargan
// aquí a través de content_scripts del manifest (mismo mundo aislado).

function watchGitHubArtifactClicks() {
  if (!/^https:\/\/github\.com\//.test(location.href)) return;
  document.addEventListener("click", (event) => {
    const target = event.target;
    const anchor = target?.closest ? target.closest("a[href*='/artifacts/']") : null;
    if (!anchor) return;
    if (!/\/actions\/runs\/\d+\/artifacts\/\d+/.test(anchor.getAttribute("href") || "")) return;
    const label = (anchor.getAttribute("aria-label") || "").toLowerCase();
    if (!label.startsWith("download") && (anchor.textContent || "").trim() !== "Download") return;
    const labelName = (anchor.getAttribute("aria-label") || "")
      .replace(/^Download\s+/i, "").replace(/\s*\(opens in a new tab\)\s*$/i, "").trim();
    const rowName = anchor.closest("tr")?.querySelector("a.text-bold")?.textContent?.trim();
    const titleName = document.title.replace(/\s*·.*$/, "").trim();
    const name = (labelName && labelName !== "Download" && labelName)
      || rowName
      || (/^actions$/i.test(titleName) ? "" : titleName);
    if (!name) return;
    api.runtime.sendMessage({ type: "pendingArtifact", page: location.href, name, href: anchor.href });
  }, true);
}

function addStyle() {
  if (document.getElementById("fluxor-capture-style")) return;
  const style = document.createElement("style");
  style.id = "fluxor-capture-style";
  style.textContent = `.${BUTTON_CLASS}{position:absolute;top:12px;right:12px;z-index:2147483647;border:0;border-radius:5px;padding:8px 10px;background:#c2ff5b;color:#10170d;font:700 12px system-ui,sans-serif;box-shadow:0 3px 12px #0008;cursor:pointer;opacity:.9}.${BUTTON_CLASS}:hover{opacity:1;transform:translateY(-1px)}.${BUTTON_CLASS}[data-state=busy]{background:#d3a24d}.${BUTTON_CLASS}[data-state=warning]{background:#e6bd59;color:#211b08}.${BUTTON_CLASS}[data-state=error]{background:#d98078}`;
  document.documentElement.appendChild(style);
}

function videoUrl(video) {
  return video.currentSrc || video.src || video.querySelector("source")?.src || "";
}

function warningLabel(code) {
  return {
    missing_token: "Configura el token",
    bridge_unavailable: "Abre Fluxor",
    bridge_timeout: "Fluxor no responde",
    invalid_token: "Revisa el token",
    extension_unavailable: "Recarga la extensión",
  }[code] || "Revisa Fluxor";
}

function resetButton(button) {
  button.textContent = "Descargar con Fluxor";
  button.title = "";
  button.removeAttribute("aria-label");
  button.dataset.state = "";
}

function showResult(button, result) {
  const warning = result?.status === "warning";
  button.dataset.state = result?.ok ? "" : warning ? "warning" : "error";
  button.title = result?.ok ? "Descarga enviada a Fluxor" : result?.error || "Error desconocido";
  button.textContent = result?.ok ? "Enviado a Fluxor" : warning ? warningLabel(result.code) : "Error al enviar";
  button.setAttribute("aria-label", button.textContent);
  window.setTimeout(() => resetButton(button), result?.ok || !warning ? 4000 : 7000);
}

function capture(video, button) {
  const url = videoUrl(video);
  if (!validUrl(url)) {
    showResult(button, { ok: false, status: "error", error: "El vídeo no tiene una URL válida." });
    return;
  }
  button.dataset.state = "busy";
  button.textContent = "Enviando...";
  api.runtime.sendMessage({
    type: "captureCandidate",
    candidate: {
      url,
      fileName: fileNameFromUrl(url),
      pageUrl: location.href,
      pageTitle: document.title,
      mediaType: "video",
      referrer: document.referrer,
      forceSingleStream: true,
    },
  }).then((result) => {
    showResult(button, result);
  }).catch((error) => {
    showResult(button, {
      ok: false,
      status: "warning",
      code: "extension_unavailable",
      error: error.message || "Recarga la extensión para continuar.",
    });
  });
}

function addButton(video) {
  if (video.dataset.fluxorCapture || !video.isConnected) return;
  video.dataset.fluxorCapture = "true";
  const host = video.closest(".plyr") || video.parentElement;
  if (!host) return;
  if (getComputedStyle(host).position === "static") host.style.position = "relative";
  const button = document.createElement("button");
  button.className = BUTTON_CLASS;
  button.type = "button";
  button.textContent = "Descargar con Fluxor";
  button.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();
    capture(video, button);
  });
  host.appendChild(button);
}

async function start() {
  const options = await api.runtime.sendMessage({ type: "getOptions" });
  if (options?.overlay === false) return;
  addStyle();
  document.querySelectorAll("video").forEach(addButton);
  const observer = new MutationObserver((mutations) => {
    const addedVideo = mutations.some((mutation) => Array.from(mutation.addedNodes).some((node) => {
      return node.nodeType === 1 && (node.nodeName === "VIDEO" || node.querySelector?.("video"));
    }));
    if (!addedVideo) return;
    document.querySelectorAll("video").forEach(addButton);
  });
  observer.observe(document.documentElement, { childList: true, subtree: true });
}

watchGitHubArtifactClicks();
start().catch(() => {});
