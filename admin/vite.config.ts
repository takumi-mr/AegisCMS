import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// https://vite.dev/config/
export default defineConfig({
  // Relative asset URLs (./assets/...), so the same build works under any `[server].base_path`
  base: './',
  plugins: [react()],
});
