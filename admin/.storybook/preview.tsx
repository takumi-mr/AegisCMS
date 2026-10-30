import type { Preview } from '@storybook/react-vite';
import { useEffect } from 'react';

import '../src/index.css';
import i18n, { LANGUAGES } from '../src/i18n';

const preview: Preview = {
  globalTypes: {
    locale: {
      description: 'Admin Studio language',
      toolbar: {
        title: 'Language',
        icon: 'globe',
        items: LANGUAGES.map((l) => ({ value: l.code, title: l.label })),
        dynamicTitle: true,
      },
    },
    theme: {
      description: 'Admin Studio color theme',
      toolbar: {
        title: 'Theme',
        icon: 'paintbrush',
        items: [
          { value: 'dark', title: 'Dark' },
          { value: 'light', title: 'Light' },
        ],
        dynamicTitle: true,
      },
    },
  },
  initialGlobals: { locale: 'en', theme: 'dark' },
  decorators: [
    (Story, { globals }) => {
      const locale = (globals.locale as string) || 'en';
      // Switch before rendering so the first paint (and play functions) use the chosen language
      if (i18n.language !== locale) void i18n.changeLanguage(locale);
      useEffect(() => {
        document.documentElement.lang = locale;
      }, [locale]);
      // The stories draw with the chosen theme (dark by default, as the tests were written for)
      const theme = (globals.theme as string) === 'light' ? 'light' : 'dark';
      document.documentElement.dataset.theme = theme;
      return <Story />;
    },
  ],
  parameters: {
    layout: 'fullscreen',
    backgrounds: { disable: true },
  },
};

export default preview;
