import React, { useEffect, useRef, useState } from 'react';
import { Images, Loader2, RefreshCw, Search, X } from 'lucide-react';
import { aegisApi } from '../api/client';
import type { StorageObject } from '../types';
import { fileNameFromKey, formatSize } from '../utils/media';

const PAGE_SIZE = 24;
const SEARCH_DEBOUNCE_MS = 300;

interface MediaLibraryProps {
  /** Called with the chosen file; the parent closes the library */
  onSelect: (object: StorageObject) => void;
  onClose: () => void;
}

/**
 * Modal that lists uploaded images (`GET /api/v1/storage/objects`, newest first)
 * so an image block can reuse an existing file instead of uploading it again.
 */
export const MediaLibrary: React.FC<MediaLibraryProps> = ({ onSelect, onClose }) => {
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
  const searchRef = useRef<HTMLInputElement>(null);

  const onCloseRef = useRef(onClose);
  useEffect(() => {
    onCloseRef.current = onClose;
  }, [onClose]);

  useEffect(() => {
    searchRef.current?.focus();
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onCloseRef.current();
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
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
          error: err instanceof Error ? err.message : 'Failed to fetch media',
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

  return (
    <div className="modal-overlay" onMouseDown={onClose}>
      <div
        className="modal media-library"
        role="dialog"
        aria-modal="true"
        aria-labelledby="media-library-title"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="modal-header">
          <div className="modal-title">
            <Images size={18} />
            <h2 id="media-library-title">Media Library</h2>
          </div>
          <div className="media-library-actions">
            <button
              className="icon-btn"
              onClick={reload}
              disabled={isLoading}
              aria-label="Reload"
              title="Reload"
            >
              <RefreshCw size={16} />
            </button>
            <button className="icon-btn" onClick={onClose} aria-label="Close">
              <X size={16} />
            </button>
          </div>
        </div>

        <div className="media-library-toolbar">
          <div className="input-field">
            <Search size={14} />
            <input
              ref={searchRef}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search by file name..."
              aria-label="Search media"
            />
          </div>
          <span className="media-library-count">
            {objects.length}/{total} images
          </span>
        </div>

        <div className="media-library-body">
          {error && <div className="category-error">{error}</div>}
          {!error && !isLoading && objects.length === 0 && (
            <div className="category-empty">
              {query ? `No images match "${query}".` : 'No images uploaded yet.'}
            </div>
          )}
          <div className="media-grid">
            {objects.map((object) => (
              <button
                key={object.key}
                type="button"
                className="media-item"
                onClick={() => onSelect(object)}
                title={object.key}
              >
                <img src={object.url} alt="" loading="lazy" decoding="async" />
                <span className="media-item-name">{fileNameFromKey(object.key)}</span>
                <span className="media-item-meta">
                  {formatSize(object.size)}
                  {object.last_modified &&
                    ` · ${new Date(object.last_modified).toLocaleDateString()}`}
                </span>
              </button>
            ))}
          </div>
          {isLoading && (
            <div className="media-library-loading">
              <Loader2 size={18} className="animate-spin" />
              <span>Loading...</span>
            </div>
          )}
          {!isLoading && objects.length < total && (
            <div className="media-library-more">
              <button className="btn btn-secondary" onClick={() => setOffset(objects.length)}>
                Load more
              </button>
            </div>
          )}
          {result?.truncated && (
            <div className="media-library-note">
              The storage holds more files than the server scans for one listing (10,000), so some
              images are not shown.
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
