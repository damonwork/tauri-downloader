const api = globalThis.browser ?? globalThis.chrome;
const token = document.getElementById("token");
const autoCapture = document.getElementById("autoCapture");
const overlay = document.getElementById("overlay");
const status = document.getElementById("status");
const candidates = document.getElementById("candidates");

function message(message) {
  return api.runtime.sendMessage(message);
}

function render(items) {
  candidates.textContent = "";
  for (const item of items || []) {
    const row = document.createElement("li");
    row.className = item.ok === true ? "ok" : item.ok === false ? "error" : "detected";
    row.title = item.error || item.url;
    const label = document.createElement("span");
    label.textContent = `${item.ok === true ? "✓" : item.ok === false ? "!" : "•"} ${item.fileName || item.url}`;
    row.appendChild(label);
    if (item.ok !== true) {
      const send = document.createElement("button");
      send.type = "button";
      send.textContent = "Enviar";
      send.addEventListener("click", async () => {
        send.disabled = true;
        const result = await message({ type: "captureCandidate", candidate: item });
        send.textContent = result.ok ? "Enviado" : "Error";
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

async function load() {
  const state = await message({ type: "getState" });
  token.value = state.token || "";
  autoCapture.checked = state.autoCapture !== false;
  overlay.checked = state.overlay !== false;
  render(state.candidates);
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
});

load().catch((error) => { status.textContent = error.message; });
