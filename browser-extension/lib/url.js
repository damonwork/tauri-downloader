// lib/url.js — Utilidades de URL compartidas por captura y diagnóstico.
// Mantener funciones puras y sin estado: no declarar almacenamiento aquí.

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

function diagnosticUrl(value) {
  try {
    const url = new URL(value);
    return `${url.origin}${url.pathname}`;
  } catch {
    return String(value || "");
  }
}
