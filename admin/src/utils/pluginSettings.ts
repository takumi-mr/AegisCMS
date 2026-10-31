import type {
  LocalizedText,
  PluginManifest,
  PluginSettingDefinition,
  PluginSettingType,
} from '../types';

/** Same rule as the server (`is_safe_css_color`): what may go into a stylesheet as a color */
export function isSafeCssColor(value: string): boolean {
  const v = value.trim();
  if (v.startsWith('#')) return /^#([0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(v);
  if (/^var\(--[A-Za-z0-9_-]+\)$/.test(v)) return true;
  if (/^(rgb|rgba|hsl|hsla)\(/.test(v)) return /^[A-Za-z]+\([0-9A-Za-z,.% /]*\)$/.test(v);
  return /^[A-Za-z]{1,20}$/.test(v);
}

/** `#rrggbb` for `<input type="color">` (it accepts nothing else), or null for other CSS colors */
export function colorPickerValue(value: string): string | null {
  const v = value.trim();
  if (/^#[0-9a-fA-F]{6}$/.test(v)) return v.toLowerCase();
  const short = /^#([0-9a-fA-F])([0-9a-fA-F])([0-9a-fA-F])$/.exec(v);
  if (!short) return null;
  return `#${short[1]}${short[1]}${short[2]}${short[2]}${short[3]}${short[3]}`.toLowerCase();
}

/** What the plugin uses while the setting is empty, as the text shown in the field */
export function settingDefaultText(setting: PluginSettingDefinition): string {
  const value = setting.default;
  if (value === undefined || value === null) return '';
  return typeof value === 'string' ? value : String(value);
}

/** Longest value in characters (0: no limit), as the server limits it unless `max_length` says otherwise */
const DEFAULT_MAX_LENGTH: Record<PluginSettingType, number> = {
  string: 500,
  text: 20000,
  url: 2000,
  number: 0,
  boolean: 0,
  select: 0,
  color: 0,
};

export interface SettingProblem {
  /** Key under `plugins.settingProblem.*` in the translations */
  code: string;
  params?: Record<string, string | number>;
}

/**
 * Why `value` cannot be saved for `setting`, or null. The server checks the same rules again; this
 * only tells the user before saving. An empty value means "unset" and is always fine.
 */
export function settingProblem(
  setting: PluginSettingDefinition,
  value: string
): SettingProblem | null {
  if (value === '') return null;
  const max = setting.max_length ?? DEFAULT_MAX_LENGTH[setting.type];
  if (max > 0 && [...value].length > max) return { code: 'tooLong', params: { max } };
  switch (setting.type) {
    case 'number': {
      const n = Number(value);
      if (value.trim() === '' || !Number.isFinite(n)) return { code: 'notNumber' };
      if (setting.min !== undefined && n < setting.min) {
        return { code: 'tooSmall', params: { min: setting.min } };
      }
      if (setting.max !== undefined && n > setting.max) {
        return { code: 'tooLarge', params: { max: setting.max } };
      }
      return null;
    }
    case 'boolean':
      return value === 'true' || value === 'false' ? null : { code: 'notBoolean' };
    case 'select':
      return setting.options?.some((o) => o.value === value) ? null : { code: 'notOption' };
    case 'color':
      return isSafeCssColor(value) ? null : { code: 'notColor' };
    case 'url':
      return /^(https?:\/\/|\/(?!\/))\S*$/.test(value) ? null : { code: 'notUrl' };
    default:
      return null;
  }
}

/** Declared settings of the manifest (empty when it declares none) */
export function declaredSettings(manifest: PluginManifest | null): PluginSettingDefinition[] {
  return manifest?.settings ?? [];
}

/** The saved settings the manifest does not declare (they stay in the key/value list) */
export function undeclaredSettings(
  manifest: PluginManifest | null,
  settings: Record<string, string>
): [string, string][] {
  const declared = new Set(declaredSettings(manifest).map((s) => s.key));
  return Object.entries(settings).filter(([key]) => !declared.has(key));
}

/** Label of a select option, falling back to its value */
export function optionLabel(
  option: { value: string; label: LocalizedText },
  localize: (text: LocalizedText | undefined) => string
): string {
  return localize(option.label) || option.value;
}
