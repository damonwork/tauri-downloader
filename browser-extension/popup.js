const api = globalThis.browser ?? globalThis.chrome;
const token = document.getElementById("token");
const autoCapture = document.getElementById("autoCapture");
const overlay = document.getElementById("overlay");
const status = document.getElementById("status");
const candidates = document.getElementById("candidates");
const logs = document.getElementById("logs");
const logLevel = document.getElementById("logLevel");
const copyStatus = document.getElementById("copyStatus");
let currentLogs = [];

const LEVEL_RANK = { debug: 0, info: 1, warning: 2, error: 3 };

function message(message) {
  return api.runtime.sendMessage(message);
}

function render(items) {
  candidates.textContent = "";
  for (const item of items || []) {
    const row = document.createElement("li");
    row.className = item.ok === true ? "ok" : item.status === "warning" ? "warning" : item.ok === false ? "error" : "detected";
    row.title = item.error || item.url;
    const label = document.createElement("span");
    label.textContent = `${item.ok === true ? "✓" : item.status === "warning" ? "!" : item.ok === false ? "!" : "•"} ${item.fileName || item.url}`;
    row.appendChild(label);
    if (item.ok !== true) {
      const send = document.createElement("button");
      send.type = "button";
      send.textContent = "Enviar";
      send.addEventListener("click", async () => {
        send.disabled = true;
        try {
          const result = await message({ type: "captureCandidate", candidate: item });
          send.textContent = result?.ok ? "Enviado" : result?.status === "warning" ? "Reintentar" : "Error";
        } catch (error) {
          send.textContent = "Reintentar";
          send.title = error.message || "Recarga la extensión para continuar.";
        } finally {
          send.disabled = false;
        }
      });
      row.appendChild(send);
    }
    candidates.appendChild(row);
  }
  if (!candidates.children.length) {
    const row = document.createElement("li");
    row.textContent = "Todavía no hay capturas.";
    candidates.appendChild(row);
  }
}

function visibleLogs(items) {
  const minimum = LEVEL_RANK[logLevel.value] ?? 1;
  return (items || []).filter((item) => (LEVEL_RANK[item.level] ?? 1) >= minimum);
}

function renderLogs(items) {
  logs.textContent = "";
  for (const item of visibleLogs(items)) {
    const row = document.createElement("li");
    row.className = `log ${item.level || "info"}`;
    const time = new Date(item.at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
    const summary = document.createElement("strong");
    summary.textContent = `${time} · ${item.event || "evento"}`;
    const detail = document.createElement("span");
    detail.textContent = item.message || "";
    row.title = JSON.stringify(item.details || {}, null, 2);
    row.append(summary, detail);
    logs.appendChild(row);
  }
  if (!logs.children.length) {
    const row = document.createElement("li");
    row.textContent = "Sin eventos todavía.";
    logs.appendChild(row);
  }
}

async function load() {
  const state = await message({ type: "getState" });
  token.value = state.token || "";
  autoCapture.checked = state.autoCapture !== false;
  overlay.checked = state.overlay !== false;
  render(state.candidates);
  currentLogs = state.logs || [];
  renderLogs(currentLogs);
}

document.getElementById("save").addEventListener("click", async () => {
  await message({ type: "saveState", state: { token: token.value.trim(), autoCapture: autoCapture.checked, overlay: overlay.checked } });
  status.textContent = "Guardado";
});

document.getElementById("test").addEventListener("click", async () => {
  status.textContent = "Probando...";
  await message({ type: "saveState", state: { token: token.value.trim(), autoCapture: autoCapture.checked, overlay: overlay.checked } });
  const result = await message({ type: "testConnection" });
  status.textContent = result.ok ? "Conectado" : result.error;
  await load();
});

document.getElementById("clearLogs").addEventListener("click", async () => {
  await message({ type: "clearLogs" });
  await load();
});

document.getElementById("clearCandidates").addEventListener("click", async () => {
  await message({ type: "clearCandidates" });
  await load();
});

document.getElementById("copyLogs").addEventListener("click", async () => {
  const text = visibleLogs(currentLogs)
    .map((item) => `${item.at} [${String(item.level || "info").toUpperCase()}] ${item.event}: ${item.message}${Object.keys(item.details || {}).length ? ` ${JSON.stringify(item.details)}` : ""}`)
    .join("\n");
  try {
    await navigator.clipboard.writeText(text || "Sin eventos para el nivel seleccionado.");
    copyStatus.textContent = "Logs copiados";
  } catch {
    copyStatus.textContent = "No se pudieron copiar los logs";
  }
  window.setTimeout(() => { copyStatus.textContent = ""; }, 1800);
});

logLevel.addEventListener("change", () => renderLogs(currentLogs));

api.storage.onChanged.addListener((changes, area) => {
  if (area !== "local") return;
  if (changes.candidates) render(changes.candidates.newValue || []);
  if (changes.logs) {
    currentLogs = changes.logs.newValue || [];
    renderLogs(currentLogs);
  }
});

load().catch((error) => { status.textContent = error.message; });
