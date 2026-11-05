import { useEffect, useRef } from 'react';
import { Crosshair, Monitor, Smartphone, Tablet } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { BlockInfo, SampleInfo } from '../types.ts';
import { VIEWPORTS, type ViewportId } from '../viewports.ts';

const ICONS = {
  fit: Monitor,
  '1280': Monitor,
  '768': Tablet,
  '390': Smartphone,
  '320': Smartphone,
};

export interface PreviewProps {
  samples: SampleInfo[];
  sample: string;
  viewport: ViewportId;
  blocks: BlockInfo[];
  selectedBlock: string | null;
  inspecting: boolean;
  /** The page to show; null until the first render finishes */
  doc: string | null;
  /** The last render failed: the frame keeps showing the last good page */
  failed: boolean;
  pending: boolean;
  onSample: (name: string) => void;
  onBlock: (block: string | null) => void;
  onToggleInspect: () => void;
  onViewport: (id: ViewportId) => void;
}

export function Preview({
  samples,
  sample,
  viewport,
  blocks,
  selectedBlock,
  inspecting,
  doc,
  failed,
  pending,
  onSample,
  onBlock,
  onToggleInspect,
  onViewport,
}: PreviewProps) {
  const { t } = useTranslation();
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const width = VIEWPORTS.find((v) => v.id === viewport)?.width ?? null;

  useEffect(() => {
    if (iframeRef.current?.contentWindow) {
      iframeRef.current.contentWindow.postMessage(
        { type: 'aegis-toggle-inspect', enabled: inspecting },
        '*'
      );
    }
  }, [inspecting]);

  return (
    <section className="preview" aria-label={t('preview.title')}>
      <div className="toolbar">
        <label>
          {t('preview.page')}
          <select value={sample} onChange={(e) => onSample(e.target.value)}>
            {samples.map((s) => (
              <option key={s.name} value={s.name}>
                {s.name}（{s.template}）
              </option>
            ))}
          </select>
        </label>
        {blocks.length > 0 && (
          <label>
            {t('preview.block')}
            <select
              value={selectedBlock ?? ''}
              onChange={(e) => onBlock(e.target.value ? e.target.value : null)}
              aria-label={t('preview.blockSelectAria')}
            >
              <option value="">{t('preview.wholePage')}</option>
              {blocks.map((b) => (
                <option key={b.name} value={b.name}>
                  {b.name}（{b.template}:{b.line}）
                </option>
              ))}
            </select>
          </label>
        )}
        <button
          type="button"
          className={`inspect-toggle ${inspecting ? 'active' : ''}`}
          onClick={onToggleInspect}
          title={t('preview.jumpTitle')}
          aria-label={t('preview.jumpAria')}
          aria-pressed={inspecting}
        >
          <Crosshair size={14} />
          <span>{t('preview.jump')}</span>
        </button>
        <div className="segmented" role="group" aria-label={t('preview.viewportWidth')}>
          {VIEWPORTS.map(({ id, label }) => {
            const Icon = ICONS[id];
            return (
              <button
                key={id}
                type="button"
                aria-pressed={viewport === id}
                title={label}
                aria-label={label}
                onClick={() => onViewport(id)}
              >
                <Icon size={14} />
                {id !== 'fit' && <span>{id}</span>}
              </button>
            );
          })}
        </div>
        <span className="status" role="status">
          {pending ? t('preview.rendering') : failed ? t('preview.staleError') : ''}
        </span>
      </div>
      <div className="stage">
        {doc === null ? (
          <p className="empty">{t('preview.preparing')}</p>
        ) : (
          <iframe
            ref={iframeRef}
            title={t('preview.iframeTitle')}
            // No allow-same-origin: the page runs with an opaque origin, apart from this app
            sandbox="allow-scripts allow-forms allow-popups"
            srcDoc={doc}
            className={failed ? 'stale' : undefined}
            style={width ? { width } : { width: '100%' }}
          />
        )}
      </div>
    </section>
  );
}
