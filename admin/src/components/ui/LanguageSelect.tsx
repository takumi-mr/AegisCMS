import React from 'react';
import { Languages } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { LANGUAGES } from '../../i18n';
import { cx } from '../../utils/cx';
import styles from './LanguageSelect.module.css';

/** Switches the Admin Studio language (remembered in this browser) */
export const LanguageSelect: React.FC<{ className?: string }> = ({ className }) => {
  const { t, i18n } = useTranslation();
  return (
    <label className={cx(styles.select, className)} title={t('common.language')}>
      <Languages size={14} aria-hidden="true" />
      <select
        value={i18n.language}
        onChange={(e) => i18n.changeLanguage(e.target.value)}
        aria-label={t('common.language')}
      >
        {LANGUAGES.map((language) => (
          <option key={language.code} value={language.code}>
            {language.label}
          </option>
        ))}
      </select>
    </label>
  );
};
