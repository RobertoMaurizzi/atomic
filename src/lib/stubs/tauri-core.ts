// Stub for web builds — these should never be called directly
// (HttpTransport handles all communication)
export function invoke(): never {
  throw new Error('@tauri-apps/api/core is not available in web mode');
}
