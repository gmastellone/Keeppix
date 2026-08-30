import { fileURLToPath, URL } from 'node:url'

import tailwindcss from '@tailwindcss/vite'
import vue from '@vitejs/plugin-vue'
// `vitest/config` instead of `vite`: that's what types the `test` key.
import { defineConfig } from 'vitest/config'

export default defineConfig({
  plugins: [vue(), tailwindcss()],
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) }
  },
  build: {
    // The 150 KB gzip budget is verified in CI; this warns earlier.
    chunkSizeWarningLimit: 400
  },
  server: {
    // In development the frontend runs on 5173 and proxies API calls to the backend.
    proxy: {
      '/api': { target: 'http://127.0.0.1:5673', changeOrigin: true },
      '/media': { target: 'http://127.0.0.1:5673', changeOrigin: true },
      '/health': { target: 'http://127.0.0.1:5673', changeOrigin: true }
    }
  },
  test: {
    environment: 'jsdom'
  }
})
