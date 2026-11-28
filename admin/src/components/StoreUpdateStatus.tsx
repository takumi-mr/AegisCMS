import React from 'react';
import { AlertTriangle, ArrowUpCircle, CheckCircle, RefreshCw } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { StorePackageKind } from '../types';
import { lastChecked, updatesOf, useStoreUpdates } from '../storeUpdates';
import { storeErrorText } from '../utils/storeErrors';
import { Button } from './ui/Button';
import styles from './StoreUpdateStatus.module.css';

interface StoreUpdateStatusProps {
  kind: StorePackageKind;
  /** Opens the Store tab, where updates are installed (with their confirmation) */
  onOpenStore: () => void;
}

/**
 * The update check for the Installed tab: how many updates there are, when the catalogs were
 * last checked, and why a check failed (never "up to date" without a verified catalog)
 */
export const StoreUpdateStatus: React.FC<StoreUpdateStatusProps> = ({ kind, onOpenStore }) => {
  const { t, i18n } = useTranslation();
  const state = useStoreUpdates();
  if (state.status === 'idle' || state.status === 'disabled') return null;

  const catalogs = state.result?.catalogs ?? [];
  const updates = updatesOf(state, kind);
  const checked = lastChecked(catalogs);
  const when = (at: string) => new Date(at).toLocaleString(i18n.language);
  const failures = catalogs.filter((c) => c.code);
  const loading = state.status === 'loading';

  let summary: React.ReactNode;
  if (state.status === 'error') {
    summary = (
      <span className={styles.warning}>
        <AlertTriangle size={14} aria-hidden="true" />
        {t('store.updates.requestFailed', { reason: state.error })}
      </span>
    );
  } else if (!state.result) {
    summary = <span>{t('store.updates.checking')}</span>;
  } else if (updates.length > 0) {
    summary = (
      <span className={styles.available}>
        <ArrowUpCircle size={14} aria-hidden="true" />
        {t('store.updates.available', { count: updates.length })}
        <Button variant="tool" onClick={onOpenStore}>
          {t('store.updates.openStore')}
        </Button>
      </span>
    );
  } else if (checked) {
    summary = (
      <span>
        <CheckCircle size={14} aria-hidden="true" />
        {t('store.updates.upToDate', { when: when(checked) })}
      </span>
    );
  } else {
    summary = <span>{t('store.updates.notChecked')}</span>;
  }

  return (
    <div className={styles.status} role="status">
      {summary}
      {failures.map((c) => (
        <span key={c.name} className={styles.warning}>
          <AlertTriangle size={14} aria-hidden="true" />
          {c.checked_at
            ? t('store.updates.catalogFailedWithLast', {
                name: c.name,
                reason: storeErrorText(c.code, c.error),
                when: when(c.checked_at),
              })
            : t('store.updates.catalogFailed', {
                name: c.name,
                reason: storeErrorText(c.code, c.error),
              })}
        </span>
      ))}
      <Button
        variant="tool"
        onClick={() => void state.refresh(true)}
        disabled={loading}
        className={styles.check}
      >
        <RefreshCw size={13} className={loading ? 'animate-spin' : ''} aria-hidden="true" />
        <span>{loading ? t('store.updates.checkingNow') : t('store.updates.checkNow')}</span>
      </Button>
    </div>
  );
};
