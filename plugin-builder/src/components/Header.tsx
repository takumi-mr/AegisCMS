import { useState } from 'react';
import { Play, RotateCcw, Box, CheckCircle2, AlertCircle, Hammer, Loader2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { PluginInfo, BuildResponse } from '../types.ts';
import { LanguageSelect } from './LanguageSelect.tsx';

interface HeaderProps {
  pluginInfo: PluginInfo | null;
  selectedHook: string;
  onSelectHook: (hook: string) => void;
  onExecute: () => void;
  onResetFixture: () => void;
  onBuild: () => void;
  loading: boolean;
  building: boolean;
  connected: boolean;
  buildStatus: BuildResponse | null;
}

export function Header({
  pluginInfo,
  selectedHook,
  onSelectHook,
  onExecute,
  onResetFixture,
  onBuild,
  loading,
  building,
  connected,
  buildStatus,
}: HeaderProps) {
  const { t } = useTranslation();
  const [showBuildOutput, setShowBuildOutput] = useState(false);
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
        position: 'relative',
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
          <span>{pluginInfo ? pluginInfo.manifest.name : t('header.title')}</span>
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
          <span>{connected ? t('header.liveSync') : t('header.offline')}</span>
        </div>

        {buildStatus && (
          <div style={{ position: 'relative' }}>
            <button
              onClick={() => setShowBuildOutput((v) => !v)}
              title={t('header.viewBuildOutput')}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '4px',
                fontSize: '11px',
                padding: '2px 8px',
                borderRadius: '12px',
                border: 'none',
                cursor: 'pointer',
                backgroundColor: buildStatus.success ? '#dafbe1' : '#ffebe9',
                color: buildStatus.success ? '#1a7f37' : '#cf222e',
                fontWeight: 500,
              }}
            >
              {buildStatus.success
                ? t('header.builtSuccess', { duration: buildStatus.duration_ms })
                : t('header.buildFailed')}
            </button>
            {showBuildOutput && (
              <div
                style={{
                  position: 'absolute',
                  top: '26px',
                  left: '0',
                  zIndex: 100,
                  width: '420px',
                  maxHeight: '260px',
                  overflow: 'auto',
                  backgroundColor: '#24292f',
                  color: '#f0f6fc',
                  padding: '10px',
                  borderRadius: '6px',
                  fontSize: '11px',
                  fontFamily: 'monospace',
                  whiteSpace: 'pre-wrap',
                  boxShadow: '0 8px 24px rgba(140,149,159,0.2)',
                  border: '1px solid #444c56',
                }}
              >
                <div
                  style={{
                    display: 'flex',
                    justifyContent: 'space-between',
                    marginBottom: '6px',
                    fontWeight: 600,
                  }}
                >
                  <span>{t('header.buildOutput')}</span>
                  <button
                    onClick={() => setShowBuildOutput(false)}
                    aria-label={t('common.close')}
                    style={{
                      background: 'none',
                      border: 'none',
                      color: '#8b949e',
                      cursor: 'pointer',
                    }}
                  >
                    ✕
                  </button>
                </div>
                {buildStatus.error && (
                  <div style={{ color: '#ff7b72', marginBottom: '6px' }}>{buildStatus.error}</div>
                )}
                <div>{buildStatus.output || '(No stdout)'}</div>
              </div>
            )}
          </div>
        )}
      </div>

      <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
        <button
          onClick={onBuild}
          disabled={building || !connected}
          title={t('header.rebuildTitle')}
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: '5px',
            padding: '5px 10px',
            fontSize: '12px',
            fontWeight: 500,
            border: '1px solid #d0d7de',
            borderRadius: '6px',
            backgroundColor: '#ffffff',
            color: '#24292f',
            cursor: building ? 'not-allowed' : 'pointer',
          }}
        >
          {building ? <Loader2 size={13} className="spin" /> : <Hammer size={13} />}
          <span>{building ? t('header.building') : t('header.rebuild')}</span>
        </button>

        <div style={{ width: '1px', height: '18px', backgroundColor: '#d0d7de' }} />

        <label style={{ fontSize: '12px', fontWeight: 500, color: '#57606a' }}>
          {t('header.hookLabel')}
        </label>
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
          title={t('header.resetFixtureTitle')}
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
          <span>{t('header.resetFixture')}</span>
        </button>

        <button
          onClick={onExecute}
          disabled={loading || !connected}
          title={t('header.runTitle')}
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
          <span>{loading ? t('header.running') : t('header.run')}</span>
        </button>

        <div style={{ width: '1px', height: '18px', backgroundColor: '#d0d7de' }} />

        <LanguageSelect />
      </div>
    </header>
  );
}
