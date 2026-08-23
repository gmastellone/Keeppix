import { describe, expect, it } from 'vitest'

import { ApiProblem } from '@/api/client'

import { canRetry, classifyError } from './classify'

describe('classifyError', () => {
  it('maps service-unavailable (DbError::Connection) to unreachable', () => {
    expect(classifyError(new ApiProblem('service-unavailable', 'Service temporarily unavailable', 503))).toBe(
      'unreachable'
    )
  })

  it('maps forbidden to permission-denied', () => {
    expect(classifyError(new ApiProblem('forbidden', 'Not allowed', 403))).toBe('permission-denied')
  })

  it('maps not-found to file-missing', () => {
    expect(classifyError(new ApiProblem('not-found', 'Resource not found', 404))).toBe('file-missing')
  })

  it('an unrecognised Problem type falls back to unknown, not a guess', () => {
    expect(classifyError(new ApiProblem('internal-error', 'Unexpected server error', 500))).toBe('unknown')
  })

  it('a raw fetch() network failure (TypeError) is unreachable', () => {
    expect(classifyError(new TypeError('Failed to fetch'))).toBe('unreachable')
  })

  it('an aborted request (DOMException AbortError) is timeout', () => {
    expect(classifyError(new DOMException('The operation was aborted', 'AbortError'))).toBe('timeout')
  })

  it('anything else falls back to unknown', () => {
    expect(classifyError(new Error('boom'))).toBe('unknown')
    expect(classifyError('a plain string')).toBe('unknown')
    expect(classifyError(undefined)).toBe('unknown')
  })
})

describe('canRetry', () => {
  it('is true only for unreachable and permission-denied (spec fase-10-api-interfaccia.md §7)', () => {
    expect(canRetry('unreachable')).toBe(true)
    expect(canRetry('permission-denied')).toBe(true)
    expect(canRetry('file-missing')).toBe(false)
    expect(canRetry('timeout')).toBe(false)
    expect(canRetry('unknown')).toBe(false)
  })
})
