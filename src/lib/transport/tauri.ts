import type { Transport } from './types';

type TauriInvoke = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
type TauriListen = <T>(event: string, handler: (event: { payload: T }) => void) => Promise<() => void>;

export class TauriTransport implements Transport {
  readonly mode = 'tauri' as const;
  private invokeFn: TauriInvoke | null = null;
  private listenFn: TauriListen | null = null;

  async connect(): Promise<void> {
    const core = await import('@tauri-apps/api/core');
    const event = await import('@tauri-apps/api/event');
    this.invokeFn = core.invoke;
    this.listenFn = event.listen;
  }

  disconnect(): void {
    // no-op for Tauri
  }

  isConnected(): boolean {
    return this.invokeFn !== null;
  }

  async invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    if (!this.invokeFn) throw new Error('TauriTransport not connected');
    return this.invokeFn<T>(command, args);
  }

  subscribe<T>(event: string, callback: (payload: T) => void): () => void {
    if (!this.listenFn) throw new Error('TauriTransport not connected');
    const unlistenPromise = this.listenFn<T>(event, (e) => callback(e.payload));
    let unlisten: (() => void) | null = null;
    unlistenPromise.then((fn) => { unlisten = fn; });
    return () => {
      if (unlisten) {
        unlisten();
      } else {
        unlistenPromise.then((fn) => fn());
      }
    };
  }
}
