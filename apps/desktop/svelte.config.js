import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/vite-plugin-svelte').SvelteConfig} */
export default {
  preprocess: vitePreprocess(),
  // Svelte 5 — runes mode is opt-in via .svelte file <script lang="ts" runes>.
  // Components in this project should use runes by default.
  compilerOptions: {
    runes: true,
  },
};
