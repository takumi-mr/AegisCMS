import React, { useEffect, useState } from 'react';
import {
  AlertTriangle,
  HardDrive,
  MessageSquare,
  RotateCcw,
  Save,
  Shield,
  SlidersHorizontal,
  Wrench,
  Zap,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi } from '../api/client';
import type { SystemSettingsResponse, SystemOverrides } from '../types';
import { Button } from './ui/Button';
import { Field } from './ui/Field';
import styles from './SystemManager.module.css';

interface SystemManagerProps {
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

export const SystemManager: React.FC<SystemManagerProps> = ({ onShowToast }) => {
  const { t } = useTranslation();
  const [data, setData] = useState<SystemSettingsResponse | null>(null);
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [isSaving, setIsSaving] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  // Form states (overrides: value or undefined/null for default)
  const [maintenanceMode, setMaintenanceMode] = useState<boolean>(false);
  const [maintenanceMessage, setMaintenanceMessage] = useState<string>('');
  const [isMaintenanceOverridden, setIsMaintenanceOverridden] = useState<boolean>(false);

  const [maxUploadMb, setMaxUploadMb] = useState<string>('');
  const [cacheEnabled, setCacheEnabled] = useState<boolean>(true);
  const [cacheTtlSecs, setCacheTtlSecs] = useState<string>('');
  const [registrationEnabled, setRegistrationEnabled] = useState<boolean>(false);
  const [maxLoginAttempts, setMaxLoginAttempts] = useState<string>('');
  const [commentsEnabled, setCommentsEnabled] = useState<boolean>(true);
  const [forumEnabled, setForumEnabled] = useState<boolean>(true);

  // Track which fields are explicitly overridden
  const [overridesState, setOverridesState] = useState<SystemOverrides>({
    maintenance_mode: null,
    maintenance_message: null,
    max_upload_mb: null,
    cache_enabled: null,
    cache_ttl_secs: null,
    registration_enabled: null,
    max_login_attempts: null,
    comments_enabled: null,
    forum_enabled: null,
  });

  const applyLoaded = (res: SystemSettingsResponse) => {
    setData(res);
    setOverridesState(res.overrides);

    setMaintenanceMode(res.settings.maintenance_mode);
    setMaintenanceMessage(res.settings.maintenance_message);
    setIsMaintenanceOverridden(res.overrides.maintenance_mode !== null);

    setMaxUploadMb(
      res.overrides.max_upload_mb !== null
        ? String(res.overrides.max_upload_mb)
        : String(res.defaults.max_upload_mb)
    );
    setCacheEnabled(res.settings.cache_enabled);
    setCacheTtlSecs(
      res.overrides.cache_ttl_secs !== null
        ? String(res.overrides.cache_ttl_secs)
        : String(res.defaults.cache_ttl_secs)
    );
    setRegistrationEnabled(res.settings.registration_enabled);
    setMaxLoginAttempts(
      res.overrides.max_login_attempts !== null
        ? String(res.overrides.max_login_attempts)
        : String(res.defaults.max_login_attempts)
    );
    setCommentsEnabled(res.settings.comments_enabled);
    setForumEnabled(res.settings.forum_enabled);
  };

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .getSystemSettings()
      .then((res) => {
        if (!cancelled) {
          applyLoaded(res);
          setIsLoading(false);
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : t('system.loadFailed'));
          setIsLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [t]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!data) return;

    setIsSaving(true);
    setError(null);

    const payload: Partial<SystemOverrides> = {
      maintenance_mode: isMaintenanceOverridden ? maintenanceMode : null,
      maintenance_message:
        maintenanceMessage.trim() && maintenanceMessage.trim() !== data.defaults.maintenance_message
          ? maintenanceMessage.trim()
          : null,
      max_upload_mb:
        overridesState.max_upload_mb !== null && maxUploadMb.trim()
          ? Number.parseInt(maxUploadMb, 10) || data.defaults.max_upload_mb
          : null,
      cache_enabled: overridesState.cache_enabled !== null ? cacheEnabled : null,
      cache_ttl_secs:
        overridesState.cache_ttl_secs !== null && cacheTtlSecs.trim()
          ? Number.parseInt(cacheTtlSecs, 10) || data.defaults.cache_ttl_secs
          : null,
      registration_enabled:
        overridesState.registration_enabled !== null ? registrationEnabled : null,
      max_login_attempts:
        overridesState.max_login_attempts !== null && maxLoginAttempts.trim()
          ? Number.parseInt(maxLoginAttempts, 10) || data.defaults.max_login_attempts
          : null,
      comments_enabled: overridesState.comments_enabled !== null ? commentsEnabled : null,
      forum_enabled: overridesState.forum_enabled !== null ? forumEnabled : null,
    };

    try {
      const updated = await aegisApi.updateSystemSettings(payload);
      applyLoaded(updated);
      onShowToast(t('system.saved'));
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : t('system.saveFailed'));
    } finally {
      setIsSaving(false);
    }
  };

  if (isLoading || !data) {
    return (
      <main className={styles.main}>
        <div className={styles.column}>
          <div className={styles.description}>{t('system.loading')}</div>
        </div>
      </main>
    );
  }

  return (
    <main className={styles.main}>
      <div className={styles.column}>
        <div className={styles.title}>
          <SlidersHorizontal size={22} />
          <h1>{t('system.title')}</h1>
        </div>
        <p className={styles.description}>{t('system.description')}</p>

        <form className={styles.form} onSubmit={handleSubmit}>
          {error && <div className={styles.error}>{error}</div>}

          {/* 1. Maintenance Mode */}
          <div className={styles.card}>
            <div className={styles.cardHeader}>
              <div className={styles.cardTitle}>
                <Wrench size={18} />
                <span>{t('system.maintenance.heading')}</span>
              </div>
              {isMaintenanceOverridden && (
                <span className={styles.overrideBadge}>{t('system.activeOverride')}</span>
              )}
            </div>
            <p className={styles.hint}>{t('system.maintenance.hint')}</p>

            <div className={styles.fieldGroup}>
              <label className={styles.toggleLabel}>
                <input
                  type="checkbox"
                  className={styles.toggleInput}
                  checked={maintenanceMode}
                  onChange={(e) => {
                    setMaintenanceMode(e.target.checked);
                    setIsMaintenanceOverridden(true);
                  }}
                />
                <span>{t('system.maintenance.enable')}</span>
              </label>

              {maintenanceMode && (
                <div className={styles.warningBanner} role="alert">
                  <AlertTriangle size={18} />
                  <span>
                    {t('system.maintenance.enable')} — {t('system.maintenance.hint')}
                  </span>
                </div>
              )}
            </div>

            <div className={styles.fieldGroup}>
              <div className={styles.fieldHeader}>
                <label className={styles.fieldLabel} htmlFor="maintenanceMessage">
                  {t('system.maintenance.messageLabel')}
                </label>
                {maintenanceMessage !== data.defaults.maintenance_message && (
                  <button
                    type="button"
                    className={styles.resetBtn}
                    onClick={() => {
                      setMaintenanceMessage(data.defaults.maintenance_message);
                      setIsMaintenanceOverridden(false);
                      setMaintenanceMode(data.defaults.maintenance_mode);
                    }}
                  >
                    <RotateCcw size={12} style={{ display: 'inline', marginRight: 4 }} />
                    {t('system.resetToDefault')}
                  </button>
                )}
              </div>
              <Field>
                <input
                  id="maintenanceMessage"
                  value={maintenanceMessage}
                  onChange={(e) => {
                    setMaintenanceMessage(e.target.value);
                    setIsMaintenanceOverridden(true);
                  }}
                  placeholder={data.defaults.maintenance_message}
                  maxLength={1000}
                />
              </Field>
            </div>
          </div>

          {/* 2. Storage & Upload */}
          <div className={styles.card}>
            <div className={styles.cardHeader}>
              <div className={styles.cardTitle}>
                <HardDrive size={18} />
                <span>{t('system.storage.heading')}</span>
              </div>
              {overridesState.max_upload_mb !== null && (
                <span className={styles.overrideBadge}>{t('system.activeOverride')}</span>
              )}
            </div>
            <p className={styles.hint}>{t('system.storage.hint')}</p>

            <div className={styles.fieldGroup}>
              <div className={styles.fieldHeader}>
                <label className={styles.fieldLabel} htmlFor="maxUploadMb">
                  {t('system.storage.maxUploadLabel')}{' '}
                  <span className={styles.hint}>
                    {t('system.defaultLabel', { value: `${data.defaults.max_upload_mb} MB` })}
                  </span>
                </label>
                {overridesState.max_upload_mb !== null && (
                  <button
                    type="button"
                    className={styles.resetBtn}
                    onClick={() => {
                      setMaxUploadMb(String(data.defaults.max_upload_mb));
                      setOverridesState((prev) => ({ ...prev, max_upload_mb: null }));
                    }}
                  >
                    <RotateCcw size={12} style={{ display: 'inline', marginRight: 4 }} />
                    {t('system.resetToDefault')}
                  </button>
                )}
              </div>
              <div className={styles.rowInputs}>
                <Field>
                  <input
                    id="maxUploadMb"
                    type="number"
                    min="1"
                    max="1024"
                    value={maxUploadMb}
                    onChange={(e) => {
                      setMaxUploadMb(e.target.value);
                      setOverridesState((prev) => ({
                        ...prev,
                        max_upload_mb: Number.parseInt(e.target.value, 10) || null,
                      }));
                    }}
                  />
                </Field>
                <span>MB</span>
              </div>
            </div>
          </div>

          {/* 3. Cache */}
          <div className={styles.card}>
            <div className={styles.cardHeader}>
              <div className={styles.cardTitle}>
                <Zap size={18} />
                <span>{t('system.cache.heading')}</span>
              </div>
              {(overridesState.cache_enabled !== null ||
                overridesState.cache_ttl_secs !== null) && (
                <span className={styles.overrideBadge}>{t('system.activeOverride')}</span>
              )}
            </div>
            <p className={styles.hint}>{t('system.cache.hint')}</p>

            <div className={styles.fieldGroup}>
              <div className={styles.fieldHeader}>
                <label className={styles.toggleLabel}>
                  <input
                    type="checkbox"
                    className={styles.toggleInput}
                    checked={cacheEnabled}
                    onChange={(e) => {
                      setCacheEnabled(e.target.checked);
                      setOverridesState((prev) => ({ ...prev, cache_enabled: e.target.checked }));
                    }}
                  />
                  <span>{t('system.cache.enable')}</span>
                </label>
                {overridesState.cache_enabled !== null && (
                  <button
                    type="button"
                    className={styles.resetBtn}
                    onClick={() => {
                      setCacheEnabled(data.defaults.cache_enabled);
                      setOverridesState((prev) => ({ ...prev, cache_enabled: null }));
                    }}
                  >
                    <RotateCcw size={12} style={{ display: 'inline', marginRight: 4 }} />
                    {t('system.resetToDefault')}
                  </button>
                )}
              </div>
            </div>

            <div className={styles.fieldGroup}>
              <div className={styles.fieldHeader}>
                <label className={styles.fieldLabel} htmlFor="cacheTtlSecs">
                  {t('system.cache.ttlLabel')}{' '}
                  <span className={styles.hint}>
                    {t('system.defaultLabel', { value: `${data.defaults.cache_ttl_secs}s` })}
                  </span>
                </label>
                {overridesState.cache_ttl_secs !== null && (
                  <button
                    type="button"
                    className={styles.resetBtn}
                    onClick={() => {
                      setCacheTtlSecs(String(data.defaults.cache_ttl_secs));
                      setOverridesState((prev) => ({ ...prev, cache_ttl_secs: null }));
                    }}
                  >
                    <RotateCcw size={12} style={{ display: 'inline', marginRight: 4 }} />
                    {t('system.resetToDefault')}
                  </button>
                )}
              </div>
              <div className={styles.rowInputs}>
                <Field>
                  <input
                    id="cacheTtlSecs"
                    type="number"
                    min="0"
                    max="31536000"
                    value={cacheTtlSecs}
                    onChange={(e) => {
                      setCacheTtlSecs(e.target.value);
                      setOverridesState((prev) => ({
                        ...prev,
                        cache_ttl_secs: Number.parseInt(e.target.value, 10) || null,
                      }));
                    }}
                  />
                </Field>
                <span>秒 (seconds)</span>
              </div>
            </div>
          </div>

          {/* 4. Auth & Accounts */}
          <div className={styles.card}>
            <div className={styles.cardHeader}>
              <div className={styles.cardTitle}>
                <Shield size={18} />
                <span>{t('system.auth.heading')}</span>
              </div>
              {(overridesState.registration_enabled !== null ||
                overridesState.max_login_attempts !== null) && (
                <span className={styles.overrideBadge}>{t('system.activeOverride')}</span>
              )}
            </div>
            <p className={styles.hint}>{t('system.auth.hint')}</p>

            <div className={styles.fieldGroup}>
              <div className={styles.fieldHeader}>
                <label className={styles.toggleLabel}>
                  <input
                    type="checkbox"
                    className={styles.toggleInput}
                    checked={registrationEnabled}
                    onChange={(e) => {
                      setRegistrationEnabled(e.target.checked);
                      setOverridesState((prev) => ({
                        ...prev,
                        registration_enabled: e.target.checked,
                      }));
                    }}
                  />
                  <span>{t('system.auth.registrationEnable')}</span>
                </label>
                {overridesState.registration_enabled !== null && (
                  <button
                    type="button"
                    className={styles.resetBtn}
                    onClick={() => {
                      setRegistrationEnabled(data.defaults.registration_enabled);
                      setOverridesState((prev) => ({ ...prev, registration_enabled: null }));
                    }}
                  >
                    <RotateCcw size={12} style={{ display: 'inline', marginRight: 4 }} />
                    {t('system.resetToDefault')}
                  </button>
                )}
              </div>
            </div>

            <div className={styles.fieldGroup}>
              <div className={styles.fieldHeader}>
                <label className={styles.fieldLabel} htmlFor="maxLoginAttempts">
                  {t('system.auth.maxLoginAttemptsLabel')}{' '}
                  <span className={styles.hint}>
                    {t('system.defaultLabel', { value: data.defaults.max_login_attempts })}
                  </span>
                </label>
                {overridesState.max_login_attempts !== null && (
                  <button
                    type="button"
                    className={styles.resetBtn}
                    onClick={() => {
                      setMaxLoginAttempts(String(data.defaults.max_login_attempts));
                      setOverridesState((prev) => ({ ...prev, max_login_attempts: null }));
                    }}
                  >
                    <RotateCcw size={12} style={{ display: 'inline', marginRight: 4 }} />
                    {t('system.resetToDefault')}
                  </button>
                )}
              </div>
              <div className={styles.rowInputs}>
                <Field>
                  <input
                    id="maxLoginAttempts"
                    type="number"
                    min="0"
                    max="1000"
                    value={maxLoginAttempts}
                    onChange={(e) => {
                      setMaxLoginAttempts(e.target.value);
                      setOverridesState((prev) => ({
                        ...prev,
                        max_login_attempts: Number.parseInt(e.target.value, 10) || null,
                      }));
                    }}
                  />
                </Field>
                <span>回 (0 で無制限)</span>
              </div>
            </div>
          </div>

          {/* 5. Interactions (Comments & Forum) */}
          <div className={styles.card}>
            <div className={styles.cardHeader}>
              <div className={styles.cardTitle}>
                <MessageSquare size={18} />
                <span>{t('system.interactions.heading')}</span>
              </div>
              {(overridesState.comments_enabled !== null ||
                overridesState.forum_enabled !== null) && (
                <span className={styles.overrideBadge}>{t('system.activeOverride')}</span>
              )}
            </div>
            <p className={styles.hint}>{t('system.interactions.hint')}</p>

            <div className={styles.fieldGroup}>
              <div className={styles.fieldHeader}>
                <label className={styles.toggleLabel}>
                  <input
                    type="checkbox"
                    className={styles.toggleInput}
                    checked={commentsEnabled}
                    onChange={(e) => {
                      setCommentsEnabled(e.target.checked);
                      setOverridesState((prev) => ({
                        ...prev,
                        comments_enabled: e.target.checked,
                      }));
                    }}
                  />
                  <span>{t('system.interactions.commentsEnable')}</span>
                </label>
                {overridesState.comments_enabled !== null && (
                  <button
                    type="button"
                    className={styles.resetBtn}
                    onClick={() => {
                      setCommentsEnabled(data.defaults.comments_enabled);
                      setOverridesState((prev) => ({ ...prev, comments_enabled: null }));
                    }}
                  >
                    <RotateCcw size={12} style={{ display: 'inline', marginRight: 4 }} />
                    {t('system.resetToDefault')}
                  </button>
                )}
              </div>
            </div>

            <div className={styles.fieldGroup}>
              <div className={styles.fieldHeader}>
                <label className={styles.toggleLabel}>
                  <input
                    type="checkbox"
                    className={styles.toggleInput}
                    checked={forumEnabled}
                    onChange={(e) => {
                      setForumEnabled(e.target.checked);
                      setOverridesState((prev) => ({ ...prev, forum_enabled: e.target.checked }));
                    }}
                  />
                  <span>{t('system.interactions.forumEnable')}</span>
                </label>
                {overridesState.forum_enabled !== null && (
                  <button
                    type="button"
                    className={styles.resetBtn}
                    onClick={() => {
                      setForumEnabled(data.defaults.forum_enabled);
                      setOverridesState((prev) => ({ ...prev, forum_enabled: null }));
                    }}
                  >
                    <RotateCcw size={12} style={{ display: 'inline', marginRight: 4 }} />
                    {t('system.resetToDefault')}
                  </button>
                )}
              </div>
            </div>
          </div>

          <div className={styles.footer}>
            <Button type="submit" variant="primary" disabled={isSaving}>
              <Save size={16} />
              <span>{isSaving ? t('system.saving') : t('system.save')}</span>
            </Button>
          </div>
        </form>
      </div>
    </main>
  );
};
