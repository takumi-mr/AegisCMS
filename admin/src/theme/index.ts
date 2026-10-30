// Admin Studio color theme: light, dark, or follow the OS. The choice is remembered in this
// browser (like the language, see `i18n`). `public/theme-init.js` applies it before the first paint
// with the same rules; this module keeps it applied afterwards and follows OS changes.
import { useSyncExternalStore } from 'react';

export const THEME_MODES = ['system', 'light', 'dark'] as const;

export type ThemeMode = (typeof THEME_MODES)[number];

/** What the page is drawn with (`data-theme` on `<html>`) */
export type ResolvedTheme = 'light' | 'dark';

export const THEME_STORAGE_KEY = 'aegis.admin.theme';

export const isThemeMode = (value: string | null | undefined): value is ThemeMode =>
  THEME_MODES.some((mode) => mode === value);

/** The mode chosen before; "system" when none (or an unknown value) is stored */
export const readStoredThemeMode = (): ThemeMode => {
  try {
    if (typeof localStorage !== 'undefined') {
      const stored = localStorage.getItem(THEME_STORAGE_KEY);
      if (isThemeMode(stored)) return stored;
    }
  } catch {
    // Storage may be unavailable (private mode); follow the OS
  }
  return 'system';
};

/**
 * The theme to draw. "system" is light only when the OS asks for light; with no preference the
 * look is dark (as it was before there was a choice)
 */
export const resolveTheme = (mode: ThemeMode, systemPrefersLight: boolean): ResolvedTheme =>
  mode === 'system' ? (systemPrefersLight ? 'light' : 'dark') : mode;

const lightQuery = (): MediaQueryList | null =>
  typeof window !== 'undefined' && typeof window.matchMedia === 'function'
    ? window.matchMedia('(prefers-color-scheme: light)')
    : null;

let currentMode: ThemeMode = readStoredThemeMode();
const listeners = new Set<() => void>();

export const getThemeMode = (): ThemeMode => currentMode;

export const getResolvedTheme = (): ResolvedTheme =>
  resolveTheme(currentMode, lightQuery()?.matches ?? false);

const apply = () => {
  if (typeof document !== 'undefined' && document.documentElement) {
    document.documentElement.dataset.theme = getResolvedTheme();
  }
};

const notify = () => listeners.forEach((listener) => listener());

export const setThemeMode = (mode: ThemeMode): void => {
  currentMode = mode;
  try {
    if (typeof localStorage !== 'undefined') localStorage.setItem(THEME_STORAGE_KEY, mode);
  } catch {
    // Not remembered across reloads; the switch still applies now
  }
  apply();
  notify();
};

export const subscribeTheme = (listener: () => void): (() => void) => {
  listeners.add(listener);
  return () => listeners.delete(listener);
};

// Follow the OS while "system" is chosen
lightQuery()?.addEventListener?.('change', () => {
  if (currentMode === 'system') {
    apply();
    notify();
  }
});
apply();

/** The chosen mode, re-rendering when it changes */
export const useThemeMode = (): ThemeMode => useSyncExternalStore(subscribeTheme, getThemeMode);
