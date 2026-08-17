// @ts-check
import { defineConfig } from 'astro/config';
import tailwindcss from '@tailwindcss/vite';

// Served from GitHub Pages at https://ben-burwood.github.io/anybucket/
export default defineConfig({
  site: 'https://ben-burwood.github.io',
  base: '/anybucket',
  vite: {
    plugins: [tailwindcss()],
    css: {
      postcss: {},
    },
  },
});
