import type { PluginLogEntry } from '../types.ts';

interface LogsViewerProps {
  logs: PluginLogEntry[];
}

export function LogsViewer({ logs }: LogsViewerProps) {
  if (logs.length === 0) {
    return (
      <div style={{ padding: '16px', color: '#6e7781', fontSize: '13px', fontStyle: 'italic' }}>
        No logs captured for this invocation. Use log_info!, log_warn!, or log_error! in your
        plugin.
      </div>
    );
  }

  const levelColor = (level: string) => {
    switch (level.toLowerCase()) {
      case 'error':
        return { bg: '#ffebe9', text: '#cf222e' };
      case 'warn':
        return { bg: '#fff8c5', text: '#9a6700' };
      case 'debug':
        return { bg: '#f6f8fa', text: '#57606a' };
      default:
        return { bg: '#ddf4ff', text: '#0969da' };
    }
  };

  return (
    <div
      style={{
        height: '100%',
        overflowY: 'auto',
        padding: '8px',
        fontFamily: 'ui-monospace, SFMono-Regular, monospace',
        fontSize: '12px',
      }}
    >
      {logs.map((log, i) => {
        const colors = levelColor(log.level);
        return (
          <div
            key={i}
            style={{
              display: 'flex',
              gap: '8px',
              padding: '4px 6px',
              borderRadius: '4px',
              borderBottom: '1px solid #eaeef2',
              alignItems: 'flex-start',
            }}
          >
            <span style={{ color: '#8c959f', fontSize: '11px', minWidth: '70px' }}>
              {new Date(log.timestamp).toLocaleTimeString()}
            </span>
            <span
              style={{
                padding: '1px 5px',
                borderRadius: '4px',
                fontSize: '10px',
                fontWeight: 600,
                textTransform: 'uppercase',
                backgroundColor: colors.bg,
                color: colors.text,
              }}
            >
              {log.level}
            </span>
            <span
              style={{ color: '#1f2328', whiteSpace: 'pre-wrap', wordBreak: 'break-word', flex: 1 }}
            >
              {log.message}
            </span>
          </div>
        );
      })}
    </div>
  );
}
