import { createPinia } from 'pinia'
import { createApp } from 'vue'

import App from './App.vue'
import { detectLocale, i18n } from './i18n'
import { router } from './router'
import './style.css'

document.documentElement.lang = detectLocale()

createApp(App).use(createPinia()).use(i18n).use(router).mount('#app')
