export function formatBytes(bytes: number | undefined, decimals = 1): string {
  if (bytes === undefined) return "—";
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const unitIndex = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** unitIndex).toFixed(unitIndex === 0 ? 0 : decimals)} ${units[unitIndex]}`;
}

export function formatSpeed(bytes: number): string {
  return bytes > 0 ? `${formatBytes(bytes)}/s` : "—";
}

export function formatEta(total: number | undefined, downloaded: number, speed: number): string {
  if (!total || speed <= 0 || downloaded >= total) return "—";
  const seconds = Math.ceil((total - downloaded) / speed);
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.ceil(seconds / 60)}m`;
  return `${Math.floor(seconds / 3600)}h ${Math.ceil((seconds % 3600) / 60)}m`;
}

export function relativeTime(isoDate: string): string {
  const elapsed = Math.max(0, Date.now() - new Date(isoDate).getTime());
  const minutes = Math.floor(elapsed / 60_000);
  if (minutes < 1) return "ahora";
  if (minutes < 60) return `hace ${minutes} min`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `hace ${hours} h`;
  return `hace ${Math.floor(hours / 24)} d`;
}

export function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return "origen desconocido";
  }
}

export function redactUrl(value: string): string {
  try {
    const url = new URL(value);
    url.username = "";
    url.password = "";
    const hasQuery = Boolean(url.search);
    url.search = "";
    url.hash = "";
    return `${url.toString()}${hasQuery ? "?•••" : ""}`;
  } catch {
    return "Enlace protegido";
  }
}
