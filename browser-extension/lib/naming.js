// lib/naming.js — Resolución del nombre de archivo para todo tipo de recursos.
// Prioridad: nombre del navegador/Content-Disposition -> título del episodio -> URL -> slug.
// Las funciones de episodios (titleFileName/slugFileName) actúan cuando el
// recurso es multimedia o cuando el nombre detectado es técnico (no describe
// el contenido); el título nunca se impone sobre un nombre real y plausible,
// en línea con page_file_name de src-tauri (manager/browser.rs).
// IMPORTANTE: resolveFileName SIEMPRE devuelve un string (nunca true/false).

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
  const extended = value.match(/filename\*\s*=\s*UTF-8''([^;]+)/i)?.[1];
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
  if (fileExtension(fallbackName) && !isTechnicalFileName(fallbackName)) return "";
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
  const detected = (candidateName && plausible(candidateName) && candidateName)
    || (plausible(urlName) ? urlName : candidateName || urlName);
  const fromTitle = titleFileName(candidate.pageTitle, candidate.mediaType, detected);
  if (fromTitle) return fromTitle;
  if (isTechnicalFileName(detected)) {
    return slugFileName(candidate.pageUrl, candidate.mediaType, detected) || detected;
  }
  return detected;
}

function mediaTypeFromUrl(value) {
  const extension = String(value).match(/\.([a-z0-9]{2,5})(?:$|[?#])/i)?.[1]?.toLowerCase() || "";
  return ["mp3", "m4a", "wav", "aac", "ogg", "flac"].includes(extension)
    ? "audio"
    : ["mp4", "m4v", "mkv", "webm", "mov", "avi"].includes(extension)
      ? "video"
      : "";
}
