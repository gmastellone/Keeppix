import { describe, expect, it } from 'vitest'

import { parseSearch } from './parse'

describe('parseSearch', () => {
  it('parses and/or/not', () => {
    expect(parseSearch('grecia and (tramonto or mare)')).toEqual({
      op: 'and',
      args: [
        { op: 'text', value: 'grecia' },
        {
          op: 'or',
          args: [
            { op: 'text', value: 'tramonto' },
            { op: 'text', value: 'mare' }
          ]
        }
      ]
    })
    expect(parseSearch('not mare')).toEqual({
      op: 'not',
      arg: { op: 'text', value: 'mare' }
    })
  })

  it('parses typed fields', () => {
    expect(parseSearch('type:video and 2024')).toEqual({
      op: 'and',
      args: [
        { op: 'type', value: 'video' },
        { op: 'year', value: 2024 }
      ]
    })
    expect(parseSearch('camera:"Sony α7 IV" and iso:>3200')).toEqual({
      op: 'and',
      args: [
        { op: 'camera', value: 'Sony α7 IV' },
        { op: 'iso', cmp: 'gt', value: 3200 }
      ]
    })
    expect(parseSearch('has:gps folder:018f0000-0000-7000-8000-000000000001')).toEqual({
      op: 'and',
      args: [
        { op: 'has_gps' },
        { op: 'folder', id: '018f0000-0000-7000-8000-000000000001' }
      ]
    })
  })

  it('treats invalid iso and folder values as free text', () => {
    expect(parseSearch('iso:abc')).toEqual({ op: 'text', value: 'iso:abc' })
    expect(parseSearch('folder:not-a-uuid')).toEqual({
      op: 'text',
      value: 'folder:not-a-uuid'
    })
  })
})
