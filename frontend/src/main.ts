import { createPinia } from 'pinia'
import { createApp } from 'vue'

import App from './App.vue'
import { detectLocale, i18n } from './i18n'
import { router } from './router'
import './style.css'

document.documentElement.lang = detectLocale()

createApp(App).use(createPinia()).use(i18n).use(router).mount('#app')

// Service worker minimo per la Web Share Target API (Fase 5, Task 10):
// vedi `public/sw.js`. Registrato dopo il load per non competere con il
// caricamento iniziale della pagina; `serviceWorker` non esiste in tutti i
// browser (né in jsdom durante i test), quindi il controllo di feature è
// necessario, non decorativo.
if ('serviceWorker' in navigator) {
  window.addEventListener('load', () => {
    void navigator.serviceWorker.register('/sw.js')
  })
}
