import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

// `aegis-server plugin dev <dir>` (task dev:plugin) serves the API on port 8091
const target = process.env.PLUGIN_DEV_URL ?? 'http://127.0.0.1:8091';

export default defineConfig({
  base: './',
  plugins: [react()],
  server: {
    port: 5175,
    // Keep the Host header: the dev server only accepts writes whose Origin is the same as Host, and
    // loopback names as Host. The string shorthand (`'/__dev': target`) sets `changeOrigin: true`,
    // which rewrites Host to the target and breaks that, so it is spelled out
    proxy: {
      '/__dev': { target, changeOrigin: false },
    },
  },
  test: {
    include: ['src/**/*.test.{ts,tsx}'],
    environment: 'node',
  },
});
