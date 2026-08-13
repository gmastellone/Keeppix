## Task 12: Frontend

**Files:**
- Create: `frontend/package.json`, `frontend/vite.config.ts`, `frontend/tsconfig.json`, `frontend/index.html`
- Create: `frontend/src/main.ts`, `frontend/src/App.vue`, `frontend/src/style.css`, `frontend/src/router.ts`
- Create: `frontend/src/api/client.ts`, `frontend/src/api/auth.ts`
- Create: `frontend/src/i18n/index.ts`, `frontend/src/i18n/it.json`, `frontend/src/i18n/en.json`
- Create: `frontend/src/stores/session.ts`
- Create: `frontend/src/components/ui/Button.vue`, `TextField.vue`, `Alert.vue`
- Create: `frontend/src/views/SetupView.vue`, `LoginView.vue`, `HomeView.vue`
- Create: `frontend/src/api/client.spec.ts`, `frontend/src/i18n/i18n.spec.ts`

**Interfaces:**
- Consumes: gli endpoint del Task 10.
- Produces:
  - `apiFetch<T>(path: string, init?: RequestInit): Promise<T>` — lancia `ApiProblem { type, title, status, detail? }` sugli errori.
  - `useSessionStore()` (Pinia) con `user`, `initialised`, `bootstrap()`, `login(username, password)`, `setup(payload)`, `logout()`.
  - Rotte: `/setup`, `/login`, `/` (protetta).

- [ ] **Step 1: Creare il progetto e installare le dipendenze**

```bash
cd Keeppix
npm create vite@latest frontend -- --template vue-ts
cd frontend
npm install
npm install vue-router pinia vue-i18n@11 @intlify/core-base reka-ui
npm install -D tailwindcss @tailwindcss/vite vitest @vue/test-utils jsdom \
  eslint eslint-plugin-vue @vue/eslint-config-typescript vue-tsc
```

- [ ] **Step 2: Configurare Vite**

`frontend/vite.config.ts`:

```ts
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
      '/health': { target: 'http://127.0.0.1:5673', changeOrigin: true }
    }
  },
  test: {
    environment: 'jsdom'
  }
})
```

- [ ] **Step 3: Configurare Tailwind v4**

`frontend/src/style.css`:

```css
@import "tailwindcss";

@theme {
  --color-surface: oklch(99% 0 0);
  --color-surface-elevated: oklch(100% 0 0);
  --color-content: oklch(20% 0 0);
  --color-content-muted: oklch(50% 0 0);
  --color-accent: oklch(58% 0.19 258);
  --color-danger: oklch(55% 0.20 25);
  --color-border: oklch(90% 0 0);
}

@media (prefers-color-scheme: dark) {
  @theme {
    --color-surface: oklch(17% 0 0);
    --color-surface-elevated: oklch(22% 0 0);
    --color-content: oklch(95% 0 0);
    --color-content-muted: oklch(65% 0 0);
    --color-border: oklch(30% 0 0);
  }
}

html, body, #app { height: 100%; }
body { background: var(--color-surface); color: var(--color-content); }

/* Rispetta chi ha ridotto le animazioni a livello di sistema. */
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: 0.01ms !important;
    transition-duration: 0.01ms !important;
  }
}
```

- [ ] **Step 4: Scrivere i test che falliscono**

`frontend/src/api/client.spec.ts`:

```ts
import { afterEach, describe, expect, it, vi } from 'vitest'

import { ApiProblem, apiFetch } from './client'

afterEach(() => vi.unstubAllGlobals())

function mockResponse(status: number, body: unknown, contentType: string) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () =>
      new Response(JSON.stringify(body), {
        status,
        headers: { 'content-type': contentType }
      })
    )
  )
}

describe('apiFetch', () => {
  it('restituisce il corpo su risposta positiva', async () => {
    mockResponse(200, { user: { username: 'giovanni' } }, 'application/json')
    await expect(apiFetch('/api/v1/auth/me')).resolves.toEqual({
      user: { username: 'giovanni' }
    })
  })

  it('lancia ApiProblem con il codice stabile', async () => {
    mockResponse(
      401,
      { type: 'keeppix/invalid-credentials', title: 'Invalid credentials', status: 401 },
      'application/problem+json'
    )

    await expect(apiFetch('/api/v1/auth/login')).rejects.toMatchObject({
      type: 'keeppix/invalid-credentials',
      status: 401
    })
  })

  it('lancia ApiProblem generico se il corpo non è problem+json', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('boom', { status: 502 })))

    const error = await apiFetch('/api/v1/auth/me').catch((e: unknown) => e)
    expect(error).toBeInstanceOf(ApiProblem)
    expect((error as ApiProblem).status).toBe(502)
  })

  it('restituisce null su 204', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(null, { status: 204 })))
    await expect(apiFetch('/api/v1/auth/refresh')).resolves.toBeNull()
  })
})
```

`frontend/src/i18n/i18n.spec.ts`:

```ts
import { describe, expect, it } from 'vitest'

import en from './en.json'
import it from './it.json'

/// Appiattisce un oggetto annidato in un elenco di chiavi puntate.
function keys(obj: Record<string, unknown>, prefix = ''): string[] {
  return Object.entries(obj).flatMap(([k, v]) =>
    typeof v === 'object' && v !== null
      ? keys(v as Record<string, unknown>, `${prefix}${k}.`)
      : [`${prefix}${k}`]
  )
}

describe('traduzioni', () => {
  it('italiano e inglese hanno le stesse chiavi', () => {
    const itKeys = keys(it).sort()
    const enKeys = keys(en).sort()
    expect(itKeys).toEqual(enKeys)
  })

  it('nessuna traduzione è vuota', () => {
    for (const [locale, messages] of [['it', it], ['en', en]] as const) {
      for (const key of keys(messages)) {
        const value = key.split('.').reduce<unknown>(
          (acc, part) => (acc as Record<string, unknown>)[part],
          messages
        )
        expect(value, `${locale}.${key}`).not.toBe('')
      }
    }
  })
})
```

- [ ] **Step 5: Eseguire e verificare il fallimento**

Run: `cd frontend && npx vitest run`
Expected: FAIL — `Cannot find module './client'`.

- [ ] **Step 6: Implementare `src/api/client.ts`**

```ts
/** Errore RFC 9457. `type` è il codice stabile su cui ramificare. */
export class ApiProblem extends Error {
  constructor(
    readonly type: string,
    readonly title: string,
    readonly status: number,
    readonly detail?: string
  ) {
    super(`${type}: ${title}`)
    this.name = 'ApiProblem'
  }
}

/**
 * Chiamata JSON verso l'API. Invia sempre i cookie e l'header custom che
 * il backend richiede sulle mutazioni: un form HTML esterno non può produrlo,
 * quindi copre la protezione CSRF insieme a SameSite=Lax.
 */
export async function apiFetch<T = unknown>(
  path: string,
  init: RequestInit = {}
): Promise<T> {
  const response = await fetch(path, {
    ...init,
    credentials: 'same-origin',
    headers: {
      'content-type': 'application/json',
      'x-keeppix-client': 'web',
      ...(init.headers ?? {})
    }
  })

  if (response.status === 204) {
    return null as T
  }

  if (!response.ok) {
    const contentType = response.headers.get('content-type') ?? ''
    if (contentType.includes('application/problem+json')) {
      const problem = await response.json()
      throw new ApiProblem(problem.type, problem.title, problem.status, problem.detail)
    }
    throw new ApiProblem('keeppix/unexpected', response.statusText, response.status)
  }

  return (await response.json()) as T
}
```

- [ ] **Step 7: Implementare le traduzioni**

`frontend/src/i18n/it.json`:

```json
{
  "app": { "name": "Keeppix" },
  "setup": {
    "title": "Benvenuto in Keeppix",
    "subtitle": "Crea l'account amministratore per iniziare.",
    "displayName": "Nome",
    "username": "Nome utente",
    "email": "Email (facoltativa)",
    "password": "Password",
    "passwordHint": "Almeno 10 caratteri.",
    "submit": "Crea account",
    "errors": {
      "invalidUsername": "Nome utente non valido: usa da 3 a 32 caratteri fra lettere, numeri, punto, trattino e trattino basso.",
      "invalidPassword": "La password deve avere almeno 10 caratteri.",
      "alreadyInitialised": "Questa istanza è già configurata."
    }
  },
  "login": {
    "title": "Accedi",
    "username": "Nome utente",
    "password": "Password",
    "submit": "Accedi",
    "errors": { "invalidCredentials": "Nome utente o password non corretti." }
  },
  "home": { "greeting": "Ciao, {name}", "logout": "Esci" },
  "common": { "loading": "Caricamento…", "unexpectedError": "Si è verificato un errore imprevisto." }
}
```

`frontend/src/i18n/en.json`:

```json
{
  "app": { "name": "Keeppix" },
  "setup": {
    "title": "Welcome to Keeppix",
    "subtitle": "Create the administrator account to get started.",
    "displayName": "Name",
    "username": "Username",
    "email": "Email (optional)",
    "password": "Password",
    "passwordHint": "At least 10 characters.",
    "submit": "Create account",
    "errors": {
      "invalidUsername": "Invalid username: use 3 to 32 characters from letters, digits, dot, hyphen and underscore.",
      "invalidPassword": "The password must be at least 10 characters long.",
      "alreadyInitialised": "This instance is already set up."
    }
  },
  "login": {
    "title": "Sign in",
    "username": "Username",
    "password": "Password",
    "submit": "Sign in",
    "errors": { "invalidCredentials": "Incorrect username or password." }
  },
  "home": { "greeting": "Hello, {name}", "logout": "Sign out" },
  "common": { "loading": "Loading…", "unexpectedError": "An unexpected error occurred." }
}
```

`frontend/src/i18n/index.ts`:

```ts
import { createI18n } from 'vue-i18n'

import en from './en.json'
import it from './it.json'

const SUPPORTED = ['it', 'en'] as const
export type Locale = (typeof SUPPORTED)[number]

const STORAGE_KEY = 'keeppix.locale'

/** Nessuna lingua predefinita: si rileva, poi vince la scelta esplicita. */
export function detectLocale(): Locale {
  const stored = localStorage.getItem(STORAGE_KEY)
  if (stored && SUPPORTED.includes(stored as Locale)) {
    return stored as Locale
  }
  const preferred = navigator.languages ?? [navigator.language]
  for (const tag of preferred) {
    const base = tag.split('-')[0]
    if (SUPPORTED.includes(base as Locale)) {
      return base as Locale
    }
  }
  return 'en'
}

export function setLocale(locale: Locale): void {
  localStorage.setItem(STORAGE_KEY, locale)
  i18n.global.locale.value = locale
  document.documentElement.lang = locale
}

export const i18n = createI18n({
  legacy: false,
  locale: detectLocale(),
  fallbackLocale: 'en',
  messages: { it, en }
})
```

- [ ] **Step 8: Eseguire i test del frontend**

Run: `cd frontend && npx vitest run`
Expected: PASS — 6 test.

- [ ] **Step 9: Implementare lo store di sessione**

`frontend/src/stores/session.ts`:

```ts
import { defineStore } from 'pinia'
import { ref } from 'vue'

import { ApiProblem, apiFetch } from '@/api/client'

export interface User {
  id: string
  username: string
  display_name: string
  email: string | null
  role: 'admin' | 'user'
  locale: string | null
}

export const useSessionStore = defineStore('session', () => {
  const user = ref<User | null>(null)
  const initialised = ref<boolean | null>(null)
  const ready = ref(false)

  /** Determina lo stato dell'istanza e ripristina la sessione se presente. */
  async function bootstrap(): Promise<void> {
    const status = await apiFetch<{ initialised: boolean }>('/api/v1/setup/status')
    initialised.value = status.initialised

    if (status.initialised) {
      try {
        const me = await apiFetch<{ user: User }>('/api/v1/auth/me')
        user.value = me.user
      } catch (error) {
        // 401 è normale: nessuna sessione attiva.
        if (!(error instanceof ApiProblem) || error.status !== 401) throw error
        user.value = null
      }
    }
    ready.value = true
  }

  async function login(username: string, password: string): Promise<void> {
    const result = await apiFetch<{ user: User }>('/api/v1/auth/login', {
      method: 'POST',
      body: JSON.stringify({ username, password })
    })
    user.value = result.user
  }

  async function setup(payload: {
    username: string
    display_name: string
    email?: string
    password: string
  }): Promise<void> {
    const result = await apiFetch<{ user: User }>('/api/v1/setup', {
      method: 'POST',
      body: JSON.stringify(payload)
    })
    user.value = result.user
    initialised.value = true
  }

  async function logout(): Promise<void> {
    await apiFetch('/api/v1/auth/logout', { method: 'POST' })
    user.value = null
  }

  return { user, initialised, ready, bootstrap, login, setup, logout }
})
```

- [ ] **Step 10: Implementare i componenti UI**

`frontend/src/components/ui/Button.vue`:

```vue
<script setup lang="ts">
defineProps<{ type?: 'button' | 'submit'; disabled?: boolean; loading?: boolean }>()
</script>

<template>
  <button
    :type="type ?? 'button'"
    :disabled="disabled || loading"
    class="w-full rounded-lg bg-accent px-4 py-2.5 font-medium text-white
           transition-opacity hover:opacity-90 focus-visible:outline-2
           focus-visible:outline-offset-2 focus-visible:outline-accent
           disabled:opacity-50"
  >
    <slot />
  </button>
</template>
```

`frontend/src/components/ui/TextField.vue`:

```vue
<script setup lang="ts">
import { useId } from 'vue'

defineProps<{ label: string; type?: string; hint?: string; autocomplete?: string; required?: boolean }>()
const model = defineModel<string>({ required: true })
const id = useId()
</script>

<template>
  <div class="flex flex-col gap-1.5">
    <label :for="id" class="text-sm font-medium text-content">{{ label }}</label>
    <input
      :id="id"
      v-model="model"
      :type="type ?? 'text'"
      :autocomplete="autocomplete"
      :required="required"
      :aria-describedby="hint ? `${id}-hint` : undefined"
      class="rounded-lg border border-border bg-surface-elevated px-3 py-2.5
             text-content focus-visible:outline-2 focus-visible:outline-accent"
    />
    <p v-if="hint" :id="`${id}-hint`" class="text-xs text-content-muted">{{ hint }}</p>
  </div>
</template>
```

`frontend/src/components/ui/Alert.vue`:

```vue
<script setup lang="ts">
defineProps<{ message: string }>()
</script>

<template>
  <p role="alert" class="rounded-lg bg-danger/10 px-3 py-2 text-sm text-danger">
    {{ message }}
  </p>
</template>
```

- [ ] **Step 11: Implementare le viste**

`frontend/src/views/LoginView.vue`:

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { ApiProblem } from '@/api/client'
import Alert from '@/components/ui/Alert.vue'
import Button from '@/components/ui/Button.vue'
import TextField from '@/components/ui/TextField.vue'
import { useSessionStore } from '@/stores/session'

const { t } = useI18n()
const router = useRouter()
const session = useSessionStore()

const username = ref('')
const password = ref('')
const error = ref('')
const loading = ref(false)

async function submit() {
  error.value = ''
  loading.value = true
  try {
    await session.login(username.value, password.value)
    await router.push('/')
  } catch (e) {
    error.value =
      e instanceof ApiProblem && e.type === 'keeppix/invalid-credentials'
        ? t('login.errors.invalidCredentials')
        : t('common.unexpectedError')
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <main class="mx-auto flex min-h-full w-full max-w-sm flex-col justify-center gap-6 p-6">
    <h1 class="text-2xl font-semibold">{{ t('login.title') }}</h1>
    <form class="flex flex-col gap-4" @submit.prevent="submit">
      <TextField v-model="username" :label="t('login.username')" autocomplete="username" required />
      <TextField
        v-model="password"
        :label="t('login.password')"
        type="password"
        autocomplete="current-password"
        required
      />
      <Alert v-if="error" :message="error" />
      <Button type="submit" :loading="loading">{{ t('login.submit') }}</Button>
    </form>
  </main>
</template>
```

`frontend/src/views/SetupView.vue`:

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { ApiProblem } from '@/api/client'
import Alert from '@/components/ui/Alert.vue'
import Button from '@/components/ui/Button.vue'
import TextField from '@/components/ui/TextField.vue'
import { useSessionStore } from '@/stores/session'

const { t } = useI18n()
const router = useRouter()
const session = useSessionStore()

const displayName = ref('')
const username = ref('')
const email = ref('')
const password = ref('')
const error = ref('')
const loading = ref(false)

/** Il backend restituisce codici stabili: la traduzione avviene qui. */
function messageFor(e: unknown): string {
  if (!(e instanceof ApiProblem)) return t('common.unexpectedError')
  const known: Record<string, string> = {
    'keeppix/invalid-username': t('setup.errors.invalidUsername'),
    'keeppix/invalid-password': t('setup.errors.invalidPassword'),
    'keeppix/already-initialised': t('setup.errors.alreadyInitialised')
  }
  return known[e.type] ?? t('common.unexpectedError')
}

async function submit() {
  error.value = ''
  loading.value = true
  try {
    await session.setup({
      username: username.value,
      display_name: displayName.value,
      email: email.value || undefined,
      password: password.value
    })
    await router.push('/')
  } catch (e) {
    error.value = messageFor(e)
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <main class="mx-auto flex min-h-full w-full max-w-sm flex-col justify-center gap-6 p-6">
    <header class="flex flex-col gap-1">
      <h1 class="text-2xl font-semibold">{{ t('setup.title') }}</h1>
      <p class="text-sm text-content-muted">{{ t('setup.subtitle') }}</p>
    </header>

    <form class="flex flex-col gap-4" @submit.prevent="submit">
      <TextField v-model="displayName" :label="t('setup.displayName')" autocomplete="name" required />
      <TextField v-model="username" :label="t('setup.username')" autocomplete="username" required />
      <TextField v-model="email" :label="t('setup.email')" type="email" autocomplete="email" />
      <TextField
        v-model="password"
        :label="t('setup.password')"
        :hint="t('setup.passwordHint')"
        type="password"
        autocomplete="new-password"
        required
      />
      <Alert v-if="error" :message="error" />
      <Button type="submit" :loading="loading">{{ t('setup.submit') }}</Button>
    </form>
  </main>
</template>
```

`frontend/src/views/HomeView.vue`:

```vue
<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import Button from '@/components/ui/Button.vue'
import { useSessionStore } from '@/stores/session'

const { t } = useI18n()
const router = useRouter()
const session = useSessionStore()

async function signOut() {
  await session.logout()
  await router.push('/login')
}
</script>

<template>
  <main class="mx-auto flex min-h-full w-full max-w-2xl flex-col gap-6 p-6">
    <h1 class="text-2xl font-semibold">
      {{ t('home.greeting', { name: session.user?.display_name ?? '' }) }}
    </h1>
    <div class="max-w-xs">
      <Button @click="signOut">{{ t('home.logout') }}</Button>
    </div>
  </main>
</template>
```

- [ ] **Step 12: Implementare router e bootstrap**

`frontend/src/router.ts`:

```ts
import { createRouter, createWebHistory } from 'vue-router'

import { useSessionStore } from '@/stores/session'

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', component: () => import('@/views/HomeView.vue'), meta: { auth: true } },
    { path: '/login', component: () => import('@/views/LoginView.vue') },
    { path: '/setup', component: () => import('@/views/SetupView.vue') },
    { path: '/:pathMatch(.*)*', redirect: '/' }
  ]
})

router.beforeEach(async (to) => {
  const session = useSessionStore()
  if (!session.ready) {
    await session.bootstrap()
  }

  // Istanza vergine: qualsiasi percorso porta al setup.
  if (session.initialised === false) {
    return to.path === '/setup' ? true : '/setup'
  }
  if (to.path === '/setup') {
    return '/'
  }
  if (to.meta.auth && !session.user) {
    return '/login'
  }
  if (to.path === '/login' && session.user) {
    return '/'
  }
  return true
})
```

`frontend/src/App.vue`:

```vue
<template>
  <RouterView />
</template>
```

`frontend/src/main.ts`:

```ts
import { createPinia } from 'pinia'
import { createApp } from 'vue'

import App from './App.vue'
import { i18n, detectLocale } from './i18n'
import { router } from './router'
import './style.css'

document.documentElement.lang = detectLocale()

createApp(App).use(createPinia()).use(i18n).use(router).mount('#app')
```

- [ ] **Step 13: Verificare tipi, lint, test e build**

Run: `cd frontend && npx vue-tsc --noEmit && npx vitest run && npm run build`
Expected: nessun errore di tipo, 6 test verdi, build completata.

- [ ] **Step 14: Verificare il budget di bundle**

```bash
cd frontend && find dist/assets -name '*.js' -exec gzip -c {} \; | wc -c
```

Expected: sotto **153600** byte. Se sforato, spostare le viste in import dinamici (già fatto nel router) e verificare che Reka UI non sia importato interamente.

- [ ] **Step 15: Provare il flusso completo a mano**

Con il backend in esecuzione (Task 9, Step 10) e `npm run dev` nel frontend, aprire `http://127.0.0.1:5173`:
1. Si viene rediretti a `/setup`.
2. Creare l'admin → si arriva a `/` con il saluto.
3. Ricaricare → si resta autenticati.
4. Uscire → si torna a `/login`.
5. Rientrare con le credenziali corrette.

- [ ] **Step 16: Commit**

```bash
git add frontend
git commit -m "feat(frontend): add vue app with setup, login and i18n"
```

---

