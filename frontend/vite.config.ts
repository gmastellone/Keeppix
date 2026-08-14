import { fileURLToPath, URL } from 'node:url'

import tailwindcss from '@tailwindcss/vite'
import vue from '@vitejs/plugin-vue'
// `vitest/config` invece di `vite`: è ciò che rende tipizzata la chiave `test`.
import { defineConfig } from 'vitest/config'

export default defineConfig({
  plugins: [vue(), tailwindcss()],
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) }
  },
  build: {
    // Il budget di 150 KB gzip è verificato in CI; qui si avvisa prima.
    chunkSizeWarningLimit: 400
  },
  server: {
    // In sviluppo il frontend gira su 5173 e inoltra le API al backend.
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
