// Stub for web builds
export function listen(): never {
  throw new Error('@tauri-apps/api/event is not available in web mode');
}

export function emit(): never {
  throw new Error('@tauri-apps/api/event is not available in web mode');
}
