// Typed translation keys: t('...') only accepts keys of the English (source) translation
import 'i18next';
import type en from '../locales/en/translation.json';

declare module 'i18next' {
  interface CustomTypeOptions {
    defaultNS: 'translation';
    resources: { translation: typeof en };
  }
}
