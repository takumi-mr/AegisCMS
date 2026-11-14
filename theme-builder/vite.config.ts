import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

// `aegis-server theme dev <dir>` (task dev:theme) serves the API, the sample pages and the theme assets
const target = process.env.THEME_DEV_URL ?? 'http://127.0.0.1:8090';

// https://vite.dev/config/
export default defineConfig({
  base: './',
  plugins: [react()],
  server: {
    port: 5174,
    // Keep the Host header (localhost:5174): the dev server only accepts loopback names, and a write
    // whose Origin is not the same as Host is refused. The string shorthand (`'/__dev': target`) sets
    // `changeOrigin: true`, which rewrites Host to the target and breaks that, so it is spelled out
    proxy: {
      '/__dev': { target, changeOrigin: false },
      '/theme': { target, changeOrigin: false },
      '/themes': { target, changeOrigin: false },
      '/preview': { target, changeOrigin: false },
    },
  },
  test: {
    include: ['src/**/*.test.{ts,tsx}'],
    environment: 'node',
  },
});
