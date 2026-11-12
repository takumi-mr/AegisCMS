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
import { newFrameKey } from '../utils/frameKey';
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
  /** Fill the width of the parent instead of the fixed editor column width (e.g. in a dialog) */
  fluid?: boolean;
}

interface PreviewResult {
  /** The request (serialized) this result was rendered from */
  key: string;
  html: string;
  /** Where the server keeps the themed page (an iframe and a new window load it) */
  pageUrl: string | null;
  /** Where the server keeps the body HTML (the "HTML output" tab loads it in an iframe) */
  bodyUrl: string | null;
  warnings: string[];
  error: string | null;
}

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
  fluid = false,
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
  const [bodyFrameKey] = useState(newFrameKey);

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
    body_frame_key: bodyFrameKey,
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
          bodyUrl: preview.body_url ? previewFrameUrl(preview.body_url) : null,
          warnings: preview.warnings,
          error: null,
        }))
        .catch((err: unknown) => ({
          html: null,
          pageUrl: null,
          bodyUrl: null,
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
            bodyUrl: outcome.error ? (prev?.bodyUrl ?? null) : outcome.bodyUrl,
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
    <aside className={cx(styles.column, fluid && styles.fluid, fullscreen && styles.fullscreen)}>
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

            {/* The body HTML is shown in a sandboxed frame loaded from the server, not put into
                Admin Studio's own page: Admin Studio's CSP allows no inline style or script, and the
                HTML (plugin blocks bring their own <style>) needs the public site's CSP */}
            {result?.bodyUrl && (
              <iframe
                className={cx(styles.htmlFrame, isRendering && styles.stale)}
                title={t('preview.bodyFrame')}
                sandbox=""
                src={result.bodyUrl}
              />
            )}

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
