import { describe, expect, it } from 'vitest';
import type { PluginManifest, PluginSettingDefinition } from '../types';
import {
  colorPickerValue,
  declaredSettings,
  isSafeCssColor,
  settingDefaultText,
  settingProblem,
  undeclaredSettings,
} from './pluginSettings';

const def = (overrides: Partial<PluginSettingDefinition>): PluginSettingDefinition => ({
  key: 'k',
  type: 'string',
  label: 'K',
  ...overrides,
});

describe('isSafeCssColor', () => {
  it('accepts the same colors as the server', () => {
    for (const ok of [
      '#fff',
      '#ff0066cc',
      'red',
      'var(--brand)',
      'rgb(1, 2, 3)',
      'hsl(200 50% 40%)',
    ]) {
      expect(isSafeCssColor(ok), ok).toBe(true);
    }
    for (const bad of [
      '#ff',
      '#gggggg',
      'red;',
      'url(x)',
      'var(--a;b)',
      'rgb(1,2,3);color:red',
      '',
    ]) {
      expect(isSafeCssColor(bad), bad).toBe(false);
    }
  });
});

describe('colorPickerValue', () => {
  it('only gives a value the color input understands', () => {
    expect(colorPickerValue('#E11D48')).toBe('#e11d48');
    expect(colorPickerValue('#abc')).toBe('#aabbcc');
    expect(colorPickerValue('red')).toBeNull();
    expect(colorPickerValue('var(--accent)')).toBeNull();
  });
});

describe('settingProblem', () => {
  it('treats an empty value as unset', () => {
    for (const type of ['string', 'text', 'url', 'number', 'boolean', 'select', 'color'] as const) {
      expect(settingProblem(def({ type, options: [{ value: 'a', label: 'A' }] }), '')).toBeNull();
    }
  });

  it('checks numbers against min and max', () => {
    const offset = def({ type: 'number', min: 0, max: 400 });
    expect(settingProblem(offset, '96')).toBeNull();
    expect(settingProblem(offset, '-1')).toEqual({ code: 'tooSmall', params: { min: 0 } });
    expect(settingProblem(offset, '401')).toEqual({ code: 'tooLarge', params: { max: 400 } });
    expect(settingProblem(offset, 'abc')).toEqual({ code: 'notNumber' });
    expect(settingProblem(offset, 'Infinity')).toEqual({ code: 'notNumber' });
  });

  it('checks the other types', () => {
    expect(settingProblem(def({ type: 'boolean' }), 'true')).toBeNull();
    expect(settingProblem(def({ type: 'boolean' }), 'yes')).toEqual({ code: 'notBoolean' });
    const side = def({ type: 'select', options: [{ value: 'left', label: 'Left' }] });
    expect(settingProblem(side, 'left')).toBeNull();
    expect(settingProblem(side, 'top')).toEqual({ code: 'notOption' });
    expect(settingProblem(def({ type: 'color' }), 'red;')).toEqual({ code: 'notColor' });
    expect(settingProblem(def({ type: 'url' }), '/contact')).toBeNull();
    expect(settingProblem(def({ type: 'url' }), 'https://example.com/a')).toBeNull();
    for (const bad of ['//evil.example', 'javascript:alert(1)', 'example.com', 'https://a b']) {
      expect(settingProblem(def({ type: 'url' }), bad), bad).toEqual({ code: 'notUrl' });
    }
    expect(settingProblem(def({ max_length: 3 }), 'abcd')).toEqual({
      code: 'tooLong',
      params: { max: 3 },
    });
  });
});

describe('declared and undeclared settings', () => {
  const manifest = {
    settings: [def({ key: 'position' }), def({ key: 'width_px' })],
  } as PluginManifest;

  it('splits the saved values by the manifest', () => {
    expect(declaredSettings(manifest)).toHaveLength(2);
    expect(declaredSettings(null)).toEqual([]);
    expect(undeclaredSettings(manifest, { position: 'left', legacy: 'x', other: 'y' })).toEqual([
      ['legacy', 'x'],
      ['other', 'y'],
    ]);
    expect(undeclaredSettings(null, { a: '1' })).toEqual([['a', '1']]);
  });

  it('shows the default as text', () => {
    expect(settingDefaultText(def({ default: 96 }))).toBe('96');
    expect(settingDefaultText(def({ default: true }))).toBe('true');
    expect(settingDefaultText(def({ default: 'right' }))).toBe('right');
    expect(settingDefaultText(def({}))).toBe('');
  });
});
