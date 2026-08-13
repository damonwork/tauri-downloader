const api = globalThis.browser ?? globalThis.chrome;
const BUTTON_CLASS = "fluxor-capture-button";

function validUrl(value) {
  try {
    const url = new URL(value);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

function fileNameFromUrl(value) {
  try {
    const url = new URL(value);
    return decodeURIComponent(url.pathname.split("/").filter(Boolean).pop() || "video");
  } catch {
    return "video";
  }
}

function addStyle() {
  if (document.getElementById("fluxor-capture-style")) return;
  const style = document.createElement("style");
  style.id = "fluxor-capture-style";
  style.textContent = `.${BUTTON_CLASS}{position:absolute;top:12px;right:12px;z-index:2147483647;border:0;border-radius:5px;padding:8px 10px;background:#c2ff5b;color:#10170d;font:700 12px system-ui,sans-serif;box-shadow:0 3px 12px #0008;cursor:pointer;opacity:.9}.${BUTTON_CLASS}:hover{opacity:1;transform:translateY(-1px)}.${BUTTON_CLASS}[data-state=busy]{background:#d3a24d}.${BUTTON_CLASS}[data-state=error]{background:#d98078}`;
  document.documentElement.appendChild(style);
}

function videoUrl(video) {
  return video.currentSrc || video.src || video.querySelector("source")?.src || "";
}

function capture(video, button) {
  const url = videoUrl(video);
  if (!validUrl(url)) {
    button.dataset.state = "error";
    button.textContent = "Sin URL";
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
    button.dataset.state = result?.ok ? "" : "error";
    button.title = result?.ok ? "Descarga enviada a Fluxor" : result?.error || "Error desconocido";
    button.textContent = result?.ok ? "Enviado a Fluxor" : "Error al enviar";
    window.setTimeout(() => {
      button.textContent = "Descargar con Fluxor";
      button.title = "";
      button.dataset.state = "";
    }, 2400);
  }).catch((error) => {
    button.dataset.state = "error";
    button.title = error.message || "No se pudo contactar con la extensión";
    button.textContent = "Error al enviar";
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
  const state = await api.runtime.sendMessage({ type: "getState" });
  if (state?.overlay === false) return;
  addStyle();
  document.querySelectorAll("video").forEach(addButton);
  new MutationObserver(() => document.querySelectorAll("video").forEach(addButton)).observe(document.documentElement, { childList: true, subtree: true });
}

start().catch(() => {});
