/**
 * Errore RFC 9457. `type` è il codice stabile su cui ramificare.
 *
 * Campi assegnati nel corpo del costruttore, non come proprietà-parametro:
 * quella sintassi richiede codice emesso a runtime ed è vietata da
 * `erasableSyntaxOnly` nel tsconfig generato dal template attuale.
 */
export class ApiProblem extends Error {
  readonly type: string
  readonly title: string
  readonly status: number
  readonly detail?: string

  constructor(type: string, title: string, status: number, detail?: string) {
    super(`${type}: ${title}`)
    this.name = 'ApiProblem'
    this.type = type
    this.title = title
    this.status = status
    this.detail = detail
  }
}

/**
 * Chiamata JSON verso l'API. Invia sempre i cookie e un header custom.
 * L'header `x-keeppix-client` è la metà client-side della difesa CSRF
 * richiesta dalla spec (§9.5): un form HTML su un sito ostile non può
 * impostare header custom. Il backend **lo pretende** sulle mutazioni
 * (POST/PUT/PATCH/DELETE) e risponde `403 keeppix/csrf-check-failed` se
 * manca: vedi `crates/keeppix-api/src/csrf.rs`. Ogni chiamata all'API deve
 * quindi passare da qui — una `fetch` scritta a mano su una mutazione
 * verrebbe rifiutata.
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

/**
 * Una vista che carica dati al mount non può distinguere, da un `catch`
 * generico, una sessione scaduta da un guasto reale — e mostrare "errore
 * imprevisto" per un 401 manda l'utente a ricaricare invece che a rifare
 * login. Le view lo controllano esplicitamente e reindirizzano loro stesse:
 * vedi `TimelineView.signOut` per lo stesso pattern di redirect a `/login`.
 */
export function isUnauthenticated(error: unknown): boolean {
  return error instanceof ApiProblem && error.status === 401
}
