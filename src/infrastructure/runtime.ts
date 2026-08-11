import type { DownloadGateway } from "@/application/download-gateway";
import { TauriDownloadGateway } from "./tauri-download-gateway";
import { WebDownloadGateway } from "./web-download-gateway";

function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

export function createDownloadGateway(): DownloadGateway {
  return isTauriRuntime() ? new TauriDownloadGateway() : new WebDownloadGateway();
}
