import { invoke, isTauri } from '@tauri-apps/api/core';
import type { CatalogInfo } from './types';

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
