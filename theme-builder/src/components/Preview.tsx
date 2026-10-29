import { Monitor, Smartphone, Tablet } from 'lucide-react';

const ICONS = {
  fit: Monitor,
  '1280': Monitor,
  '768': Tablet,
  '390': Smartphone,
  '320': Smartphone,
};
import type { SampleInfo } from '../types.ts';
import { VIEWPORTS, type ViewportId } from '../viewports.ts';

export interface PreviewProps {
  samples: SampleInfo[];
  sample: string;
  viewport: ViewportId;
  /** The page to show; null until the first render finishes */
  doc: string | null;
  /** The last render failed: the frame keeps showing the last good page */
  failed: boolean;
  pending: boolean;
  onSample: (name: string) => void;
  onViewport: (id: ViewportId) => void;
}

export function Preview({
  samples,
  sample,
  viewport,
  doc,
  failed,
  pending,
  onSample,
  onViewport,
}: PreviewProps) {
  const width = VIEWPORTS.find((v) => v.id === viewport)?.width ?? null;
  return (
    <section className="preview" aria-label="プレビュー">
      <div className="toolbar">
        <label>
          ページ
          <select value={sample} onChange={(e) => onSample(e.target.value)}>
            {samples.map((s) => (
              <option key={s.name} value={s.name}>
                {s.name}（{s.template}）
              </option>
            ))}
          </select>
        </label>
        <div className="segmented" role="group" aria-label="画面の幅">
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
          {pending ? '描画中…' : failed ? 'エラー（直前の表示）' : ''}
        </span>
      </div>
      <div className="stage">
        {doc === null ? (
          <p className="empty">プレビューを準備しています…</p>
        ) : (
          <iframe
            title="プレビュー"
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
