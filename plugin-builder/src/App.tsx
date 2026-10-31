import { useEffect, useState, useCallback, useRef } from 'react';
import { Header } from './components/Header.tsx';
import { CodeEditor } from './components/CodeEditor.tsx';
import { MetricsBar } from './components/MetricsBar.tsx';
import { OutputViewer } from './components/OutputViewer.tsx';
import { fetchPluginInfo, fetchFixtures, invokeHook, pollVersion } from './api.ts';
import type { PluginInfo, InvokeResponse, MockHttpRule } from './types.ts';

export function App() {
  const [pluginInfo, setPluginInfo] = useState<PluginInfo | null>(null);
  const [fixtures, setFixtures] = useState<Record<string, unknown>>({});
  const [selectedHook, setSelectedHook] = useState<string>('');
  const [inputTab, setInputTab] = useState<'payload' | 'config' | 'kv' | 'mock_http'>('payload');

  const [payloadText, setPayloadText] = useState<string>('{}');
  const [configText, setConfigText] = useState<string>('{}');
  const [kvText, setKvText] = useState<string>('{}');
  const [mockHttpText, setMockHttpText] = useState<string>('[]');

  const [response, setResponse] = useState<InvokeResponse | null>(null);
  const [loading, setLoading] = useState<boolean>(false);
  const [connected, setConnected] = useState<boolean>(false);

  const lastVersionRef = useRef<number>(0);

  // 初期ロード
  useEffect(() => {
    let unmounted = false;

    async function init() {
      try {
        const [info, fixs] = await Promise.all([fetchPluginInfo(), fetchFixtures()]);
        if (unmounted) return;
        setPluginInfo(info);
        setFixtures(fixs);
        setConnected(true);

        const availableHooks = info.manifest.capabilities.hooks;
        if (availableHooks.length > 0) {
          const first = availableHooks[0];
          setSelectedHook(first);
          if (fixs[first]) {
            setPayloadText(JSON.stringify(fixs[first], null, 2));
          }
        }
      } catch (err) {
        if (!unmounted) setConnected(false);
        console.error('Failed to initialize Plugin Builder:', err);
      }
    }

    init();

    // ホットリロードのポーリング
    const abortCtrl = new AbortController();
    async function startPolling() {
      while (!abortCtrl.signal.aborted) {
        try {
          const v = await pollVersion(lastVersionRef.current);
          if (abortCtrl.signal.aborted) break;
          if (lastVersionRef.current !== 0 && v !== lastVersionRef.current) {
            // リロード検知: プラグイン情報を再取得
            const updated = await fetchPluginInfo();
            setPluginInfo(updated);
          }
          lastVersionRef.current = v;
          setConnected(true);
        } catch {
          if (!abortCtrl.signal.aborted) {
            setConnected(false);
            await new Promise((r) => setTimeout(r, 2000));
          }
        }
      }
    }

    startPolling();

    return () => {
      unmounted = true;
      abortCtrl.abort();
    };
  }, []);

  // フック切り替え時
  const handleSelectHook = useCallback(
    (hook: string) => {
      setSelectedHook(hook);
      if (fixtures[hook]) {
        setPayloadText(JSON.stringify(fixtures[hook], null, 2));
      }
    },
    [fixtures],
  );

  // フィクスチャ復元
  const handleResetFixture = useCallback(() => {
    if (selectedHook && fixtures[selectedHook]) {
      setPayloadText(JSON.stringify(fixtures[selectedHook], null, 2));
    }
  }, [selectedHook, fixtures]);

  // フック実行
  const handleExecute = useCallback(async () => {
    if (!selectedHook || loading) return;
    setLoading(true);

    try {
      let parsedInput: unknown = payloadText;
      try {
        parsedInput = JSON.parse(payloadText);
      } catch {
        // 文字列として送信
      }

      let parsedConfig: Record<string, string> = {};
      try {
        parsedConfig = JSON.parse(configText);
      } catch {
        // ignore
      }

      let parsedKv: Record<string, string> = {};
      try {
        parsedKv = JSON.parse(kvText);
      } catch {
        // ignore
      }

      let parsedMockHttp: MockHttpRule[] = [];
      try {
        parsedMockHttp = JSON.parse(mockHttpText);
      } catch {
        // ignore
      }

      const res = await invokeHook({
        hook: selectedHook,
        input: parsedInput,
        config: parsedConfig,
        kv: parsedKv,
        mock_http: parsedMockHttp,
      });

      setResponse(res);
    } catch (err) {
      setResponse({
        success: false,
        output: null,
        raw_output: null,
        error: err instanceof Error ? err.message : String(err),
        fuel_consumed: 0,
        duration_ms: 0,
        logs: [],
        kv_mutations: {},
        http_requests: [],
      });
    } finally {
      setLoading(false);
    }
  }, [selectedHook, payloadText, configText, kvText, mockHttpText, loading]);

  const inputTabs = [
    { id: 'payload', label: 'Payload (Input)' },
    { id: 'config', label: 'Settings (Config)' },
    { id: 'kv', label: 'Initial KV Store' },
    { id: 'mock_http', label: 'Mock HTTP Rules' },
  ] as const;

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        height: '100vh',
        width: '100vw',
        overflow: 'hidden',
        backgroundColor: '#ffffff',
        color: '#1f2328',
      }}
    >
      <Header
        pluginInfo={pluginInfo}
        selectedHook={selectedHook}
        onSelectHook={handleSelectHook}
        onExecute={handleExecute}
        onResetFixture={handleResetFixture}
        loading={loading}
        connected={connected}
      />

      <div style={{ display: 'flex', flex: 1, minHeight: 0 }}>
        {/* 左側: 入力パネル */}
        <div
          style={{
            flex: 1,
            display: 'flex',
            flexDirection: 'column',
            borderRight: '1px solid #d0d7de',
            minWidth: 0,
          }}
        >
          <div
            style={{
              display: 'flex',
              borderBottom: '1px solid #d0d7de',
              backgroundColor: '#f6f8fa',
            }}
          >
            {inputTabs.map((tab) => (
              <button
                key={tab.id}
                onClick={() => setInputTab(tab.id)}
                style={{
                  padding: '8px 14px',
                  fontSize: '12px',
                  fontWeight: inputTab === tab.id ? 600 : 400,
                  color: inputTab === tab.id ? '#0969da' : '#57606a',
                  border: 'none',
                  borderBottom: inputTab === tab.id ? '2px solid #0969da' : '2px solid transparent',
                  backgroundColor: 'transparent',
                  cursor: 'pointer',
                }}
              >
                {tab.label}
              </button>
            ))}
          </div>

          <div style={{ flex: 1, overflow: 'hidden' }}>
            {inputTab === 'payload' && (
              <CodeEditor value={payloadText} onChange={setPayloadText} onExecute={handleExecute} />
            )}
            {inputTab === 'config' && (
              <CodeEditor value={configText} onChange={setConfigText} onExecute={handleExecute} />
            )}
            {inputTab === 'kv' && (
              <CodeEditor value={kvText} onChange={setKvText} onExecute={handleExecute} />
            )}
            {inputTab === 'mock_http' && (
              <CodeEditor
                value={mockHttpText}
                onChange={setMockHttpText}
                onExecute={handleExecute}
              />
            )}
          </div>
        </div>

        {/* 右側: 結果パネル */}
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0 }}>
          <MetricsBar
            response={response}
            maxFuel={pluginInfo?.manifest.resource_limits.max_fuel_units}
          />
          <div style={{ flex: 1, overflow: 'hidden' }}>
            <OutputViewer response={response} />
          </div>
        </div>
      </div>
    </div>
  );
}
