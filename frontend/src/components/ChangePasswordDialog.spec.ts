import { mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { defineComponent, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { ApiProblem } from '@/api/client'
import { i18n } from '@/i18n'
import { useToastStore } from '@/stores/toast'

import ChangePasswordDialog from './ChangePasswordDialog.vue'

const changePasswordMock = vi.fn()

vi.mock('@/api/users', () => ({
  changePassword: (...args: unknown[]) => changePasswordMock(...args)
}))

const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

let wrapper: VueWrapper | undefined

function mountHost() {
  const pinia = createPinia()
  setActivePinia(pinia)
  const Host = defineComponent({
    components: { TheDialog: ChangePasswordDialog },
    setup() {
      const open = ref(true)
      return { open }
    },
    template: `<TheDialog v-model:open="open" />`
  })
  wrapper = mount(Host, { global: { plugins: [i18n, pinia] }, attachTo: document.body })
  return wrapper
}

function fillField(label: string, value: string) {
  const labelEl = Array.from(document.body.querySelectorAll('label')).find((l) => l.textContent === label)
  const input = labelEl?.parentElement?.querySelector('input')
  if (!input) throw new Error(`field not found: ${label}`)
  input.value = value
  input.dispatchEvent(new Event('input'))
}

beforeEach(() => {
  i18n.global.locale.value = 'it'
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
  vi.clearAllMocks()
})

describe('ChangePasswordDialog — §61.3 "Cambia password"', () => {
  it('rejects a mismatched confirmation without calling the API', async () => {
    changePasswordMock.mockResolvedValue(null)
    mountHost()
    await tick()

    fillField('Password attuale', 'currentpass123')
    fillField('Nuova password', 'newpassword123')
    fillField('Conferma nuova password', 'somethingelse123')
    document.body.querySelector('form')?.dispatchEvent(new Event('submit', { cancelable: true }))
    await tick()

    expect(changePasswordMock).not.toHaveBeenCalled()
    expect(document.body.textContent).toContain('Le due password non coincidono.')
  })

  it('rejects a too-short new password without calling the API', async () => {
    mountHost()
    await tick()

    fillField('Password attuale', 'currentpass123')
    fillField('Nuova password', 'short')
    fillField('Conferma nuova password', 'short')
    document.body.querySelector('form')?.dispatchEvent(new Event('submit', { cancelable: true }))
    await tick()

    expect(changePasswordMock).not.toHaveBeenCalled()
    expect(document.body.textContent).toContain('La password deve avere almeno 10 caratteri.')
  })

  it('on success, calls the real API, toasts, and closes', async () => {
    changePasswordMock.mockResolvedValue(null)
    const w = mountHost()
    await tick()

    fillField('Password attuale', 'currentpass123')
    fillField('Nuova password', 'newpassword123')
    fillField('Conferma nuova password', 'newpassword123')
    document.body.querySelector('form')?.dispatchEvent(new Event('submit', { cancelable: true }))
    await tick()
    await tick()

    expect(changePasswordMock).toHaveBeenCalledWith('currentpass123', 'newpassword123')
    const toast = useToastStore()
    expect(toast.toasts.some((t) => t.message === 'Password aggiornata.')).toBe(true)
    expect((w.vm as unknown as { open: boolean }).open).toBe(false)
  })

  it('shows a dedicated error when the current password is wrong (403)', async () => {
    changePasswordMock.mockRejectedValue(new ApiProblem('keeppix/forbidden', 'Forbidden', 403))
    mountHost()
    await tick()

    fillField('Password attuale', 'wrongpassword')
    fillField('Nuova password', 'newpassword123')
    fillField('Conferma nuova password', 'newpassword123')
    document.body.querySelector('form')?.dispatchEvent(new Event('submit', { cancelable: true }))
    await tick()
    await tick()

    expect(document.body.textContent).toContain('Password attuale errata.')
  })
})
