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
  },
  initialGlobals: { locale: 'en' },
  decorators: [
    (Story, { globals }) => {
      const locale = (globals.locale as string) || 'en';
      // Switch before rendering so the first paint (and play functions) use the chosen language
      if (i18n.language !== locale) void i18n.changeLanguage(locale);
      useEffect(() => {
        document.documentElement.lang = locale;
      }, [locale]);
      return <Story />;
    },
  ],
  parameters: {
    layout: 'fullscreen',
    backgrounds: { disable: true },
  },
};

export default preview;
