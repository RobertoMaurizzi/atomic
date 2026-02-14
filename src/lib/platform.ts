import { getTransport } from './transport';

export async function openExternalUrl(url: string): Promise<void> {
  if (getTransport().mode === 'tauri') {
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(url);
  } else {
    window.open(url, '_blank', 'noopener,noreferrer');
  }
}

export async function pickDirectory(title?: string): Promise<string | null> {
  if (getTransport().mode === 'tauri') {
    const { open } = await import('@tauri-apps/plugin-dialog');
    return await open({ directory: true, multiple: false, title }) as string | null;
  }
  return null; // Not available in web/remote mode
}

export function isTauri(): boolean {
  return typeof window !== 'undefined' && !!(window as any).__TAURI_INTERNALS__;
}
