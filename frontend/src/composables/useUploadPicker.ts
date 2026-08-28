import type { Ref } from 'vue'

import { useUploadStore } from '@/stores/upload'

// The upload subsystem's picker — shared between the topbar "Upload"
// command and the mobile "+" button because they're the same mechanism
// with only two visual differences (label/icon), not two independent
// components — this avoids a second, diverging copy of the hidden input.
//
// The `accept` extension list matches the prototype exactly: no RAW — it's
// only a hint to the OS file picker, not applied consistently across
// browsers, so `classifyFiles` remains the real gatekeeper inside
// `addFilesFromPicker`.
export const UPLOAD_ACCEPT =
  '.jpg,.jpeg,.jpe,.png,.tif,.tiff,.webp,.heic,.heif,.mp4,.mov,.m4v,image/*,video/mp4,video/quicktime'

/**
 * `inputEl` belongs to the caller, not to this composable: it must stay a
 * `ref()` declared locally in the component and bound with `ref="..."` in
 * the template — a `ref` returned from a composable and merely
 * destructured is not recognized as "used" by `vue-tsc`'s `noUnusedLocals`
 * check when the only use is that template binding (verified false
 * positive while writing this component).
 */
export function useUploadPicker(inputEl: Ref<HTMLInputElement | null>) {
  const upload = useUploadStore()

  /** `destHint`: the context the upload command was triggered from. Always
   * `null` in this app today — no view exposes an observable
   * `currentFolder` yet. */
  function open(): void {
    inputEl.value?.click()
  }

  async function onChange(event: Event): Promise<void> {
    const input = event.target as HTMLInputElement
    const files = Array.from(input.files ?? [])
    input.value = ''
    if (files.length > 0) await upload.addFilesFromPicker(files)
  }

  return { open, onChange }
}
