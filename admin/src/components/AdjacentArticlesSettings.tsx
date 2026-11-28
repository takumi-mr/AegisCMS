import React, { useEffect, useState } from 'react';
import { ArrowLeftRight, Save } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi } from '../api/client';
import type { AdjacentArticlesSettings as Settings, AdjacentOrder } from '../types';
import { Button } from './ui/Button';
import { Field } from './ui/Field';
import styles from './SiteManager.module.css';

interface AdjacentArticlesSettingsProps {
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

/** `''` = use the configuration file's value (sent as `null`) */
type Choice = '' | 'true' | 'false';

const toChoice = (value: boolean | null): Choice =>
  value === null ? '' : value ? 'true' : 'false';
const fromChoice = (choice: Choice): boolean | null => (choice === '' ? null : choice === 'true');

/**
 * How the previous and next articles are picked (`GET` / `PUT /api/v1/site/adjacent-articles`,
 * admin only). Themes get them as `prev_article` / `next_article`, and so do plugins. Saved
 * values override `[adjacent_articles]` and apply without restarting.
 */
export const AdjacentArticlesSettings: React.FC<AdjacentArticlesSettingsProps> = ({
  onShowToast,
}) => {
  const { t } = useTranslation();
  const [settings, setSettings] = useState<Settings | null>(null);
  const [sameCategory, setSameCategory] = useState<Choice>('');
  const [fallbackToAll, setFallbackToAll] = useState<Choice>('');
  const [orderBy, setOrderBy] = useState<'' | AdjacentOrder>('');
  const [isSaving, setIsSaving] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  const apply = (loaded: Settings) => {
    setSettings(loaded);
    setSameCategory(toChoice(loaded.same_category));
    setFallbackToAll(toChoice(loaded.fallback_to_all));
    setOrderBy(loaded.order_by ?? '');
  };

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .getAdjacentArticlesSettings()
      .then((loaded) => {
        if (!cancelled) apply(loaded);
      })
      .catch((err: unknown) => {
        if (!cancelled)
          setError(err instanceof Error ? err.message : t('site.adjacent.loadFailed'));
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
        await aegisApi.updateAdjacentArticlesSettings({
          same_category: fromChoice(sameCategory),
          fallback_to_all: fromChoice(fallbackToAll),
          order_by: orderBy || null,
        })
      );
      onShowToast(t('site.adjacent.saved'));
    } catch (err) {
      setError(err instanceof Error ? err.message : t('site.adjacent.saveFailed'));
    } finally {
      setIsSaving(false);
    }
  };

  const isDirty =
    settings !== null &&
    (sameCategory !== toChoice(settings.same_category) ||
      fallbackToAll !== toChoice(settings.fallback_to_all) ||
      orderBy !== (settings.order_by ?? ''));
  const yesNo = (value: boolean | undefined) =>
    value === undefined ? '…' : value ? t('site.adjacent.yes') : t('site.adjacent.no');
  const orderLabel = (order: AdjacentOrder | undefined) =>
    order === undefined ? '…' : t(`site.adjacent.order.${order}`);
  const defaults = settings?.defaults;

  return (
    <form
      className={styles.form}
      onSubmit={handleSubmit}
      aria-labelledby="adjacent-articles-heading"
    >
      <div className={styles.section}>
        <div className={styles.sectionTitle} id="adjacent-articles-heading">
          <ArrowLeftRight size={14} aria-hidden="true" /> {t('site.adjacent.heading')}
        </div>
        <p className={styles.hint}>{t('site.adjacent.description')}</p>

        <label className={styles.hint} htmlFor="adjacent-same-category">
          {t('site.adjacent.sameCategory')}
        </label>
        <Field>
          <select
            id="adjacent-same-category"
            value={sameCategory}
            onChange={(e) => setSameCategory(e.target.value as Choice)}
            disabled={!settings}
          >
            <option value="">
              {t('site.adjacent.useDefault', { value: yesNo(defaults?.same_category) })}
            </option>
            <option value="true">{t('site.adjacent.yes')}</option>
            <option value="false">{t('site.adjacent.no')}</option>
          </select>
        </Field>

        <label className={styles.hint} htmlFor="adjacent-fallback">
          {t('site.adjacent.fallbackToAll')}
        </label>
        <Field>
          <select
            id="adjacent-fallback"
            value={fallbackToAll}
            onChange={(e) => setFallbackToAll(e.target.value as Choice)}
            disabled={!settings}
          >
            <option value="">
              {t('site.adjacent.useDefault', { value: yesNo(defaults?.fallback_to_all) })}
            </option>
            <option value="true">{t('site.adjacent.yes')}</option>
            <option value="false">{t('site.adjacent.no')}</option>
          </select>
        </Field>

        <label className={styles.hint} htmlFor="adjacent-order">
          {t('site.adjacent.orderBy')}
        </label>
        <Field>
          <select
            id="adjacent-order"
            value={orderBy}
            onChange={(e) => setOrderBy(e.target.value as '' | AdjacentOrder)}
            disabled={!settings}
          >
            <option value="">
              {t('site.adjacent.useDefault', { value: orderLabel(defaults?.order_by) })}
            </option>
            <option value="published_at">{t('site.adjacent.order.published_at')}</option>
            <option value="created_at">{t('site.adjacent.order.created_at')}</option>
          </select>
        </Field>
      </div>

      {error && <div className={styles.error}>{error}</div>}
      <div className={styles.formActions}>
        <Button type="submit" variant="primary" disabled={!settings || !isDirty || isSaving}>
          <Save size={16} />
          <span>{isSaving ? t('site.saving') : t('site.adjacent.save')}</span>
        </Button>
      </div>
    </form>
  );
};
