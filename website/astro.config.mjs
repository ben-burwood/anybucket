// @ts-check
import { defineConfig } from 'astro/config';
import tailwindcss from '@tailwindcss/vite';
import icon from 'astro-icon';

// Served from GitHub Pages at https://ben-burwood.github.io/anybucket/
export default defineConfig({
  site: 'https://ben-burwood.github.io',
  base: '/anybucket',
  integrations: [icon()],
  vite: {
    plugins: [tailwindcss()],
    css: {
      postcss: {},
    },
  },
});
