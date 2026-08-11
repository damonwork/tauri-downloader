import {
  fileNameFromUrl,
  type CookieEntry,
  type HeaderEntry,
  type ParsedRequest,
} from "./download";

const URL_PATTERN = /^https?:\/\//i;
const FORBIDDEN_OPTIONS = new Set([
  "--config",
  "-K",
  "--data-binary",
  "--upload-file",
  "-T",
  "--form",
  "-F",
]);

export class IngestError extends Error {}

function tokenizeShell(input: string): string[] {
  const tokens: string[] = [];
  let current = "";
  let quote: "'" | '"' | undefined;
  let escaped = false;

  for (const character of input.trim()) {
    if (escaped) {
      current += character;
      escaped = false;
      continue;
    }
    if (character === "\\" && quote !== "'") {
      escaped = true;
      continue;
    }
    if (quote) {
      if (character === quote) quote = undefined;
      else current += character;
      continue;
    }
    if (character === "'" || character === '"') {
      quote = character;
      continue;
    }
    if (/\s/.test(character)) {
      if (current) tokens.push(current);
      current = "";
      continue;
    }
    current += character;
  }

  if (escaped || quote) throw new IngestError("El comando contiene comillas o escapes incompletos.");
  if (current) tokens.push(current);
  return tokens;
}

function parseHeader(raw: string): HeaderEntry {
  const separator = raw.indexOf(":");
  if (separator <= 0) throw new IngestError(`Header inválido: ${raw}`);
  const name = raw.slice(0, separator).trim();
  const value = raw.slice(separator + 1).trim();
  if (/\r|\n/.test(name + value)) throw new IngestError("Los headers no pueden contener saltos de línea.");
  return { name, value };
}

export function parseHeaderLines(raw: string): HeaderEntry[] {
  return raw
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .map(parseHeader);
}

export function parseCookies(raw: string): CookieEntry[] {
  return raw
    .split(";")
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => {
      const separator = part.indexOf("=");
      if (separator < 1) throw new IngestError(`Cookie inválida: ${part}`);
      return { name: part.slice(0, separator).trim(), value: part.slice(separator + 1).trim() };
    });
}

function assertHttpUrl(value: string): string {
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new IngestError("No se encontró una URL válida.");
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new IngestError("Solo se permiten enlaces HTTP o HTTPS.");
  }
  url.username = "";
  url.password = "";
  return url.toString();
}

function nextValue(tokens: string[], index: number, option: string): string {
  const value = tokens[index + 1];
  if (!value || value.startsWith("-")) throw new IngestError(`${option} necesita un valor.`);
  return value;
}

export function parseRequest(input: string): ParsedRequest {
  const trimmed = input.trim();
  if (!trimmed) throw new IngestError("Pega un enlace o un comando cURL.");

  if (URL_PATTERN.test(trimmed)) {
    const url = assertHttpUrl(trimmed);
    return {
      source: { url, headers: [], cookies: [], proxy: { kind: "direct" } },
      fileName: fileNameFromUrl(url),
      warnings: [],
    };
  }

  const tokens = tokenizeShell(trimmed);
  if (tokens[0]?.toLowerCase() !== "curl") {
    throw new IngestError("La entrada debe ser una URL o comenzar con curl.");
  }

  const headers: HeaderEntry[] = [];
  const cookies: CookieEntry[] = [];
  const warnings: string[] = [];
  let rawUrl = "";
  let proxyUrl = "";

  for (let index = 1; index < tokens.length; index += 1) {
    const token = tokens[index];
    if (FORBIDDEN_OPTIONS.has(token)) {
      throw new IngestError(`${token} no se admite por seguridad.`);
    }
    if (token === "-H" || token === "--header") {
      headers.push(parseHeader(nextValue(tokens, index, token)));
      index += 1;
      continue;
    }
    if (token === "-b" || token === "--cookie") {
      const value = nextValue(tokens, index, token);
      if (value.startsWith("@")) throw new IngestError("No se permiten archivos de cookies locales.");
      cookies.push(...parseCookies(value));
      index += 1;
      continue;
    }
    if (token === "-A" || token === "--user-agent") {
      headers.push({ name: "User-Agent", value: nextValue(tokens, index, token) });
      index += 1;
      continue;
    }
    if (token === "-e" || token === "--referer") {
      headers.push({ name: "Referer", value: nextValue(tokens, index, token) });
      index += 1;
      continue;
    }
    if (token === "-x" || token === "--proxy") {
      proxyUrl = nextValue(tokens, index, token);
      index += 1;
      continue;
    }
    if (token === "--url") {
      rawUrl = nextValue(tokens, index, token);
      index += 1;
      continue;
    }
    if (token === "-X" || token === "--request") {
      const method = nextValue(tokens, index, token).toUpperCase();
      if (method !== "GET" && method !== "HEAD") {
        throw new IngestError(`El método ${method} no es una descarga GET/HEAD.`);
      }
      index += 1;
      continue;
    }
    if (URL_PATTERN.test(token)) rawUrl = token;
    else if (token.startsWith("-")) warnings.push(`Opción ignorada: ${token}`);
  }

  const url = assertHttpUrl(rawUrl);
  if (proxyUrl) warnings.push("Proxy detectado en cURL. Créalo como perfil para reutilizarlo.");
  return {
    source: { url, headers, cookies, proxy: { kind: "direct" } },
    fileName: fileNameFromUrl(url),
    warnings,
  };
}
