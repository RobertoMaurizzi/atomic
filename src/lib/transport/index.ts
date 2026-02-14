import type { Transport, HttpTransportConfig } from './types';
export type { Transport, HttpTransportConfig };

let activeTransport: Transport | null = null;

export function getTransport(): Transport {
  if (!activeTransport) throw new Error('Transport not initialized. Call initTransport() first.');
  return activeTransport;
}

export async function initTransport(): Promise<void> {
  // Auto-detect: if Tauri runtime is present, use TauriTransport
  if (typeof window !== 'undefined' && (window as any).__TAURI_INTERNALS__) {
    const { TauriTransport } = await import('./tauri');
    activeTransport = new TauriTransport();
    await activeTransport.connect();
  } else {
    // Web SPA — require explicit config from localStorage or prompt user
    const saved = localStorage.getItem('atomic-server-config');
    if (saved) {
      const config: HttpTransportConfig = JSON.parse(saved);
      const { HttpTransport } = await import('./http');
      activeTransport = new HttpTransport(config);
      await activeTransport.connect();
    } else {
      // Create a disconnected HttpTransport — user must configure via settings
      const { HttpTransport } = await import('./http');
      activeTransport = new HttpTransport({ baseUrl: '', authToken: '' });
    }
  }
}

export async function switchTransport(config: HttpTransportConfig): Promise<void> {
  if (activeTransport) activeTransport.disconnect();
  const { HttpTransport } = await import('./http');
  activeTransport = new HttpTransport(config);
  await activeTransport.connect();
  localStorage.setItem('atomic-server-config', JSON.stringify(config));
}

export async function switchToLocal(): Promise<void> {
  if (activeTransport) activeTransport.disconnect();
  const { TauriTransport } = await import('./tauri');
  activeTransport = new TauriTransport();
  await activeTransport.connect();
  localStorage.removeItem('atomic-server-config');
}
