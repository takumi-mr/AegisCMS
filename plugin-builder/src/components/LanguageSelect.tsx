import { Languages } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { LANGUAGES } from '../i18n';

export function LanguageSelect() {
  const { t, i18n } = useTranslation();

  return (
    <label
      title={t('common.language')}
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: '4px',
        padding: '3px 8px',
        backgroundColor: '#ffffff',
        border: '1px solid #d0d7de',
        borderRadius: '6px',
        fontSize: '12px',
        color: '#24292f',
        cursor: 'pointer',
      }}
    >
      <Languages size={14} color="#57609a" aria-hidden="true" />
      <select
        value={i18n.language}
        onChange={(e) => void i18n.changeLanguage(e.target.value)}
        aria-label={t('common.language')}
        style={{
          border: 'none',
          backgroundColor: 'transparent',
          fontSize: '12px',
          color: 'inherit',
          cursor: 'pointer',
          outline: 'none',
        }}
      >
        {LANGUAGES.map((lang) => (
          <option key={lang.code} value={lang.code}>
            {lang.label}
          </option>
        ))}
      </select>
    </label>
  );
}

