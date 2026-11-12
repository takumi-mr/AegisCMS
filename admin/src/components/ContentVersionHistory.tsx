import React from 'react';
import { useTranslation } from 'react-i18next';
import { formatDateTime } from '../utils/format';
import { Button } from './ui/Button';
import styles from './ContentVersionHistory.module.css';

export interface ContentVersionSummary {
  revision: number;
  created_at: string;
  created_by?: string;
  change_note?: string;
}

interface ContentVersionHistoryProps {
  versions: ContentVersionSummary[];
  loading?: boolean;
  /** Opens the version's Preview and its diff against the editor */
  onPreview?: (revision: number) => void;
  onRestore: (revision: number) => void;
  /** False hides the restore buttons (the content cannot be changed by this user) */
  canRestore?: boolean;
}

export const ContentVersionHistory: React.FC<ContentVersionHistoryProps> = ({
  versions,
  loading = false,
  onPreview,
  onRestore,
  canRestore = true,
}) => {
  const { t } = useTranslation();

  return (
    <section className={styles.history} aria-labelledby="content-version-history-title">
      <h2 id="content-version-history-title">{t('editor.versionHistory')}</h2>
      {loading ? (
        <p role="status">{t('editor.loadingVersions')}</p>
      ) : versions.length === 0 ? (
        <p className={styles.empty}>{t('editor.noVersions')}</p>
      ) : (
        <ol className={styles.list}>
          {versions.map((version) => (
            <li className={styles.item} key={version.revision}>
              <div className={styles.details}>
                <strong>{t('editor.versionLabel', { revision: version.revision })}</strong>
                <time dateTime={version.created_at}>{formatDateTime(version.created_at)}</time>
                {version.created_by && <span>{version.created_by}</span>}
                {version.change_note && <span>{version.change_note}</span>}
              </div>
              <div className={styles.actions}>
                {onPreview && (
                  <Button
                    variant="tool"
                    aria-label={t('editor.previewVersionAria', { revision: version.revision })}
                    onClick={() => onPreview(version.revision)}
                  >
                    {t('editor.previewVersion')}
                  </Button>
                )}
                {canRestore && (
                  <Button
                    variant="tool"
                    aria-label={t('editor.restoreVersionAria', { revision: version.revision })}
                    onClick={() => onRestore(version.revision)}
                  >
                    {t('editor.restoreVersion')}
                  </Button>
                )}
              </div>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
};
