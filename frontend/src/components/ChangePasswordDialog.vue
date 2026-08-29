<script setup lang="ts">
// Unlike the mockup (where "Change password" isn't wired to anything),
// here the form does something real: `POST /users/me/password`
// (`api/users.ts::changePassword`, already written but never called from
// the frontend before now). Built on `Dialog.vue`, not `ConfirmDialog`: it
// needs a three-field form, not a yes/no confirmation.
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { ApiProblem } from '@/api/client'
import { changePassword } from '@/api/users'
import Dialog from '@/components/ui/Dialog.vue'
import TextField from '@/components/ui/TextField.vue'
import { useToastStore } from '@/stores/toast'

const open = defineModel<boolean>('open', { required: true })
const { t } = useI18n()
const toast = useToastStore()

const current = ref('')
const next = ref('')
const confirm = ref('')
const error = ref('')
const saving = ref(false)

function reset() {
  current.value = ''
  next.value = ''
  confirm.value = ''
  error.value = ''
}

function close() {
  open.value = false
}

watch(open, (isOpen) => {
  if (!isOpen) reset()
})

async function save() {
  error.value = ''
  if (next.value !== confirm.value) {
    error.value = t('profile.passwordDialog.mismatch')
    return
  }
  if (next.value.length < 10) {
    error.value = t('profile.passwordDialog.tooShort')
    return
  }
  saving.value = true
  try {
    await changePassword(current.value, next.value)
    toast.show(t('profile.passwordDialog.done'))
    close()
  } catch (err) {
    if (err instanceof ApiProblem && err.status === 403) {
      error.value = t('profile.passwordDialog.wrongCurrent')
    } else if (err instanceof ApiProblem && err.status === 422) {
      error.value = t('profile.passwordDialog.tooShort')
    } else {
      error.value = t('profile.sessions.actionError')
    }
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <Dialog
    v-model:open="open"
    :title="t('profile.passwordDialog.title')"
  >
    <form
      class="flex flex-col gap-3"
      @submit.prevent="save"
    >
      <TextField
        v-model="current"
        :label="t('profile.passwordDialog.current')"
        type="password"
        autocomplete="current-password"
        required
      />
      <TextField
        v-model="next"
        :label="t('profile.passwordDialog.new')"
        type="password"
        autocomplete="new-password"
        :hint="t('profile.passwordDialog.hint')"
        required
      />
      <TextField
        v-model="confirm"
        :label="t('profile.passwordDialog.confirm')"
        type="password"
        autocomplete="new-password"
        required
      />
      <p
        v-if="error"
        class="text-sm text-danger"
      >
        {{ error }}
      </p>
      <button
        type="submit"
        class="mt-1 rounded-lg bg-accent px-3.5 py-2 text-[13px] font-semibold text-white disabled:opacity-60"
        :disabled="saving"
      >
        {{ t('profile.passwordDialog.save') }}
      </button>
    </form>
  </Dialog>
</template>
