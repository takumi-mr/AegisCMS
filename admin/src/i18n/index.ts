// Admin Studio translations (i18next). Strings live in src/locales/<language>/translation.json,
// the format translation services (Crowdin, Lokalise, ...) read and write.
import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import en from '../locales/en/translation.json';
import ja from '../locales/ja/translation.json';

export const LANGUAGES = [
  { code: 'en', label: 'English' },
  { code: 'ja', label: '日本語' },
] as const;

export type Language = (typeof LANGUAGES)[number]['code'];

const STORAGE_KEY = 'aegis.admin.language';

const isLanguage = (value: string | null | undefined): value is Language =>
  LANGUAGES.some((l) => l.code === value);

/** The language chosen before, else the browser's (Japanese or English) */
function detectLanguage(): Language {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (isLanguage(stored)) return stored;
  } catch {
    // Storage may be unavailable (private mode); fall back to the browser
  }
  const preferred = navigator.languages?.length ? navigator.languages : [navigator.language];
  const match = preferred
    .map((tag) => tag?.toLowerCase().split('-')[0])
    .find((code): code is Language => isLanguage(code));
  return match ?? 'en';
}

i18n.use(initReactI18next).init({
  resources: {
    en: { translation: en },
    ja: { translation: ja },
  },
  lng: detectLanguage(),
  fallbackLng: 'en',
  // React escapes rendered text already
  interpolation: { escapeValue: false },
});

const applyLanguage = (language: string) => {
  document.documentElement.lang = language;
};
applyLanguage(i18n.language);

i18n.on('languageChanged', (language) => {
  applyLanguage(language);
  try {
    localStorage.setItem(STORAGE_KEY, language);
  } catch {
    // Not remembered across reloads; the switch still applies now
  }
});

/** The current language, for Intl date and number formatting */
export const currentLanguage = (): Language => (isLanguage(i18n.language) ? i18n.language : 'en');

export default i18n;
