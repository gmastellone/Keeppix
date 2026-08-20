import { ApiProblem, apiFetch } from './client'

export interface CheckResponse {
  unknown_hashes: string[]
}

/** `POST /api/v1/upload/check` — corpo JSON, passa da `apiFetch` normale. */
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

/** `POST /api/v1/upload` — corpo JSON, passa da `apiFetch` normale. */
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
 * `HEAD /api/v1/upload/{id}` — metodo sicuro, non passa dal CSRF layer, ma
 * non è JSON: legge l'header `Upload-Offset`, non un corpo. Non può passare
 * da `apiFetch`, che pretende sempre `content-type: application/json` e
 * chiama `response.json()`.
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
 * `PATCH /api/v1/upload/{id}` — corpo `application/offset+octet-stream`,
 * bytes grezzi: non può passare da `apiFetch`, che forza
 * `content-type: application/json` e serializzerebbe il body. Il layer CSRF
 * copre comunque questa rotta (è sotto `/api/v1`), quindi l'header
 * `x-keeppix-client` va impostato a mano qui.
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
 * blake3 esadecimale calcolato lato client — via WebAssembly (`hash-wasm`),
 * l'unica libreria che lo offre in browser: `crypto.subtle` non implementa
 * blake3. Import dinamico perché il pannello di upload è un overlay globale
 * montato in `App.vue`: senza `import()` il binario wasm finirebbe nel
 * bundle iniziale anche per chi non carica mai un file.
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
