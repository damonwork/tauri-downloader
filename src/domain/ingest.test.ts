import { describe, expect, it } from "vitest";
import { IngestError, parseCookies, parseHeaderLines, parseRequest } from "./ingest";

describe("parseRequest", () => {
  it("normaliza una URL HTTP sin credenciales", () => {
    const result = parseRequest("https://user:secret@example.com/releases/file.zip?token=abc");

    expect(result.source.url).toBe("https://example.com/releases/file.zip?token=abc");
    expect(result.fileName).toBe("file.zip");
    expect(result.source.proxy).toEqual({ kind: "direct" });
  });

  it("extrae URL, headers y cookies de cURL sin ejecutar comandos", () => {
    const result = parseRequest(
      "curl 'https://cdn.example.com/video.mkv' -H 'Authorization: Bearer secret' -A 'Fluxor/1' -b 'session=abc; theme=dark'",
    );

    expect(result.source.headers).toEqual([
      { name: "Authorization", value: "Bearer secret" },
      { name: "User-Agent", value: "Fluxor/1" },
    ]);
    expect(result.source.cookies).toEqual([
      { name: "session", value: "abc" },
      { name: "theme", value: "dark" },
    ]);
  });

  it("rechaza opciones que pueden leer archivos locales", () => {
    expect(() => parseRequest("curl https://example.com/file -K ~/.curlrc")).toThrow(IngestError);
    expect(() => parseRequest("curl https://example.com/file -b @cookies.txt")).toThrow(
      "No se permiten archivos de cookies locales.",
    );
  });

  it("rechaza métodos que no representan una descarga", () => {
    expect(() => parseRequest("curl -X POST https://example.com/export")).toThrow(
      "El método POST no es una descarga GET/HEAD.",
    );
  });

  it("detecta comillas incompletas", () => {
    expect(() => parseRequest("curl 'https://example.com/file")).toThrow(
      "El comando contiene comillas o escapes incompletos.",
    );
  });

  it("no incluye credenciales del proxy en advertencias", () => {
    const result = parseRequest(
      "curl https://example.com/file.zip --proxy socks5://user:secret@proxy.example:1080",
    );
    expect(result.warnings).toEqual([
      "Proxy detectado en cURL. Créalo como perfil para reutilizarlo.",
    ]);
    expect(result.warnings.join(" ")).not.toContain("secret");
  });
});

describe("credential helpers", () => {
  it("mantiene valores con separadores internos", () => {
    expect(parseCookies("token=a=b=c; mode=fast")).toEqual([
      { name: "token", value: "a=b=c" },
      { name: "mode", value: "fast" },
    ]);
    expect(parseHeaderLines("Referer: https://example.com/a:b\nX-Mode: fast")).toEqual([
      { name: "Referer", value: "https://example.com/a:b" },
      { name: "X-Mode", value: "fast" },
    ]);
  });
});
