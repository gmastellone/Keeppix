import type { Ref } from 'vue'

import { useUploadStore } from '@/stores/upload'

// Fase 11, sottosistema di caricamento — documento §3.2/§3.3 (comando
// "Carica" in topbar, "+" mobile), condiviso perché sono lo stesso
// meccanismo con due sole differenze visive (etichetta/icona), non due
// componenti indipendenti — evita una seconda copia divergente
// dell'input nascosto, scritta prima di avere entrambi i chiamanti
// pronti (stesso principio già applicato a `nav/routeTitles.ts`).
//
// Estensioni esatte dell'`accept` del prototipo (riga 1449 del mockup):
// niente RAW — è solo un suggerimento al selettore del sistema
// operativo, non applicato in modo uniforme dai browser, quindi
// `classifyFiles` (§4) resta comunque l'unica barriera vera dentro
// `addFilesFromPicker`.
export const UPLOAD_ACCEPT =
  '.jpg,.jpeg,.jpe,.png,.tif,.tiff,.webp,.heic,.heif,.mp4,.mov,.m4v,image/*,video/mp4,video/quicktime'

/**
 * `inputEl` è del chiamante, non di questo composable: deve restare un
 * `ref()` dichiarato localmente nel componente e legato con `ref="..."`
 * nel template — un `ref` restituito da un composable e solo
 * ridestrutturato non viene riconosciuto come "usato" dal controllo
 * `noUnusedLocals` di `vue-tsc` quando l'unico uso è quel legame di
 * template (falso positivo verificato scrivendo questo componente).
 */
export function useUploadPicker(inputEl: Ref<HTMLInputElement | null>) {
  const upload = useUploadStore()

  /** `destHint` (righe 2876-2883 del mockup): il contesto da cui si è
   * premuto il comando. Sempre `null` in questa app oggi — nessuna vista
   * porta un `currentFolder` osservabile (stesso debito già dichiarato
   * più volte per la timeline filtrata per cartella). */
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
