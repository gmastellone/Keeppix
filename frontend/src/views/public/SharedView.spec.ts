import { describe, expect, it } from 'vitest'

describe('SharedView', () => {
  it('does not import the session store', async () => {
    const source = await import.meta.glob('./SharedView.vue', {
      query: '?raw',
      import: 'default',
      eager: true
    })
    const code = Object.values(source)[0] as string
    expect(code).not.toContain('useSessionStore')
    expect(code).not.toContain('stores/session')
  })
})
