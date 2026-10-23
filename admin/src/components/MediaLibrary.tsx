import React, { useEffect, useRef, useState } from 'react';
import { Images, Loader2, RefreshCw, Search, Trash2 } from 'lucide-react';
import { Trans, useTranslation } from 'react-i18next';
import { aegisApi } from '../api/client';
import i18n from '../i18n';
import type { ObjectReference, StorageObject } from '../types';
import { formatDate } from '../utils/format';
import { fileNameFromKey, formatSize } from '../utils/media';
import { Button } from './ui/Button';
import { Field } from './ui/Field';
import { Modal } from './ui/Modal';
import styles from './MediaLibrary.module.css';

const PAGE_SIZE = 24;
const SEARCH_DEBOUNCE_MS = 300;

interface MediaLibraryProps {
  /** Called with the chosen file; the parent closes the library */
  onSelect: (object: StorageObject) => void;
  onClose: () => void;
  /** Show delete buttons (deleting needs the editor role) */
  canDelete?: boolean;
}

interface PendingDelete {
  object: StorageObject;
  /** Contents still using the file (the server refused to delete it), or null before asking */
  references: ObjectReference[] | null;
  busy: boolean;
  error: string | null;
}

/**
 * Modal that lists uploaded images (`GET /api/v1/storage/objects`, newest first)
 * so an image block can reuse an existing file instead of uploading it again.
 * Editors can also delete files (`DELETE /api/v1/storage/objects/{key}`); a file that saved
 * contents still use is only deleted after a second confirmation.
 */
export const MediaLibrary: React.FC<MediaLibraryProps> = ({
  onSelect,
  onClose,
  canDelete = false,
}) => {
  const { t } = useTranslation();
  const [search, setSearch] = useState<string>('');
  const [query, setQuery] = useState<string>('');
  // Offset of the latest requested page; 0 starts a fresh listing, more appends
  const [offset, setOffset] = useState<number>(0);
  const [reloadCount, setReloadCount] = useState<number>(0);
  const [result, setResult] = useState<{
    key: string;
    objects: StorageObject[];
    total: number;
    truncated: boolean;
    error: string | null;
  } | null>(null);
  const [pendingDelete, setPendingDelete] = useState<PendingDelete | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    searchRef.current?.focus();
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      setQuery(search.trim());
      setOffset(0);
    }, SEARCH_DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [search]);

  // A request is identified by its query, offset and reload counter; loading = no result for it yet
  const requestKey = JSON.stringify([query, offset, reloadCount]);
  const isLoading = result?.key !== requestKey;
  const objects = result?.objects ?? [];
  const total = result?.total ?? 0;
  const error = result?.key === requestKey ? result.error : null;

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .listStorageObjects({ content_type: 'image/', q: query, limit: PAGE_SIZE, offset })
      .then((page) => {
        if (cancelled) return;
        setResult((prev) => ({
          key: requestKey,
          objects: offset === 0 ? page.objects : [...(prev?.objects ?? []), ...page.objects],
          total: page.total,
          truncated: page.truncated,
          error: null,
        }));
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setResult((prev) => ({
          key: requestKey,
          objects: offset === 0 ? [] : (prev?.objects ?? []),
          total: prev?.total ?? 0,
          truncated: prev?.truncated ?? false,
          error: err instanceof Error ? err.message : i18n.t('api.mediaFailed'),
        }));
      });
    return () => {
      cancelled = true;
    };
  }, [query, offset, requestKey]);

  const reload = () => {
    setOffset(0);
    setReloadCount((n) => n + 1);
  };

  const askDelete = (object: StorageObject) => {
    setNotice(null);
    setPendingDelete({ object, references: null, busy: false, error: null });
  };

  const confirmDelete = async () => {
    if (!pendingDelete) return;
    const { object, references } = pendingDelete;
    setPendingDelete({ ...pendingDelete, busy: true, error: null });
    try {
      // The second confirmation (after the server listed the contents using it) forces it
      const result = await aegisApi.deleteStorageObject(object.key, references !== null);
      if (!result.deleted) {
        setPendingDelete({ object, references: result.references, busy: false, error: null });
        return;
      }
      setResult((prev) =>
        prev
          ? {
              ...prev,
              objects: prev.objects.filter((o) => o.key !== object.key),
              total: Math.max(prev.total - 1, 0),
            }
          : prev
      );
      setPendingDelete(null);
      setNotice(t('media.deleted', { name: fileNameFromKey(object.key) }));
    } catch (err: unknown) {
      setPendingDelete({
        object,
        references,
        busy: false,
        error: err instanceof Error ? err.message : t('api.deleteFileFailed'),
      });
    }
  };

  return (
    <Modal
      title={t('media.title')}
      icon={<Images size={18} />}
      onClose={onClose}
      wide
      actions={
        <Button
          variant="icon"
          onClick={reload}
          disabled={isLoading}
          aria-label={t('common.reload')}
          title={t('common.reload')}
        >
          <RefreshCw size={16} />
        </Button>
      }
    >
      <div className={styles.toolbar}>
        <Field className={styles.searchField}>
          <Search size={14} />
          <input
            ref={searchRef}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t('media.searchPlaceholder')}
            aria-label={t('media.searchAria')}
          />
        </Field>
        <span className={styles.count}>{t('media.count', { shown: objects.length, total })}</span>
      </div>

      <div className={styles.body}>
        {pendingDelete && (
          <div className={styles.confirm} role="alertdialog" aria-label={t('media.deleteAria')}>
            <p>
              <Trans
                i18nKey="media.confirm"
                values={{ name: fileNameFromKey(pendingDelete.object.key) }}
                components={{ strong: <strong /> }}
              />
            </p>
            {pendingDelete.references && (
              <>
                <p className={styles.warning}>
                  {t('media.inUse', { count: pendingDelete.references.length })}
                </p>
                <ul className={styles.references}>
                  {pendingDelete.references.map((ref) => (
                    <li key={`${ref.content_type}/${ref.id}`}>
                      {ref.title || ref.slug}{' '}
                      <span className={styles.referenceMeta}>
                        ({ref.content_type}/{ref.slug})
                      </span>
                    </li>
                  ))}
                </ul>
              </>
            )}
            {pendingDelete.error && <div className={styles.error}>{pendingDelete.error}</div>}
            <div className={styles.confirmActions}>
              <Button onClick={() => setPendingDelete(null)} disabled={pendingDelete.busy}>
                {t('common.cancel')}
              </Button>
              <Button
                variant="primary"
                className={styles.deleteButton}
                onClick={confirmDelete}
                disabled={pendingDelete.busy}
              >
                {pendingDelete.busy && <Loader2 size={14} className="animate-spin" />}
                {pendingDelete.references ? t('media.deleteAnyway') : t('common.delete')}
              </Button>
            </div>
          </div>
        )}
        {notice && (
          <div className={styles.notice} role="status">
            {notice}
          </div>
        )}
        {error && <div className={styles.error}>{error}</div>}
        {!error && !isLoading && objects.length === 0 && (
          <div className={styles.empty}>
            {query ? t('media.noMatch', { query }) : t('media.empty')}
          </div>
        )}
        <div className={styles.grid}>
          {objects.map((object) => (
            <div key={object.key} className={styles.itemWrap}>
              <button
                type="button"
                className={styles.item}
                onClick={() => onSelect(object)}
                title={object.key}
              >
                <img src={object.url} alt="" loading="lazy" decoding="async" />
                <span className={styles.itemName}>{fileNameFromKey(object.key)}</span>
                <span className={styles.itemMeta}>
                  {formatSize(object.size)}
                  {object.last_modified && ` · ${formatDate(object.last_modified)}`}
                </span>
              </button>
              {canDelete && (
                <Button
                  variant="icon"
                  danger
                  className={styles.itemDelete}
                  onClick={() => askDelete(object)}
                  disabled={pendingDelete?.busy}
                  aria-label={t('media.deleteFile', { name: fileNameFromKey(object.key) })}
                  title={t('common.delete')}
                >
                  <Trash2 size={14} />
                </Button>
              )}
            </div>
          ))}
        </div>
        {isLoading && (
          <div className={styles.loading}>
            <Loader2 size={18} className="animate-spin" />
            <span>{t('common.loading')}</span>
          </div>
        )}
        {!isLoading && objects.length < total && (
          <div className={styles.more}>
            <Button onClick={() => setOffset(objects.length)}>{t('media.loadMore')}</Button>
          </div>
        )}
        {result?.truncated && <div className={styles.note}>{t('media.truncated')}</div>}
      </div>
    </Modal>
  );
};
