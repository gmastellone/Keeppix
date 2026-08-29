import { ApiProblem, apiFetch } from './client'

export interface CheckResponse {
  unknown_hashes: string[]
}

/** `POST /api/v1/upload/check` — JSON body, goes through the regular `apiFetch`. */
export async function checkHashes(hashes: string[]): Promise<CheckResponse> {
  return apiFetch<CheckResponse>('/api/v1/upload/check', {
    method: 'POST',
    body: JSON.stringify({ hashes })
  })
}

export interface CreateSessionRequest {
  target_folder_id: string
  filename: string
  expected_size: number
  expected_hash?: string
  client_mtime?: string
}

export interface CreateSessionResponse {
  id: string
}

/** `POST /api/v1/upload` — JSON body, goes through the regular `apiFetch`. */
export async function createSession(
  req: CreateSessionRequest
): Promise<CreateSessionResponse> {
  return apiFetch<CreateSessionResponse>('/api/v1/upload', {
    method: 'POST',
    body: JSON.stringify(req)
  })
}

export type HeadResult = { kind: 'ok'; receivedBytes: number } | { kind: 'gone' }

/**
 * `HEAD /api/v1/upload/{id}` — a safe method, doesn't go through the CSRF
 * layer, but isn't JSON: it reads the `Upload-Offset` header, not a
 * body. Can't go through `apiFetch`, which always forces
 * `content-type: application/json` and calls `response.json()`.
 */
export async function headSession(id: string): Promise<HeadResult> {
  const response = await fetch(`/api/v1/upload/${id}`, {
    method: 'HEAD',
    credentials: 'same-origin'
  })

  if (response.status === 410) {
    return { kind: 'gone' }
  }

  if (!response.ok) {
    throw new ApiProblem('keeppix/unexpected', response.statusText, response.status)
  }

  const offset = Number(response.headers.get('upload-offset') ?? '0')
  return { kind: 'ok', receivedBytes: offset }
}

export type PatchChunkResult =
  | { status: 'chunk-accepted'; receivedBytes: number }
  | {
      status: 'finalized'
      asset_id: string
      filename: string
      collision: 'created' | 'skipped_duplicate' | 'renamed'
      existing_asset_id?: string
    }

/**
 * `PATCH /api/v1/upload/{id}` — `application/offset+octet-stream` body,
 * raw bytes: can't go through `apiFetch`, which forces
 * `content-type: application/json` and would serialize the body. The
 * CSRF layer still covers this route (it's under `/api/v1`), so the
 * `x-keeppix-client` header has to be set manually here.
 */
export async function patchChunk(
  id: string,
  offset: number,
  checksumHex: string,
  chunk: Blob
): Promise<PatchChunkResult> {
  const response = await fetch(`/api/v1/upload/${id}`, {
    method: 'PATCH',
    credentials: 'same-origin',
    headers: {
      'content-type': 'application/offset+octet-stream',
      'x-keeppix-client': 'web',
      'upload-offset': String(offset),
      'upload-checksum': `blake3 ${checksumHex}`
    },
    body: chunk
  })

  if (response.status === 204) {
    const nextOffset = Number(response.headers.get('upload-offset') ?? String(offset))
    return { status: 'chunk-accepted', receivedBytes: nextOffset }
  }

  if (response.status === 201) {
    const body = (await response.json()) as Omit<
      Extract<PatchChunkResult, { status: 'finalized' }>,
      'status'
    >
    return { status: 'finalized', ...body }
  }

  const contentType = response.headers.get('content-type') ?? ''
  if (contentType.includes('application/problem+json')) {
    const problem = await response.json()
    throw new ApiProblem(problem.type, problem.title, problem.status, problem.detail)
  }
  throw new ApiProblem('keeppix/unexpected', response.statusText, response.status)
}

/**
 * Client-side hex blake3 — via WebAssembly (`hash-wasm`), the only
 * library that offers it in the browser: `crypto.subtle` doesn't
 * implement blake3. Dynamic import because the upload panel is a global
 * overlay mounted in `App.vue`: without `import()` the wasm binary would
 * end up in the initial bundle even for someone who never uploads a
 * file.
 */
export async function hashBytes(data: ArrayBuffer | Uint8Array): Promise<string> {
  const { blake3 } = await import('hash-wasm')
  const bytes = data instanceof Uint8Array ? data : new Uint8Array(data)
  return blake3(bytes)
}

export async function hashFile(file: Blob): Promise<string> {
  const buffer = await file.arrayBuffer()
  return hashBytes(buffer)
}
