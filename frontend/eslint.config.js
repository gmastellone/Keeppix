// ESLint 10 flat config. Needed so `npm run lint` has a config to use:
// the `vue-ts` create-vite template doesn't generate one.
import { defineConfigWithVueTs, vueTsConfigs } from '@vue/eslint-config-typescript'
import pluginVue from 'eslint-plugin-vue'

export default defineConfigWithVueTs(
  { ignores: ['dist/**'] },
  pluginVue.configs['flat/recommended'],
  vueTsConfigs.recommended,
  {
    // Base components in components/ui/ have single-word names by
    // convention (Button, Alert): they're primitives, not pages.
    files: ['src/components/ui/**/*.vue'],
    rules: { 'vue/multi-word-component-names': 'off' }
  }
)
