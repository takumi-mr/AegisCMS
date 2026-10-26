import React, { useEffect, useState } from 'react';
import { Plus, RotateCcw, Save, ShieldCheck, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi } from '../api/client';
import type { CustomHeader, HeaderSettings } from '../types';
import { Button } from './ui/Button';
import { Field } from './ui/Field';
import styles from './HeaderManager.module.css';

interface HeaderManagerProps {
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

/**
 * Public and admin response HTTP headers management (`GET` / `PUT /api/v1/headers`, admin only).
 * Allows customising defense headers (CSP, X-Frame-Options, nosniff, etc.) and arbitrary HTTP headers.
 */
export const HeaderManager: React.FC<HeaderManagerProps> = ({ onShowToast }) => {
  const { t } = useTranslation();
  const [settings, setSettings] = useState<HeaderSettings | null>(null);

  const [csp, setCsp] = useState<string>('');
  const [xFrameOptions, setXFrameOptions] = useState<string>('');
  const [xContentTypeOptions, setXContentTypeOptions] = useState<string>('');
  const [referrerPolicy, setReferrerPolicy] = useState<string>('');
  const [permissionsPolicy, setPermissionsPolicy] = useState<string>('');
  const [coop, setCoop] = useState<string>('');
  const [customHeaders, setCustomHeaders] = useState<CustomHeader[]>([]);

  const [isSaving, setIsSaving] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  const apply = (loaded: HeaderSettings) => {
    setSettings(loaded);
    setCsp(loaded.content_security_policy ?? '');
    setXFrameOptions(loaded.x_frame_options ?? '');
    setXContentTypeOptions(loaded.x_content_type_options ?? '');
    setReferrerPolicy(loaded.referrer_policy ?? '');
    setPermissionsPolicy(loaded.permissions_policy ?? '');
    setCoop(loaded.cross_origin_opener_policy ?? '');
    setCustomHeaders(loaded.custom_headers.map((h) => ({ name: h.name, value: h.value })));
  };

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .getHeadersSettings()
      .then((loaded) => {
        if (!cancelled) apply(loaded);
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : t('headers.loadFailed'));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [t]);

  const handleAddCustomHeader = () => {
    setCustomHeaders((prev) => [...prev, { name: '', value: '' }]);
  };

  const handleRemoveCustomHeader = (index: number) => {
    setCustomHeaders((prev) => prev.filter((_, i) => i !== index));
  };

  const handleCustomHeaderChange = (index: number, field: 'name' | 'value', val: string) => {
    setCustomHeaders((prev) =>
      prev.map((item, i) => (i === index ? { ...item, [field]: val } : item))
    );
  };

  const handleResetCsp = () => {
    if (settings) {
      setCsp(settings.defaults.content_security_policy);
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsSaving(true);
    setError(null);
    try {
      const validCustomHeaders = customHeaders
        .map((h) => ({ name: h.name.trim(), value: h.value.trim() }))
        .filter((h) => h.name.length > 0 || h.value.length > 0);

      const updated = await aegisApi.updateHeadersSettings({
        content_security_policy: csp.trim() || null,
        x_frame_options: xFrameOptions.trim() || null,
        x_content_type_options: xContentTypeOptions.trim() || null,
        referrer_policy: referrerPolicy.trim() || null,
        permissions_policy: permissionsPolicy.trim() || null,
        cross_origin_opener_policy: coop.trim() || null,
        custom_headers: validCustomHeaders,
      });
      apply(updated);
      onShowToast(t('headers.saved'));
    } catch (err) {
      setError(err instanceof Error ? err.message : t('headers.saveFailed'));
    } finally {
      setIsSaving(false);
    }
  };

  const isDirty =
    settings !== null &&
    (csp.trim() !== (settings.content_security_policy ?? '') ||
      xFrameOptions.trim() !== (settings.x_frame_options ?? '') ||
      xContentTypeOptions.trim() !== (settings.x_content_type_options ?? '') ||
      referrerPolicy.trim() !== (settings.referrer_policy ?? '') ||
      permissionsPolicy.trim() !== (settings.permissions_policy ?? '') ||
      coop.trim() !== (settings.cross_origin_opener_policy ?? '') ||
      JSON.stringify(customHeaders) !== JSON.stringify(settings.custom_headers));

  return (
    <main className={styles.main}>
      <div className={styles.column}>
        <div className={styles.title}>
          <ShieldCheck size={22} />
          <h1>{t('headers.title')}</h1>
        </div>
        <p className={styles.description}>{t('headers.description')}</p>

        <form className={styles.form} onSubmit={handleSubmit}>
          {/* Defense Headers Section */}
          <div className={styles.section}>
            <div className={styles.sectionHeader}>
              <div className={styles.sectionTitle}>{t('headers.defenseSectionTitle')}</div>
            </div>
            <p className={styles.hint}>{t('headers.defenseSectionHint')}</p>

            {/* CSP */}
            <div className={styles.card}>
              <div className={styles.sectionHeader}>
                <label className={styles.cardLabel} htmlFor="headers-csp">
                  Content-Security-Policy (CSP)
                </label>
                <Button
                  variant="tool"
                  onClick={handleResetCsp}
                  title={t('headers.resetToDefault')}
                  disabled={!settings}
                >
                  <RotateCcw size={12} />
                  <span>{t('headers.resetDefault')}</span>
                </Button>
              </div>
              <Field className={styles.textareaField}>
                <textarea
                  id="headers-csp"
                  className={styles.textarea}
                  value={csp}
                  onChange={(e) => setCsp(e.target.value)}
                  placeholder={settings?.defaults.content_security_policy}
                  rows={2}
                  disabled={!settings}
                  aria-label="Content-Security-Policy"
                />
              </Field>
              <p className={styles.hint}>
                {t('headers.cspHint', {
                  defaultVal: settings?.defaults.content_security_policy ?? '…',
                })}
              </p>
            </div>

            {/* Grid of Other Security Headers */}
            <div className={styles.grid}>
              <div className={styles.card}>
                <label className={styles.cardLabel} htmlFor="headers-x-frame">
                  X-Frame-Options
                </label>
                <Field>
                  <input
                    id="headers-x-frame"
                    value={xFrameOptions}
                    onChange={(e) => setXFrameOptions(e.target.value)}
                    placeholder={settings?.defaults.x_frame_options ?? 'SAMEORIGIN'}
                    disabled={!settings}
                  />
                </Field>
                <p className={styles.hint}>
                  {t('headers.xFrameHint', {
                    defaultVal: settings?.defaults.x_frame_options ?? 'SAMEORIGIN',
                  })}
                </p>
              </div>

              <div className={styles.card}>
                <label className={styles.cardLabel} htmlFor="headers-x-content-type">
                  X-Content-Type-Options
                </label>
                <Field>
                  <input
                    id="headers-x-content-type"
                    value={xContentTypeOptions}
                    onChange={(e) => setXContentTypeOptions(e.target.value)}
                    placeholder={settings?.defaults.x_content_type_options ?? 'nosniff'}
                    disabled={!settings}
                  />
                </Field>
                <p className={styles.hint}>
                  {t('headers.xContentTypeHint', {
                    defaultVal: settings?.defaults.x_content_type_options ?? 'nosniff',
                  })}
                </p>
              </div>

              <div className={styles.card}>
                <label className={styles.cardLabel} htmlFor="headers-referrer">
                  Referrer-Policy
                </label>
                <Field>
                  <input
                    id="headers-referrer"
                    value={referrerPolicy}
                    onChange={(e) => setReferrerPolicy(e.target.value)}
                    placeholder={
                      settings?.defaults.referrer_policy ?? 'strict-origin-when-cross-origin'
                    }
                    disabled={!settings}
                  />
                </Field>
                <p className={styles.hint}>
                  {t('headers.referrerPolicyHint', {
                    defaultVal:
                      settings?.defaults.referrer_policy ?? 'strict-origin-when-cross-origin',
                  })}
                </p>
              </div>

              <div className={styles.card}>
                <label className={styles.cardLabel} htmlFor="headers-permissions">
                  Permissions-Policy
                </label>
                <Field>
                  <input
                    id="headers-permissions"
                    value={permissionsPolicy}
                    onChange={(e) => setPermissionsPolicy(e.target.value)}
                    placeholder={settings?.defaults.permissions_policy}
                    disabled={!settings}
                  />
                </Field>
                <p className={styles.hint}>{t('headers.permissionsPolicyHint')}</p>
              </div>

              <div className={styles.card}>
                <label className={styles.cardLabel} htmlFor="headers-coop">
                  Cross-Origin-Opener-Policy
                </label>
                <Field>
                  <input
                    id="headers-coop"
                    value={coop}
                    onChange={(e) => setCoop(e.target.value)}
                    placeholder={settings?.defaults.cross_origin_opener_policy ?? 'same-origin'}
                    disabled={!settings}
                  />
                </Field>
                <p className={styles.hint}>{t('headers.coopHint')}</p>
              </div>
            </div>
          </div>

          {/* Custom Headers Section */}
          <div className={styles.section}>
            <div className={styles.sectionHeader}>
              <div className={styles.sectionTitle}>{t('headers.customSectionTitle')}</div>
              <Button variant="tool" onClick={handleAddCustomHeader} disabled={!settings}>
                <Plus size={14} />
                <span>{t('headers.addCustomHeader')}</span>
              </Button>
            </div>
            <p className={styles.hint}>{t('headers.customSectionHint')}</p>

            <div className={styles.customHeadersList}>
              {customHeaders.length === 0 ? (
                <div className={styles.emptyCustom}>{t('headers.noCustomHeaders')}</div>
              ) : (
                customHeaders.map((header, idx) => (
                  <div key={idx} className={styles.customHeaderRow}>
                    <div className={styles.customHeaderName}>
                      <Field>
                        <input
                          placeholder={t('headers.headerNamePlaceholder')}
                          value={header.name}
                          onChange={(e) => handleCustomHeaderChange(idx, 'name', e.target.value)}
                          aria-label={`Header Name ${idx + 1}`}
                          disabled={!settings}
                        />
                      </Field>
                    </div>
                    <div className={styles.customHeaderValue}>
                      <Field>
                        <input
                          placeholder={t('headers.headerValuePlaceholder')}
                          value={header.value}
                          onChange={(e) => handleCustomHeaderChange(idx, 'value', e.target.value)}
                          aria-label={`Header Value ${idx + 1}`}
                          disabled={!settings}
                        />
                      </Field>
                    </div>
                    <Button
                      variant="tool"
                      onClick={() => handleRemoveCustomHeader(idx)}
                      title={t('headers.removeHeader')}
                      aria-label={t('headers.removeHeader')}
                      disabled={!settings}
                    >
                      <Trash2 size={14} />
                    </Button>
                  </div>
                ))
              )}
            </div>
          </div>

          {error && <div className={styles.error}>{error}</div>}

          <div className={styles.actions}>
            <Button type="submit" variant="primary" disabled={!settings || isSaving || !isDirty}>
              <Save size={16} />
              <span>{isSaving ? t('headers.saving') : t('headers.save')}</span>
            </Button>
          </div>
        </form>
      </div>
    </main>
  );
};
