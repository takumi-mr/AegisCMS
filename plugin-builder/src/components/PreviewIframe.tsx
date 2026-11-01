import React, { useRef, useState } from 'react';

interface PreviewIframeProps {
  html: string | null;
  loading?: boolean;
}

export const PreviewIframe: React.FC<PreviewIframeProps> = ({ html, loading = false }) => {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const [deviceMode, setDeviceMode] = useState<'desktop' | 'tablet' | 'mobile'>('desktop');

  const handleOpenNewTab = () => {
    if (!html) return;
    const blob = new Blob([html], { type: 'text/html;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    window.open(url, '_blank', 'noopener,noreferrer');
  };

  const handleReload = () => {
    if (iframeRef.current && html) {
      iframeRef.current.srcdoc = html;
    }
  };

  const widthStyle = deviceMode === 'mobile' ? '375px' : deviceMode === 'tablet' ? '768px' : '100%';

  if (!html) {
    return (
      <div
        style={{
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          height: '100%',
          color: 'var(--text-muted, #888)',
          padding: '24px',
          textAlign: 'center',
        }}
      >
        <svg
          width="48"
          height="48"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
          style={{ marginBottom: '12px', opacity: 0.5 }}
        >
          <path d="M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7Z" />
          <circle cx="12" cy="12" r="3" />
        </svg>
        <p style={{ margin: 0, fontWeight: 500 }}>No preview available</p>
        <p style={{ margin: '8px 0 0', fontSize: '13px', maxWidth: '360px' }}>
          Execute a frontend hook (such as <code>on-render-page</code>, <code>render-blocks</code>,
          or <code>transform-content-ast</code>) to see a live HTML render.
        </p>
      </div>
    );
  }

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        height: '100%',
        width: '100%',
        background: 'var(--preview-bg, #1e1e24)',
      }}
    >
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '6px 12px',
          background: 'var(--panel-header-bg, #25252d)',
          borderBottom: '1px solid var(--border-color, #333)',
          fontSize: '12px',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <span style={{ color: 'var(--text-muted, #aaa)', fontWeight: 500 }}>Device:</span>
          <div
            style={{
              display: 'flex',
              borderRadius: '4px',
              overflow: 'hidden',
              border: '1px solid #444',
            }}
          >
            {(['desktop', 'tablet', 'mobile'] as const).map((mode) => (
              <button
                key={mode}
                onClick={() => setDeviceMode(mode)}
                style={{
                  background: deviceMode === mode ? 'var(--accent, #3b82f6)' : '#2a2a35',
                  color: deviceMode === mode ? '#fff' : '#aaa',
                  border: 'none',
                  padding: '3px 8px',
                  cursor: 'pointer',
                  fontSize: '11px',
                  textTransform: 'capitalize',
                }}
              >
                {mode}
              </button>
            ))}
          </div>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          {loading && <span style={{ color: '#aaa', fontSize: '11px' }}>Rendering...</span>}
          <button
            onClick={handleReload}
            title="Reload preview"
            style={{
              background: '#2a2a35',
              border: '1px solid #444',
              color: '#ddd',
              padding: '3px 8px',
              borderRadius: '4px',
              cursor: 'pointer',
              fontSize: '11px',
            }}
          >
            Reload
          </button>
          <button
            onClick={handleOpenNewTab}
            title="Open in new window"
            style={{
              background: '#2a2a35',
              border: '1px solid #444',
              color: '#ddd',
              padding: '3px 8px',
              borderRadius: '4px',
              cursor: 'pointer',
              fontSize: '11px',
            }}
          >
            New Tab ↗
          </button>
        </div>
      </div>
      <div
        style={{
          flex: 1,
          display: 'flex',
          justifyContent: 'center',
          alignItems: 'stretch',
          padding: deviceMode === 'desktop' ? '0' : '16px',
          background: deviceMode === 'desktop' ? 'transparent' : 'rgba(0, 0, 0, 0.2)',
          overflow: 'auto',
        }}
      >
        <iframe
          ref={iframeRef}
          srcDoc={html}
          sandbox="allow-scripts allow-forms allow-popups allow-modals"
          title="Plugin HTML Preview"
          style={{
            width: widthStyle,
            height: '100%',
            border: deviceMode === 'desktop' ? 'none' : '1px solid #444',
            borderRadius: deviceMode === 'desktop' ? '0' : '6px',
            background: '#ffffff',
            boxShadow: deviceMode === 'desktop' ? 'none' : '0 4px 12px rgba(0, 0, 0, 0.3)',
            transition: 'width 0.2s ease',
          }}
        />
      </div>
    </div>
  );
};
