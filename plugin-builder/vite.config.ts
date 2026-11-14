import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

// `aegis-server plugin dev <dir>` (task dev:plugin) serves the API on port 8091
const target = process.env.PLUGIN_DEV_URL ?? 'http://127.0.0.1:8091';

export default defineConfig({
  base: './',
  plugins: [react()],
  server: {
    port: 5175,
    // Keep the Host header (do not set `changeOrigin`): the dev server only accepts writes whose
    // Origin is the same as Host, and loopback names as Host
    proxy: {
      '/__dev': target,
    },
  },
  test: {
    include: ['src/**/*.test.{ts,tsx}'],
    environment: 'node',
  },
});
