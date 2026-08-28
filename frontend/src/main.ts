import { createPinia } from 'pinia'
import { createApp } from 'vue'

import App from './App.vue'
import { detectLocale, i18n } from './i18n'
import { router } from './router'
import './style.css'

document.documentElement.lang = detectLocale()

createApp(App).use(createPinia()).use(i18n).use(router).mount('#app')

// Minimal service worker for the Web Share Target API: see `public/sw.js`.
// Registered after load to avoid competing with the initial page load;
// `serviceWorker` doesn't exist in every browser (nor in jsdom during
// tests), so the feature check is necessary, not decorative.
if ('serviceWorker' in navigator) {
  window.addEventListener('load', () => {
    void navigator.serviceWorker.register('/sw.js')
  })
}
