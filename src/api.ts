import { invoke, isTauri } from '@tauri-apps/api/core';
import type { CaptureSummary, CatalogInfo, MarketRefresh } from './types';

export const desktop = isTauri();
const disconnected =
  'O core Rust não está conectado. Execute npm run tauri dev para usar o aplicativo desktop.';
export async function request<T>(
  operation: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (!desktop) throw new Error(disconnected);
  return invoke<T>('dispatch', { request: { operation, ...args } });
}
export async function exportSession(sessionId: string, format: string) {
  if (!desktop) throw new Error(disconnected);
  return invoke<boolean>('export_session', { sessionId, format });
}
/** Resolves to null when the user cancels the native file dialog. */
export async function importCatalog() {
  if (!desktop) throw new Error(disconnected);
  return invoke<CatalogInfo | null>('import_catalog');
}
/** Fetches Albion Data Project quotes; the request runs outside the database lock. */
export async function refreshMarketPrices(sessionId: string) {
  if (!desktop) throw new Error(disconnected);
  return invoke<MarketRefresh>('refresh_market_prices', { sessionId });
}
/** Offline import of a capture file; resolves to null when the dialog is cancelled. */
export async function importCapture(sessionId: string, roster: string[]) {
  if (!desktop) throw new Error(disconnected);
  return invoke<CaptureSummary | null>('import_capture', { sessionId, roster });
}
