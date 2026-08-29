/**
 * keeppix.v1 WebSocket client. The setup wizard stays on polling: it's a
 * page the user is actively watching, a one-time thing.
 */

export type LiveMessage = {
  v: number
  type: string
  payload?: unknown
}

export type LiveSocket = {
  close: () => void
}

const PROTOCOL = 'keeppix.v1'

export async function fetchTicket(): Promise<string> {
  const response = await fetch('/api/v1/ws/ticket', {
    method: 'POST',
    credentials: 'same-origin',
    headers: {
      'content-type': 'application/json',
      'x-keeppix-client': 'web'
    }
  })
  if (!response.ok) {
    throw new Error(`ws ticket ${response.status}`)
  }
  const body = (await response.json()) as { ticket: string }
  return body.ticket
}

function socketUrl(): string {
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${proto}//${location.host}/api/v1/ws`
}

function jittered(ms: number): number {
  return Math.round(ms * (0.5 + Math.random()))
}

/**
 * Reconnects on its own. Exponential backoff with jitter, capped at 30s.
 * `onEvent` receives every envelope, including `resync`.
 */
export function startLiveEvents(onEvent: (msg: LiveMessage) => void): LiveSocket {
  let closed = false
  let socket: WebSocket | null = null
  let delay = 1000
  let timer: ReturnType<typeof setTimeout> | undefined

  const connect = () => {
    if (closed) return
    void fetchTicket()
      .then((ticket) => {
        if (closed) return
        const ws = new WebSocket(socketUrl(), [PROTOCOL, `ticket.${ticket}`])
        socket = ws
        ws.onmessage = (ev) => {
          try {
            const msg = JSON.parse(String(ev.data)) as LiveMessage
            if (msg && typeof msg.type === 'string') onEvent(msg)
          } catch {
            /* non-JSON envelope: ignored */
          }
        }
        ws.onopen = () => {
          delay = 1000
        }
        ws.onclose = () => {
          socket = null
          if (closed) return
          timer = setTimeout(connect, jittered(delay))
          delay = Math.min(delay * 2, 30_000)
        }
      })
      .catch(() => {
        if (closed) return
        timer = setTimeout(connect, jittered(delay))
        delay = Math.min(delay * 2, 30_000)
      })
  }

  connect()
  return {
    close() {
      closed = true
      if (timer !== undefined) clearTimeout(timer)
      socket?.close()
      socket = null
    }
  }
}
