import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import en from '../locales/en/translation.json';
import ja from '../locales/ja/translation.json';

export const LANGUAGES = [
  { code: 'en', label: 'English' },
  { code: 'ja', label: '日本語' },
] as const;

export type Language = (typeof LANGUAGES)[number]['code'];

const STORAGE_KEY = 'aegis.plugin-builder.language';

const isLanguage = (value: string | null | undefined): value is Language =>
  LANGUAGES.some((l) => l.code === value);

function detectLanguage(): Language {
  try {
    if (typeof localStorage !== 'undefined') {
      const stored = localStorage.getItem(STORAGE_KEY);
      if (isLanguage(stored)) return stored;
    }
  } catch {
    // Storage may be unavailable (private mode)
  }
  if (typeof navigator === 'undefined') return 'en';
  const preferred = navigator.languages?.length ? navigator.languages : [navigator.language];
  const match = preferred
    .map((tag) => tag?.toLowerCase().split('-')[0])
    .find((code): code is Language => isLanguage(code));
  return match ?? 'en';
}

void i18n.use(initReactI18next).init({
  resources: {
    en: { translation: en },
    ja: { translation: ja },
  },
  lng: detectLanguage(),
  fallbackLng: 'en',
  interpolation: { escapeValue: false },
});

const applyLanguage = (language: string) => {
  if (typeof document !== 'undefined' && document.documentElement) {
    document.documentElement.lang = language;
  }
};
applyLanguage(i18n.language);

i18n.on('languageChanged', (language) => {
  applyLanguage(language);
  try {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(STORAGE_KEY, language);
    }
  } catch {
    // Not remembered across reloads
  }
});

export const currentLanguage = (): Language => (isLanguage(i18n.language) ? i18n.language : 'en');

export default i18n;

