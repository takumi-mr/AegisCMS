import { Play, RotateCcw, Box, CheckCircle2, AlertCircle } from 'lucide-react';
import type { PluginInfo } from '../types.ts';

interface HeaderProps {
  pluginInfo: PluginInfo | null;
  selectedHook: string;
  onSelectHook: (hook: string) => void;
  onExecute: () => void;
  onResetFixture: () => void;
  loading: boolean;
  connected: boolean;
}

export function Header({
  pluginInfo,
  selectedHook,
  onSelectHook,
  onExecute,
  onResetFixture,
  loading,
  connected,
}: HeaderProps) {
  const hooks = pluginInfo?.manifest.capabilities.hooks ?? [];

  return (
    <header
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        padding: '10px 16px',
        backgroundColor: '#f6f8fa',
        borderBottom: '1px solid #d0d7de',
      }}
    >
      <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: '6px',
            fontWeight: 600,
            fontSize: '15px',
          }}
        >
          <Box size={18} color="#0969da" />
          <span>{pluginInfo ? pluginInfo.manifest.name : 'Plugin Builder'}</span>
          {pluginInfo && (
            <span
              style={{
                fontSize: '11px',
                padding: '2px 6px',
                borderRadius: '10px',
                backgroundColor: '#ddf4ff',
                color: '#0969da',
                fontWeight: 500,
              }}
            >
              v{pluginInfo.manifest.version}
            </span>
          )}
        </div>

        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: '4px',
            fontSize: '12px',
            color: connected ? '#1a7f37' : '#cf222e',
          }}
        >
          {connected ? <CheckCircle2 size={14} /> : <AlertCircle size={14} />}
          <span>{connected ? 'Live Sync' : 'Offline'}</span>
        </div>
      </div>

      <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
        <label style={{ fontSize: '12px', fontWeight: 500, color: '#57606a' }}>Hook:</label>
        <select
          value={selectedHook}
          onChange={(e) => onSelectHook(e.target.value)}
          style={{
            padding: '5px 8px',
            fontSize: '13px',
            borderRadius: '6px',
            border: '1px solid #d0d7de',
            backgroundColor: '#ffffff',
            fontWeight: 500,
          }}
        >
          {hooks.map((h) => (
            <option key={h} value={h}>
              {h}
            </option>
          ))}
        </select>

        <button
          onClick={onResetFixture}
          title="Reset input to standard fixture"
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: '4px',
            padding: '5px 10px',
            fontSize: '12px',
            border: '1px solid #d0d7de',
            borderRadius: '6px',
            backgroundColor: '#ffffff',
            cursor: 'pointer',
          }}
        >
          <RotateCcw size={13} />
          <span>Fixture</span>
        </button>

        <button
          onClick={onExecute}
          disabled={loading || !connected}
          title="Execute hook in Wasm sandbox (Ctrl+Enter)"
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: '6px',
            padding: '6px 14px',
            fontSize: '13px',
            fontWeight: 600,
            color: '#ffffff',
            backgroundColor: loading ? '#94d3a2' : '#1f883d',
            border: 'none',
            borderRadius: '6px',
            cursor: loading ? 'not-allowed' : 'pointer',
          }}
        >
          <Play size={14} fill="#ffffff" />
          <span>{loading ? 'Running...' : 'Run (Ctrl+Enter)'}</span>
        </button>
      </div>
    </header>
  );
}
