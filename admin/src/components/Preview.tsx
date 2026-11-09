import React, { useEffect, useId, useState } from 'react';
import {
  Eye,
  Code,
  LayoutTemplate,
  Maximize2,
  Minimize2,
  ExternalLink,
  ShieldAlert,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi, previewFrameUrl } from '../api/client';
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
  /** Page layout: 'default' or 'blank' */
  layout?: string;
  /** Who can read the content: '' is everyone, otherwise a member level */
  access?: string;
  /** `[members].levels` (empty when member accounts are off): the choices of "preview as" */
  memberLevels?: string[];
  blocks: BlockNode[];
}

interface PreviewResult {
  /** The request (serialized) this result was rendered from */
  key: string;
  html: string;
  /** Where the server keeps the themed page (an iframe and a new window load it) */
  pageUrl: string | null;
  warnings: string[];
  error: string | null;
}

/**
 * A random name for where the server keeps this preview's themed page (32 hex digits). The server
 * serves the page at that URL, with the public site's CSP and `sandbox allow-scripts`.
 *
 * The page is loaded from a URL, not put into the iframe's `srcdoc`: a `srcdoc` document inherits
 * Admin Studio's CSP (scripts from its own origin only), which would stop the theme's and plugins'
 * inline scripts. Loaded from the URL, the page runs as it does on the public site, but with an
 * opaque origin (sandbox without `allow-same-origin`), so it cannot read the admin's session token
 * or call the API as the signed-in user.
 */
const newFrameKey = (): string =>
  Array.from(crypto.getRandomValues(new Uint8Array(16)), (b) =>
    b.toString(16).padStart(2, '0')
  ).join('');

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
  layout,
  access = '',
  memberLevels = [],
  blocks,
}) => {
  const { t } = useTranslation();
  const [tab, setTab] = useState<PreviewTab>('html');
  const [result, setResult] = useState<PreviewResult | null>(null);
  const [fullscreen, setFullscreen] = useState(false);
  // Who the content is shown to: 'staff' is the full text; 'guest' and 'member:<level>' show what
  // that kind of reader gets (the part before the members-only break, and a notice)
  const [viewer, setViewer] = useState('staff');
  // Only content with an access level looks different to different readers
  const canChooseViewer = access !== '' && memberLevels.length > 0;
  const effectiveViewer = canChooseViewer ? viewer : 'staff';
  const idPrefix = useId();
  const [frameKey] = useState(newFrameKey);

  // A result is current when it was rendered from the content shown now
  const request: PreviewRequest = {
    content_type: contentType,
    title,
    slug,
    fields: {
      ...(categoryId ? { category_id: categoryId } : {}),
      ...(layout ? { layout } : {}),
      ...(access ? { access } : {}),
    },
    ...(effectiveViewer !== 'staff' ? { viewer: effectiveViewer } : {}),
    frame_key: frameKey,
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
          pageUrl: preview.page_url ? previewFrameUrl(preview.page_url) : null,
          warnings: preview.warnings,
          error: null,
        }))
        .catch((err: unknown) => ({
          html: null,
          pageUrl: null,
          warnings: [],
          error: err instanceof Error ? err.message : i18n.t('api.previewFailed'),
        }))
        .then((outcome) => {
          if (cancelled) return;
          // Keep the last good output on screen when a render fails
          setResult((prev) => ({
            key: requestKey,
            html: outcome.html ?? prev?.html ?? '',
            pageUrl: outcome.error ? (prev?.pageUrl ?? null) : outcome.pageUrl,
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

  // Esc leaves the full-screen view
  useEffect(() => {
    if (!fullscreen) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setFullscreen(false);
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [fullscreen]);

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
    <aside className={cx(styles.column, fullscreen && styles.fullscreen)}>
      <div className={styles.bar}>
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
        <div className={styles.actions}>
          <button
            type="button"
            className={styles.action}
            title={t('preview.openWindow')}
            aria-label={t('preview.openWindow')}
            disabled={!result?.pageUrl}
            // The server sends the page with `sandbox allow-scripts`, so a window of its own is
            // as isolated from Admin Studio as the iframe is
            onClick={() => result?.pageUrl && window.open(result.pageUrl, '_blank', 'noopener')}
          >
            <ExternalLink size={14} />
          </button>
          <button
            type="button"
            className={styles.action}
            title={fullscreen ? t('preview.exitFullscreen') : t('preview.fullscreen')}
            aria-label={fullscreen ? t('preview.exitFullscreen') : t('preview.fullscreen')}
            aria-pressed={fullscreen}
            onClick={() => setFullscreen((on) => !on)}
          >
            {fullscreen ? <Minimize2 size={14} /> : <Maximize2 size={14} />}
          </button>
        </div>
      </div>

      {canChooseViewer && (
        <div className={styles.viewerRow}>
          <label htmlFor={`${idPrefix}-viewer`}>{t('preview.viewer.label')}</label>
          <select
            id={`${idPrefix}-viewer`}
            value={effectiveViewer}
            onChange={(e) => setViewer(e.target.value)}
          >
            <option value="staff">{t('preview.viewer.staff')}</option>
            <option value="guest">{t('preview.viewer.guest')}</option>
            {memberLevels.map((level) => (
              <option key={level} value={`member:${level}`}>
                {t('preview.viewer.member', { level })}
              </option>
            ))}
          </select>
        </div>
      )}

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
            {result?.pageUrl ? (
              <iframe
                className={cx(styles.pageFrame, isRendering && styles.stale)}
                title={t('preview.pageFrame')}
                sandbox="allow-scripts"
                src={result.pageUrl}
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
