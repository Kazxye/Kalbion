import { invoke, isTauri } from '@tauri-apps/api/core';
export const desktop = isTauri();
export async function request<T>(
  operation: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (!desktop)
    throw new Error(
      'O core Rust não está conectado. Execute npm run tauri dev para usar o aplicativo desktop.',
    );
  return invoke<T>('dispatch', { request: { operation, ...args } });
}
export async function exportSession(sessionId: string, format: string) {
  return invoke<boolean>('export_session', { sessionId, format });
}
