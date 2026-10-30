import React from 'react';
import { Monitor, Moon, Sun } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { setThemeMode, THEME_MODES, useThemeMode, type ThemeMode } from '../../theme';
import { cx } from '../../utils/cx';
import styles from './ThemeSelect.module.css';

const ICONS = { system: Monitor, light: Sun, dark: Moon } as const;

/** Switches the Admin Studio color theme: light, dark or follow the OS (remembered in this browser) */
export const ThemeSelect: React.FC<{ className?: string }> = ({ className }) => {
  const { t } = useTranslation();
  const mode = useThemeMode();
  const Icon = ICONS[mode];
  return (
    <label className={cx(styles.select, className)} title={t('theme.label')}>
      <Icon size={14} aria-hidden="true" />
      <select
        value={mode}
        onChange={(e) => setThemeMode(e.target.value as ThemeMode)}
        aria-label={t('theme.label')}
      >
        {THEME_MODES.map((option) => (
          <option key={option} value={option}>
            {t(`theme.${option}`)}
          </option>
        ))}
      </select>
    </label>
  );
};
