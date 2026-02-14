import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import path from 'path'

const isWebBuild = process.env.VITE_BUILD_TARGET === 'web'

export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
  ],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  resolve: isWebBuild
    ? {
        alias: {
          '@tauri-apps/api/core': path.resolve(__dirname, 'src/lib/stubs/tauri-core.ts'),
          '@tauri-apps/api/event': path.resolve(__dirname, 'src/lib/stubs/tauri-event.ts'),
          '@tauri-apps/plugin-dialog': path.resolve(__dirname, 'src/lib/stubs/tauri-dialog.ts'),
          '@tauri-apps/plugin-opener': path.resolve(__dirname, 'src/lib/stubs/tauri-opener.ts'),
        },
      }
    : undefined,
})
