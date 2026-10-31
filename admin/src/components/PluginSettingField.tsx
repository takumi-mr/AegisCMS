import React from 'react';
import { useTranslation } from 'react-i18next';
import i18n from '../i18n';
import type { PluginSettingDefinition } from '../types';
import { cx } from '../utils/cx';
import { localize } from '../utils/pluginBlocks';
import { colorPickerValue, optionLabel, settingDefaultText } from '../utils/pluginSettings';
import styles from './PluginSettingField.module.css';

interface PluginSettingFieldProps {
  setting: PluginSettingDefinition;
  /** Saved value; an empty string means "not set" (the plugin uses its default) */
  value: string;
  /** Why the current value is refused (shown under the field) */
  error?: string | null;
  disabled?: boolean;
  onChange: (value: string) => void;
}

/** One input of a plugin's declared settings (`settings` of its manifest), chosen by `type` */
export const PluginSettingField: React.FC<PluginSettingFieldProps> = ({
  setting,
  value,
  error,
  disabled,
  onChange,
}) => {
  const { t } = useTranslation();
  const text = (v: Parameters<typeof localize>[0]) => localize(v, i18n.language);
  const id = `plugin-setting-${setting.key}`;
  const defaultText = settingDefaultText(setting);
  const placeholder = text(setting.placeholder) || defaultText;
  const description = text(setting.description);
  const describedBy = [description ? `${id}-help` : '', error ? `${id}-error` : '']
    .filter(Boolean)
    .join(' ');
  const common = {
    id,
    disabled,
    'aria-invalid': error ? true : undefined,
    'aria-describedby': describedBy || undefined,
  };

  const defaultOption = (shown: string) =>
    shown ? t('plugins.field.useDefaultValue', { value: shown }) : t('plugins.field.useDefault');

  let control: React.ReactNode;
  switch (setting.type) {
    case 'text':
      control = (
        <textarea
          {...common}
          className={cx(styles.control, styles.textarea)}
          rows={4}
          value={value}
          placeholder={placeholder}
          onChange={(e) => onChange(e.target.value)}
        />
      );
      break;
    case 'number':
      control = (
        <input
          {...common}
          className={styles.control}
          type="number"
          step="any"
          min={setting.min}
          max={setting.max}
          value={value}
          placeholder={placeholder}
          onChange={(e) => onChange(e.target.value)}
        />
      );
      break;
    case 'boolean':
      control = (
        <select
          {...common}
          className={styles.control}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        >
          <option value="">
            {defaultOption(
              defaultText === ''
                ? ''
                : defaultText === 'true'
                  ? t('plugins.field.on')
                  : t('plugins.field.off')
            )}
          </option>
          <option value="true">{t('plugins.field.on')}</option>
          <option value="false">{t('plugins.field.off')}</option>
        </select>
      );
      break;
    case 'select': {
      const options = setting.options ?? [];
      const shownDefault = options.find((o) => o.value === defaultText);
      control = (
        <select
          {...common}
          className={styles.control}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        >
          <option value="">
            {defaultOption(shownDefault ? optionLabel(shownDefault, text) : defaultText)}
          </option>
          {options.map((o) => (
            <option key={o.value} value={o.value}>
              {optionLabel(o, text)}
            </option>
          ))}
          {value !== '' && !options.some((o) => o.value === value) && (
            <option value={value}>{value}</option>
          )}
        </select>
      );
      break;
    }
    case 'color': {
      const picked = colorPickerValue(value) ?? colorPickerValue(defaultText) ?? '#000000';
      control = (
        <div className={styles.colorRow}>
          <input
            type="color"
            className={styles.swatch}
            value={picked}
            disabled={disabled}
            aria-label={t('plugins.field.pickColor')}
            onChange={(e) => onChange(e.target.value)}
          />
          <input
            {...common}
            className={styles.control}
            type="text"
            value={value}
            placeholder={placeholder}
            spellCheck={false}
            onChange={(e) => onChange(e.target.value)}
          />
        </div>
      );
      break;
    }
    default:
      control = (
        <input
          {...common}
          className={styles.control}
          type="text"
          inputMode={setting.type === 'url' ? 'url' : undefined}
          value={value}
          placeholder={placeholder}
          spellCheck={false}
          onChange={(e) => onChange(e.target.value)}
        />
      );
  }

  return (
    <div className={cx(styles.field, error && styles.invalid)} data-setting-key={setting.key}>
      <label htmlFor={id} className={styles.label}>
        {text(setting.label)}
        <code className={styles.key}>{setting.key}</code>
      </label>
      {control}
      {description && (
        <p id={`${id}-help`} className={styles.help}>
          {description}
        </p>
      )}
      {error && (
        <p id={`${id}-error`} className={styles.error} role="alert">
          {error}
        </p>
      )}
    </div>
  );
};
