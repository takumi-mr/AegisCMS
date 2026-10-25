import React, { useEffect, useState } from 'react';
import {
  AlertTriangle,
  Bot,
  ExternalLink,
  ImageIcon,
  Images,
  Save,
  Search,
  Share2,
  Sparkles,
  Upload,
  X,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi, serverAssetUrl } from '../api/client';
import type { SeoSettings } from '../types';
import { Button } from './ui/Button';
import { Field } from './ui/Field';
import { ImageUploader } from './ImageUploader';
import { MediaLibrary } from './MediaLibrary';
import styles from './SeoManager.module.css';

interface SeoManagerProps {
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

const AI_BOTS_ROBOTS_TXT = `User-agent: GPTBot
Disallow: /

User-agent: ChatGPT-User
Disallow: /

User-agent: CCBot
Disallow: /

User-agent: anthropic-ai
Disallow: /

User-agent: Claude-Web
Disallow: /

User-agent: Google-Extended
Disallow: /

User-agent: Bytespider
Disallow: /

User-agent: PerplexityBot
Disallow: /

User-agent: *
Disallow: /admin/
Disallow: /api/
Allow: /
`;

const STANDARD_ROBOTS_TXT = `User-agent: *
Disallow: /admin/
Disallow: /api/
Allow: /
`;

const DISALLOW_ALL_ROBOTS_TXT = `User-agent: *
Disallow: /
`;

export const SeoManager: React.FC<SeoManagerProps> = ({ onShowToast }) => {
  const { t } = useTranslation();
  const [settings, setSettings] = useState<SeoSettings | null>(null);

  const [noindexAll, setNoindexAll] = useState<boolean>(false);
  const [canonicalBaseUrl, setCanonicalBaseUrl] = useState<string>('');
  const [metaDescription, setMetaDescription] = useState<string>('');
  const [metaKeywords, setMetaKeywords] = useState<string>('');
  const [ogImage, setOgImage] = useState<string>('');
  const [twitterCard, setTwitterCard] = useState<'summary' | 'summary_large_image'>(
    'summary_large_image'
  );
  const [robotsTxt, setRobotsTxt] = useState<string>('');

  const [isLibraryOpen, setIsLibraryOpen] = useState<boolean>(false);
  const [isUploadOpen, setIsUploadOpen] = useState<boolean>(false);
  const [isSaving, setIsSaving] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  const apply = (loaded: SeoSettings) => {
    setSettings(loaded);
    setNoindexAll(loaded.noindex_all);
    setCanonicalBaseUrl(loaded.canonical_base_url ?? '');
    setMetaDescription(loaded.meta_description ?? '');
    setMetaKeywords(loaded.meta_keywords ?? '');
    setOgImage(loaded.og_image ?? '');
    setTwitterCard(loaded.twitter_card === 'summary' ? 'summary' : 'summary_large_image');
    setRobotsTxt(loaded.robots_txt ?? loaded.defaults.robots_txt);
  };

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .getSeoSettings()
      .then((loaded) => {
        if (!cancelled) apply(loaded);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(err instanceof Error ? err.message : t('seo.loadFailed'));
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
      const updated = await aegisApi.updateSeoSettings({
        noindex_all: noindexAll,
        canonical_base_url: canonicalBaseUrl.trim() || null,
        meta_description: metaDescription.trim() || null,
        meta_keywords: metaKeywords.trim() || null,
        og_image: ogImage.trim() || null,
        twitter_card: twitterCard,
        robots_txt: robotsTxt.trim() ? robotsTxt : null,
      });
      apply(updated);
      onShowToast(t('seo.saved'));
    } catch (err) {
      setError(err instanceof Error ? err.message : t('seo.saveFailed'));
    } finally {
      setIsSaving(false);
    }
  };

  const candidateOg = ogImage.trim();
  const shownOgImage = /^(\/(?!\/)|https?:\/\/)/i.test(candidateOg) ? candidateOg : null;

  const isDirty =
    settings !== null &&
    (noindexAll !== settings.noindex_all ||
      canonicalBaseUrl.trim() !== (settings.canonical_base_url ?? '') ||
      metaDescription.trim() !== (settings.meta_description ?? '') ||
      metaKeywords.trim() !== (settings.meta_keywords ?? '') ||
      ogImage.trim() !== (settings.og_image ?? '') ||
      twitterCard !== (settings.twitter_card ?? 'summary_large_image') ||
      robotsTxt.trim() !== (settings.robots_txt ?? settings.defaults.robots_txt).trim());

  return (
    <main className={styles.main}>
      <div className={styles.column}>
        <div className={styles.title}>
          <Search size={22} />
          <h1>{t('seo.title')}</h1>
        </div>
        <p className={styles.description}>{t('seo.description')}</p>

        <form className={styles.form} onSubmit={handleSubmit}>
          {/* Card 1: 検索エンジン・インデックス制御 */}
          <div className={styles.card}>
            <div className={styles.cardHeader}>
              <div className={styles.cardTitle}>
                <Search size={18} />
                <span>{t('seo.indexingHeading')}</span>
              </div>
            </div>

            {/* noindex_all トグル */}
            <div className={styles.fieldGroup}>
              <label className={styles.toggleLabel}>
                <input
                  type="checkbox"
                  className={styles.toggleInput}
                  checked={noindexAll}
                  onChange={(e) => setNoindexAll(e.target.checked)}
                  disabled={!settings}
                />
                <span>{t('seo.noindexLabel')}</span>
              </label>
              {noindexAll && (
                <div className={styles.warningBanner} role="alert">
                  <AlertTriangle size={18} />
                  <span>{t('seo.noindexWarning')}</span>
                </div>
              )}
            </div>

            {/* Canonical Base URL */}
            <div className={styles.fieldGroup}>
              <div className={styles.fieldHeader}>
                <label className={styles.fieldTitle}>{t('seo.canonicalUrlLabel')}</label>
              </div>
              <Field>
                <input
                  type="url"
                  value={canonicalBaseUrl}
                  onChange={(e) => setCanonicalBaseUrl(e.target.value)}
                  placeholder="https://example.com"
                  maxLength={2048}
                  disabled={!settings}
                />
              </Field>
              <p className={styles.hint}>{t('seo.canonicalUrlHint')}</p>
            </div>

            {/* Meta Description */}
            <div className={styles.fieldGroup}>
              <div className={styles.fieldHeader}>
                <label className={styles.fieldTitle}>{t('seo.metaDescriptionLabel')}</label>
                <span className={styles.counter}>{metaDescription.length} / 500</span>
              </div>
              <Field>
                <textarea
                  className={styles.textarea}
                  style={{ minHeight: '80px' }}
                  value={metaDescription}
                  onChange={(e) => setMetaDescription(e.target.value)}
                  placeholder={settings?.defaults.meta_description}
                  maxLength={500}
                  disabled={!settings}
                />
              </Field>
              <p className={styles.hint}>{t('seo.metaDescriptionHint')}</p>
            </div>

            {/* Meta Keywords */}
            <div className={styles.fieldGroup}>
              <div className={styles.fieldHeader}>
                <label className={styles.fieldTitle}>{t('seo.metaKeywordsLabel')}</label>
              </div>
              <Field>
                <input
                  value={metaKeywords}
                  onChange={(e) => setMetaKeywords(e.target.value)}
                  placeholder="cms, rust, secure"
                  maxLength={500}
                  disabled={!settings}
                />
              </Field>
              <p className={styles.hint}>{t('seo.metaKeywordsHint')}</p>
            </div>
          </div>

          {/* Card 2: ソーシャル共有 & OGP */}
          <div className={styles.card}>
            <div className={styles.cardHeader}>
              <div className={styles.cardTitle}>
                <Share2 size={18} />
                <span>{t('seo.ogpHeading')}</span>
              </div>
            </div>

            {/* OGP 画像 */}
            <div className={styles.fieldGroup}>
              <div className={styles.fieldHeader}>
                <label className={styles.fieldTitle}>{t('seo.ogImageLabel')}</label>
              </div>
              <div className={styles.imageRow}>
                <div className={styles.imagePreview} aria-hidden="true">
                  {shownOgImage ? (
                    <img src={serverAssetUrl(shownOgImage)} alt="" />
                  ) : (
                    <ImageIcon size={20} />
                  )}
                </div>
                <Field className={styles.imageField}>
                  <input
                    value={ogImage}
                    onChange={(e) => setOgImage(e.target.value)}
                    placeholder="/uploads/og-default.png"
                    disabled={!settings}
                  />
                </Field>
                {ogImage && (
                  <Button
                    variant="tool"
                    onClick={() => setOgImage('')}
                    title={t('seo.clearImage')}
                    aria-label={t('seo.clearImage')}
                  >
                    <X size={14} />
                  </Button>
                )}
              </div>
              <div className={styles.imageActions}>
                <Button variant="tool" onClick={() => setIsLibraryOpen(true)} disabled={!settings}>
                  <Images size={14} />
                  <span>{t('seo.chooseFromLibrary')}</span>
                </Button>
                <Button
                  variant="tool"
                  onClick={() => setIsUploadOpen((open) => !open)}
                  disabled={!settings}
                  aria-expanded={isUploadOpen}
                >
                  <Upload size={14} />
                  <span>{t('seo.uploadImage')}</span>
                </Button>
              </div>
              {isUploadOpen && (
                <ImageUploader
                  onUploaded={(meta) => {
                    setOgImage(meta.url);
                    setIsUploadOpen(false);
                    onShowToast(t('seo.imageUploaded'));
                  }}
                />
              )}
              <p className={styles.hint}>{t('seo.ogImageHint')}</p>
            </div>

            {/* Twitter Card */}
            <div className={styles.fieldGroup}>
              <div className={styles.fieldHeader}>
                <label className={styles.fieldTitle}>{t('seo.twitterCardLabel')}</label>
              </div>
              <select
                className={styles.select}
                value={twitterCard}
                onChange={(e) =>
                  setTwitterCard(e.target.value as 'summary' | 'summary_large_image')
                }
                disabled={!settings}
              >
                <option value="summary_large_image">
                  {t('seo.twitterCardLarge')} (summary_large_image)
                </option>
                <option value="summary">{t('seo.twitterCardSummary')} (summary)</option>
              </select>
              <p className={styles.hint}>{t('seo.twitterCardHint')}</p>
            </div>
          </div>

          {/* Card 3: クローラー & Bot 制御 (robots.txt) */}
          <div className={styles.card}>
            <div className={styles.cardHeader}>
              <div className={styles.cardTitle}>
                <Bot size={18} />
                <span>{t('seo.robotsHeading')}</span>
              </div>
            </div>
            <p className={styles.cardDescription}>{t('seo.robotsDescription')}</p>

            <div className={styles.presetBar}>
              <span className={styles.presetLabel}>{t('seo.presetLabel')}:</span>
              <Button
                variant="tool"
                type="button"
                onClick={() => setRobotsTxt(STANDARD_ROBOTS_TXT)}
                title={t('seo.presetStandardHint')}
              >
                <Sparkles size={14} />
                <span>{t('seo.presetStandard')}</span>
              </Button>
              <Button
                variant="tool"
                type="button"
                onClick={() => setRobotsTxt(AI_BOTS_ROBOTS_TXT)}
                title={t('seo.presetBlockAiHint')}
              >
                <Bot size={14} />
                <span>{t('seo.presetBlockAi')}</span>
              </Button>
              <Button
                variant="tool"
                type="button"
                onClick={() => setRobotsTxt(DISALLOW_ALL_ROBOTS_TXT)}
                title={t('seo.presetDisallowAllHint')}
              >
                <AlertTriangle size={14} />
                <span>{t('seo.presetDisallowAll')}</span>
              </Button>
            </div>

            <div className={styles.fieldGroup}>
              <textarea
                className={styles.textarea}
                value={robotsTxt}
                onChange={(e) => setRobotsTxt(e.target.value)}
                placeholder={settings?.defaults.robots_txt}
                rows={10}
                spellCheck={false}
                disabled={!settings}
              />
            </div>

            <div className={styles.linksBar}>
              <a
                href="/robots.txt"
                target="_blank"
                rel="noopener noreferrer"
                className={styles.quickLink}
              >
                <span>{t('seo.viewRobotsTxt')}</span>
                <ExternalLink size={12} />
              </a>
              <a
                href="/sitemap.xml"
                target="_blank"
                rel="noopener noreferrer"
                className={styles.quickLink}
              >
                <span>{t('seo.viewSitemapXml')}</span>
                <ExternalLink size={12} />
              </a>
            </div>
          </div>

          {error && <div className={styles.error}>{error}</div>}

          <div className={styles.formActions}>
            {isDirty && <span className={styles.dirtyBadge}>{t('seo.unsavedChanges')}</span>}
            <Button type="submit" variant="primary" disabled={!settings || !isDirty || isSaving}>
              <Save size={16} />
              <span>{isSaving ? t('seo.saving') : t('seo.save')}</span>
            </Button>
          </div>
        </form>
      </div>

      {isLibraryOpen && (
        <MediaLibrary
          onSelect={(obj) => {
            setOgImage(obj.url);
            setIsLibraryOpen(false);
          }}
          onClose={() => setIsLibraryOpen(false)}
        />
      )}
    </main>
  );
};
