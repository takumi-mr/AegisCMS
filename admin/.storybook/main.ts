import type { StorybookConfig } from '@storybook/react-vite';

const config: StorybookConfig = {
  stories: ['../src/**/*.stories.@(ts|tsx)'],
  framework: '@storybook/react-vite',
  core: { disableTelemetry: true },
  // The app's vite.config.ts uses `base: './'` for the server; Storybook serves from its own root
  viteFinal: (config) => ({ ...config, base: '/' }),
};

export default config;
