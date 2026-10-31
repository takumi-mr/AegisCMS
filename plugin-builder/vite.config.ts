import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

// `aegis-server plugin dev <dir>` (task dev:plugin) serves the API on port 8091
const target = process.env.PLUGIN_DEV_URL ?? 'http://127.0.0.1:8091';

export default defineConfig({
  base: './',
  plugins: [react()],
  server: {
    port: 5175,
    proxy: {
      '/__dev': target,
    },
  },
  test: {
    include: ['src/**/*.test.{ts,tsx}'],
    environment: 'node',
  },
});
