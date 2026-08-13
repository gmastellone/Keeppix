// Config flat di ESLint 10. Non prescritta dal piano ma necessaria perché
// `npm run lint` (richiesto dalla verifica finale) abbia un config da usare:
// il template `vue-ts` di create-vite non ne genera uno.
import { defineConfigWithVueTs, vueTsConfigs } from '@vue/eslint-config-typescript'
import pluginVue from 'eslint-plugin-vue'

export default defineConfigWithVueTs(
  { ignores: ['dist/**'] },
  pluginVue.configs['flat/recommended'],
  vueTsConfigs.recommended,
  {
    // I componenti base in components/ui/ hanno nomi a una parola per
    // convenzione (Button, Alert): sono primitivi, non pagine, e il piano
    // li chiama così verbatim.
    files: ['src/components/ui/**/*.vue'],
    rules: { 'vue/multi-word-component-names': 'off' }
  }
)
