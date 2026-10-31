import type { StorybookConfig } from '@storybook/react-vite';

const config: StorybookConfig = {
  stories: ['../src/**/*.stories.@(ts|tsx)'],
  addons: ['@storybook/addon-vitest'],
  framework: '@storybook/react-vite',
  core: { disableTelemetry: true },
  // The app's vite.config.ts uses `base: './'` for the server; Storybook serves from its own root
  // `base`: see above. `fs.allow`: stories may read the files the server serves (crates/aegis-server/assets)
  viteFinal: (config) => ({
    ...config,
    base: '/',
    server: { ...config.server, fs: { ...config.server?.fs, allow: ['..'] } },
  }),
};

export default config;
