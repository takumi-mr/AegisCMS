import React, { useEffect, useId, useState } from 'react';
import { Eye, Code, LayoutTemplate, ShieldAlert } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi, SERVER_URL } from '../api/client';
import i18n from '../i18n';
import type { BlockNode, PreviewRequest } from '../types';
import { cx } from '../utils/cx';
import styles from './Preview.module.css';

/** Wait for a pause in typing before asking the server to render */
const PREVIEW_DEBOUNCE_MS = 300;

type PreviewTab = 'html' | 'page' | 'ast';

const TABS: Array<{ id: PreviewTab; icon: React.ReactNode }> = [
  { id: 'html', icon: <Eye size={14} className={styles.tabIcon} /> },
  { id: 'page', icon: <LayoutTemplate size={14} className={styles.tabIcon} /> },
  { id: 'ast', icon: <Code size={14} className={styles.tabIcon} /> },
];

interface PreviewProps {
  contentType: string;
  title: string;
  slug: string;
  /** Selected category id, or '' for none */
  categoryId: string;
  blocks: BlockNode[];
}

interface PreviewResult {
  /** The request (serialized) this result was rendered from */
  key: string;
  html: string;
  pageHtml: string | null;
  warnings: string[];
  error: string | null;
}

/**
 * The themed page is shown in an iframe sandboxed to scripts only: theme and plugin scripts
 * (Mermaid, Prism) run, but without `allow-same-origin` the document gets an opaque origin and
 * cannot read the admin's storage (the session token) or call the API as the signed-in user.
 * `<base>` makes root-relative theme assets and uploads load from the AegisCMS server.
 */
const withBase = (pageHtml: string): string => {
  const base = `<base href="${SERVER_URL}/">`;
  return /<head[^>]*>/i.test(pageHtml)
    ? pageHtml.replace(/<head[^>]*>/i, (head) => `${head}${base}`)
    : `${base}${pageHtml}`;
};

/**
 * Live preview rendered by the server (`POST /api/v1/preview`) with the same pipeline as saving
 * and the public site: heading ids, plugin hooks, HtmlRenderer, the security rewriter and, for
 * the themed page, the `on-render-page` hook and theme template.
 */
export const Preview: React.FC<PreviewProps> = ({
  contentType,
  title,
  slug,
  categoryId,
  blocks,
}) => {
  const { t } = useTranslation();
  const [tab, setTab] = useState<PreviewTab>('html');
  const [result, setResult] = useState<PreviewResult | null>(null);
  const idPrefix = useId();

  // A result is current when it was rendered from the content shown now
  const request: PreviewRequest = {
    content_type: contentType,
    title,
    slug,
    fields: categoryId ? { category_id: categoryId } : {},
    blocks,
  };
  const requestKey = JSON.stringify(request);
  const isRendering = result?.key !== requestKey;

  useEffect(() => {
    let cancelled = false;
    const timer = window.setTimeout(() => {
      aegisApi
        .previewContent(JSON.parse(requestKey) as PreviewRequest)
        .then((preview) => ({
          html: preview.html,
          pageHtml: preview.page_html,
          warnings: preview.warnings,
          error: null,
        }))
        .catch((err: unknown) => ({
          html: null,
          pageHtml: null,
          warnings: [],
          error: err instanceof Error ? err.message : i18n.t('api.previewFailed'),
        }))
        .then((outcome) => {
          if (cancelled) return;
          // Keep the last good output on screen when a render fails
          setResult((prev) => ({
            key: requestKey,
            html: outcome.html ?? prev?.html ?? '',
            pageHtml: outcome.error ? (prev?.pageHtml ?? null) : outcome.pageHtml,
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
    ? t('preview.failed', { error: result.error })
    : isRendering
      ? t('preview.rendering')
      : t('preview.rendered');

  const statusLines = (
    <>
      <div className={cx(styles.status, result?.error && styles.statusError)}>{status}</div>
      {result?.warnings.map((warning) => (
        <div key={warning} className={cx(styles.status, styles.statusError)}>
          ⚠ {warning}
        </div>
      ))}
    </>
  );

  return (
    <aside className={styles.column}>
      <div className={styles.tabs} role="tablist" aria-label={t('preview.aria')}>
        {TABS.map(({ id, icon }) => (
          <button
            key={id}
            type="button"
            role="tab"
            id={`${idPrefix}-tab-${id}`}
            aria-selected={tab === id}
            aria-controls={`${idPrefix}-panel`}
            className={cx(styles.tab, tab === id && styles.active)}
            onClick={() => setTab(id)}
          >
            {icon}
            {t(`preview.tabs.${id}`)}
          </button>
        ))}
      </div>

      <div
        id={`${idPrefix}-panel`}
        role="tabpanel"
        aria-labelledby={`${idPrefix}-tab-${tab}`}
        className={cx(styles.content, tab === 'page' && styles.pageContent)}
      >
        {tab === 'html' && (
          <div>
            <div className={styles.titleBlock}>
              <h1 className={styles.title}>{title || t('preview.untitled')}</h1>
              {statusLines}
            </div>

            <div
              className={cx(styles.htmlOutput, isRendering && styles.stale)}
              dangerouslySetInnerHTML={{ __html: result?.html ?? '' }}
            />

            {blocks.length === 0 && (
              <div className={styles.empty}>
                <ShieldAlert size={36} className={styles.emptyIcon} />
                <p>{t('preview.empty')}</p>
              </div>
            )}
          </div>
        )}

        {tab === 'page' && (
          <>
            <div className={styles.pageStatus}>{statusLines}</div>
            {result?.pageHtml ? (
              <iframe
                className={cx(styles.pageFrame, isRendering && styles.stale)}
                title={t('preview.pageFrame')}
                sandbox="allow-scripts"
                srcDoc={withBase(result.pageHtml)}
              />
            ) : (
              <div className={styles.empty}>
                <p>{t('preview.pageEmpty')}</p>
              </div>
            )}
          </>
        )}

        {tab === 'ast' && <pre className={styles.ast}>{JSON.stringify(blocks, null, 2)}</pre>}
      </div>
    </aside>
  );
};
