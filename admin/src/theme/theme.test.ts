import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { isThemeMode, resolveTheme, THEME_MODES, type ThemeMode } from './index';

// Minimal browser stand-ins: the unit tests run in Node
const createBrowser = (stored: string | null, systemLight: boolean) => {
  const storage = new Map<string, string>(stored === null ? [] : [['aegis.admin.theme', stored]]);
  const listeners = new Set<() => void>();
  const query = {
    matches: systemLight,
    addEventListener: (_: string, listener: () => void) => listeners.add(listener),
  };
  const attributes = new Map<string, string>();
  const root = {
    dataset: {} as Record<string, string>,
    setAttribute: (name: string, value: string) => {
      attributes.set(name, value);
      if (name === 'data-theme') root.dataset.theme = value;
    },
  };
  return {
    storage,
    query,
    root,
    attributes,
    fireSystemChange: (light: boolean) => {
      query.matches = light;
      listeners.forEach((listener) => listener());
    },
    globals: {
      localStorage: {
        getItem: (key: string) => storage.get(key) ?? null,
        setItem: (key: string, value: string) => void storage.set(key, value),
      },
      window: { matchMedia: () => query },
      document: { documentElement: root },
    },
  };
};

describe('resolveTheme', () => {
  it('uses the chosen theme, and for "system" follows the OS (dark when it does not ask for light)', () => {
    expect(resolveTheme('light', false)).toBe('light');
    expect(resolveTheme('dark', true)).toBe('dark');
    expect(resolveTheme('system', true)).toBe('light');
    expect(resolveTheme('system', false)).toBe('dark');
  });

  it('knows the modes', () => {
    expect(THEME_MODES).toEqual(['system', 'light', 'dark']);
    expect(isThemeMode('light') && isThemeMode('dark') && isThemeMode('system')).toBe(true);
    expect(isThemeMode('sepia')).toBe(false);
    expect(isThemeMode(null)).toBe(false);
  });
});

describe('public/theme-init.js (applied before the first paint)', () => {
  const script = readFileSync(new URL('../../public/theme-init.js', import.meta.url), 'utf8');
  const run = (stored: string | null, systemLight: boolean) => {
    const browser = createBrowser(stored, systemLight);
    runInNewContext(script, browser.globals);
    return browser.attributes.get('data-theme');
  };

  it('agrees with resolveTheme for every stored value and OS preference', () => {
    for (const systemLight of [true, false]) {
      for (const stored of [null, 'system', 'light', 'dark', 'garbage']) {
        const mode: ThemeMode = isThemeMode(stored) ? stored : 'system';
        expect(run(stored, systemLight), `${stored} / system light: ${systemLight}`).toBe(
          resolveTheme(mode, systemLight)
        );
      }
    }
  });

  it('draws dark when the browser cannot tell, or when storage is blocked', () => {
    const browser = createBrowser(null, false);
    runInNewContext(script, {
      document: browser.globals.document,
      window: {},
      localStorage: {
        getItem: () => {
          throw new Error('blocked');
        },
      },
    });
    expect(browser.attributes.get('data-theme')).toBe('dark');
  });
});

describe('the theme module', () => {
  beforeEach(() => {
    vi.resetModules();
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  const load = async (stored: string | null, systemLight: boolean) => {
    const browser = createBrowser(stored, systemLight);
    for (const [name, value] of Object.entries(browser.globals)) vi.stubGlobal(name, value);
    const theme = await import('./index');
    return { browser, theme };
  };

  it('starts from the stored mode and applies it', async () => {
    const { browser, theme } = await load('light', false);
    expect(theme.getThemeMode()).toBe('light');
    expect(browser.root.dataset.theme).toBe('light');
  });

  it('follows the OS with no stored mode (or an unknown one)', async () => {
    expect((await load(null, true)).browser.root.dataset.theme).toBe('light');
    vi.resetModules();
    const { browser, theme } = await load('garbage', false);
    expect(theme.getThemeMode()).toBe('system');
    expect(browser.root.dataset.theme).toBe('dark');
  });

  it('applies, remembers and announces a new choice', async () => {
    const { browser, theme } = await load(null, true);
    const listener = vi.fn();
    theme.subscribeTheme(listener);

    theme.setThemeMode('dark');
    expect(browser.root.dataset.theme).toBe('dark');
    expect(browser.storage.get('aegis.admin.theme')).toBe('dark');
    expect(theme.getThemeMode()).toBe('dark');
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it('follows OS changes only while "system" is chosen', async () => {
    const { browser, theme } = await load(null, false);
    expect(browser.root.dataset.theme).toBe('dark');
    browser.fireSystemChange(true);
    expect(browser.root.dataset.theme).toBe('light');

    theme.setThemeMode('dark');
    browser.fireSystemChange(false);
    browser.fireSystemChange(true);
    expect(browser.root.dataset.theme).toBe('dark');
  });

  it('still applies the choice when storage is blocked', async () => {
    const { browser, theme } = await load(null, false);
    vi.stubGlobal('localStorage', {
      getItem: () => null,
      setItem: () => {
        throw new Error('blocked');
      },
    });
    theme.setThemeMode('light');
    expect(browser.root.dataset.theme).toBe('light');
  });
});
