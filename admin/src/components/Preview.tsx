import React, { useEffect, useState } from 'react';
import { Eye, Code, ShieldAlert } from 'lucide-react';
import { aegisApi } from '../api/client';
import type { BlockNode } from '../types';
import { cx } from '../utils/cx';
import styles from './Preview.module.css';

/** Wait for a pause in typing before asking the server to render */
const PREVIEW_DEBOUNCE_MS = 300;

interface PreviewProps {
  title: string;
  blocks: BlockNode[];
}

interface PreviewResult {
  /** The blocks (serialized) this result was rendered from */
  key: string;
  html: string;
  warnings: string[];
  error: string | null;
}

/**
 * Live preview rendered by the server (`POST /api/v1/preview`) with the same pipeline as saving
 * and the public site: heading ids, plugin AST hooks, HtmlRenderer and the security rewriter.
 */
export const Preview: React.FC<PreviewProps> = ({ title, blocks }) => {
  const [tab, setTab] = useState<'html' | 'ast'>('html');
  const [result, setResult] = useState<PreviewResult | null>(null);

  // A result is current when it was rendered from the blocks shown now
  const requestKey = JSON.stringify(blocks);
  const isRendering = result?.key !== requestKey;

  useEffect(() => {
    let cancelled = false;
    const timer = window.setTimeout(() => {
      aegisApi
        .previewContent(JSON.parse(requestKey) as BlockNode[])
        .then((preview) => ({ html: preview.html, warnings: preview.warnings, error: null }))
        .catch((err: unknown) => ({
          html: null,
          warnings: [],
          error: err instanceof Error ? err.message : 'Failed to render the preview',
        }))
        .then((outcome) => {
          if (cancelled) return;
          // Keep the last good HTML on screen when a render fails
          setResult((prev) => ({
            key: requestKey,
            html: outcome.html ?? prev?.html ?? '',
            warnings: outcome.warnings,
            error: outcome.error,
          }));
        });
    }, PREVIEW_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [requestKey]);

  const status = result?.error
    ? `⚠ Preview failed: ${result.error}`
    : isRendering
      ? 'Rendering on the server...'
      : '✓ Rendered by the server (same HTML as the published page)';

  return (
    <aside className={styles.column}>
      <div className={styles.tabs}>
        <div
          className={cx(styles.tab, tab === 'html' && styles.active)}
          onClick={() => setTab('html')}
        >
          <Eye size={14} className={styles.tabIcon} />
          Live HTML Output
        </div>
        <div
          className={cx(styles.tab, tab === 'ast' && styles.active)}
          onClick={() => setTab('ast')}
        >
          <Code size={14} className={styles.tabIcon} />
          Block AST (JSON)
        </div>
      </div>

      <div className={styles.content}>
        {tab === 'html' ? (
          <div>
            <div className={styles.titleBlock}>
              <h1 className={styles.title}>{title || 'Untitled Article'}</h1>
              <div className={cx(styles.status, result?.error && styles.statusError)}>{status}</div>
              {result?.warnings.map((warning) => (
                <div key={warning} className={cx(styles.status, styles.statusError)}>
                  ⚠ {warning}
                </div>
              ))}
            </div>

            <div
              className={cx(styles.htmlOutput, isRendering && styles.stale)}
              dangerouslySetInnerHTML={{ __html: result?.html ?? '' }}
            />

            {blocks.length === 0 && (
              <div className={styles.empty}>
                <ShieldAlert size={36} className={styles.emptyIcon} />
                <p>Add blocks from the toolbar to see live deterministic output.</p>
              </div>
            )}
          </div>
        ) : (
          <pre className={styles.ast}>{JSON.stringify(blocks, null, 2)}</pre>
        )}
      </div>
    </aside>
  );
};
