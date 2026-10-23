import React, { useEffect, useState } from 'react';
import { Globe, ImageIcon, Images, Save, Upload, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi, serverAssetUrl } from '../api/client';
import type { SiteSettings } from '../types';
import { Button } from './ui/Button';
import { Field } from './ui/Field';
import { ImageUploader } from './ImageUploader';
import { MediaLibrary } from './MediaLibrary';
import styles from './SiteManager.module.css';

interface SiteManagerProps {
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

/**
 * Name and favicon of the public site (`GET` / `PUT /api/v1/site`, admin only). Saved values
 * override `[theme].site_title` / `favicon` and apply without restarting; an empty field goes
 * back to the configuration file's value.
 */
export const SiteManager: React.FC<SiteManagerProps> = ({ onShowToast }) => {
  const { t } = useTranslation();
  const [settings, setSettings] = useState<SiteSettings | null>(null);
  const [title, setTitle] = useState<string>('');
  const [favicon, setFavicon] = useState<string>('');
  const [isLibraryOpen, setIsLibraryOpen] = useState<boolean>(false);
  const [isUploadOpen, setIsUploadOpen] = useState<boolean>(false);
  const [isSaving, setIsSaving] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  const apply = (loaded: SiteSettings) => {
    setSettings(loaded);
    setTitle(loaded.title ?? '');
    setFavicon(loaded.favicon ?? '');
  };

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .getSiteSettings()
      .then((loaded) => {
        if (!cancelled) apply(loaded);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(err instanceof Error ? err.message : t('site.loadFailed'));
      });
    return () => {
      cancelled = true;
    };
  }, [t]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsSaving(true);
    setError(null);
    try {
      apply(
        await aegisApi.updateSiteSettings({
          title: title.trim() || null,
          favicon: favicon.trim() || null,
        })
      );
      onShowToast(t('site.saved'));
    } catch (err) {
      setError(err instanceof Error ? err.message : t('site.saveFailed'));
    } finally {
      setIsSaving(false);
    }
  };

  // Preview only what the server accepts (a path from the root or an http(s) URL)
  const candidate = favicon.trim() || settings?.defaults.favicon || '';
  const shownFavicon = /^(\/(?!\/)|https?:\/\/)/i.test(candidate) ? candidate : null;
  const isDirty =
    settings !== null &&
    (title.trim() !== (settings.title ?? '') || favicon.trim() !== (settings.favicon ?? ''));

  return (
    <main className={styles.main}>
      <div className={styles.column}>
        <div className={styles.title}>
          <Globe size={22} />
          <h1>{t('site.title')}</h1>
        </div>
        <p className={styles.description}>{t('site.description')}</p>

        <form className={styles.form} onSubmit={handleSubmit}>
          <div className={styles.section}>
            <div className={styles.sectionTitle}>{t('site.nameHeading')}</div>
            <Field>
              <input
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                placeholder={settings?.defaults.title}
                maxLength={200}
                aria-label={t('site.nameAria')}
                disabled={!settings}
              />
            </Field>
            <p className={styles.hint}>
              {t('site.defaultHint', { value: settings?.defaults.title ?? '…' })}
            </p>
          </div>

          <div className={styles.section}>
            <div className={styles.sectionTitle}>{t('site.faviconHeading')}</div>
            <div className={styles.faviconRow}>
              <div className={styles.faviconPreview} aria-hidden="true">
                {shownFavicon ? (
                  <img src={serverAssetUrl(shownFavicon)} alt="" />
                ) : (
                  <ImageIcon size={18} />
                )}
              </div>
              <Field className={styles.faviconField}>
                <input
                  value={favicon}
                  onChange={(e) => setFavicon(e.target.value)}
                  placeholder={settings?.defaults.favicon ?? '/uploads/favicon.png'}
                  aria-label={t('site.faviconAria')}
                  disabled={!settings}
                />
              </Field>
              {favicon && (
                <Button
                  variant="tool"
                  onClick={() => setFavicon('')}
                  title={t('site.clearFavicon')}
                  aria-label={t('site.clearFavicon')}
                >
                  <X size={14} />
                </Button>
              )}
            </div>
            <div className={styles.faviconActions}>
              <Button variant="tool" onClick={() => setIsLibraryOpen(true)} disabled={!settings}>
                <Images size={14} />
                <span>{t('site.chooseFromLibrary')}</span>
              </Button>
              <Button
                variant="tool"
                onClick={() => setIsUploadOpen((open) => !open)}
                disabled={!settings}
                aria-expanded={isUploadOpen}
              >
                <Upload size={14} />
                <span>{t('site.upload')}</span>
              </Button>
            </div>
            {isUploadOpen && (
              <ImageUploader
                onUploaded={(meta) => {
                  setFavicon(meta.url);
                  setIsUploadOpen(false);
                }}
              />
            )}
            <p className={styles.hint}>
              {settings?.defaults.favicon
                ? t('site.defaultHint', { value: settings.defaults.favicon })
                : t('site.faviconHint')}
            </p>
          </div>

          {error && <div className={styles.error}>{error}</div>}
          <div className={styles.formActions}>
            <Button type="submit" variant="primary" disabled={!settings || !isDirty || isSaving}>
              <Save size={16} />
              <span>{isSaving ? t('site.saving') : t('site.save')}</span>
            </Button>
          </div>
        </form>
      </div>

      {isLibraryOpen && (
        <MediaLibrary
          onSelect={(object) => {
            setFavicon(object.url);
            setIsLibraryOpen(false);
          }}
          onClose={() => setIsLibraryOpen(false)}
        />
      )}
    </main>
  );
};
