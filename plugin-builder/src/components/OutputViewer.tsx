import { useState } from 'react';
import { CodeEditor } from './CodeEditor.tsx';
import { LogsViewer } from './LogsViewer.tsx';
import { PreviewIframe } from './PreviewIframe.tsx';
import type { InvokeResponse } from '../types.ts';

interface OutputViewerProps {
  hook: string;
  response: InvokeResponse | null;
  previewHtml: string | null;
  previewLoading?: boolean;
}

export function OutputViewer({
  hook,
  response,
  previewHtml,
  previewLoading = false,
}: OutputViewerProps) {
  const [userTab, setUserTab] = useState<'output' | 'preview' | 'logs' | 'kv' | 'http' | null>(
    null,
  );
  const [prevResponse, setPrevResponse] = useState<InvokeResponse | null>(response);

  // フロントエンド系フック実行時、成功していれば自動的に preview タブを選択
  if (prevResponse !== response) {
    setPrevResponse(response);
    if (
      response?.success &&
      ['on-render-page', 'render-blocks', 'transform-content-ast'].includes(hook)
    ) {
      setUserTab('preview');
    }
  }

  const activeTab = userTab ?? 'output';

  const tabs = [
    { id: 'output' as const, label: 'Output' },
    { id: 'preview' as const, label: 'HTML Preview' },
    { id: 'logs' as const, label: `Logs (${response?.logs.length ?? 0})` },
    {
      id: 'kv' as const,
      label: `KV Changes (${response ? Object.keys(response.kv_mutations).length : 0})`,
    },
    { id: 'http' as const, label: `HTTP (${response?.http_requests.length ?? 0})` },
  ];

  const outputString =
    response?.output !== undefined && response?.output !== null
      ? JSON.stringify(response.output, null, 2)
      : (response?.raw_output ?? (response?.error ? `Error: ${response.error}` : ''));

  const kvString = response?.kv_mutations ? JSON.stringify(response.kv_mutations, null, 2) : '{}';

  const httpString = response?.http_requests
    ? JSON.stringify(response.http_requests, null, 2)
    : '[]';

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', width: '100%' }}>
      <div
        style={{ display: 'flex', borderBottom: '1px solid #d0d7de', backgroundColor: '#f6f8fa' }}
      >
        {tabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setUserTab(tab.id)}
            style={{
              padding: '8px 14px',
              fontSize: '12px',
              fontWeight: activeTab === tab.id ? 600 : 400,
              color: activeTab === tab.id ? '#0969da' : '#57606a',
              border: 'none',
              borderBottom: activeTab === tab.id ? '2px solid #0969da' : '2px solid transparent',
              backgroundColor: 'transparent',
              cursor: 'pointer',
            }}
          >
            {tab.label}
          </button>
        ))}
      </div>

      <div style={{ flex: 1, overflow: 'hidden' }}>
        {activeTab === 'output' && <CodeEditor value={outputString} readOnly />}
        {activeTab === 'preview' && <PreviewIframe html={previewHtml} loading={previewLoading} />}
        {activeTab === 'logs' && <LogsViewer logs={response?.logs ?? []} />}
        {activeTab === 'kv' && <CodeEditor value={kvString} readOnly />}
        {activeTab === 'http' && <CodeEditor value={httpString} readOnly />}
      </div>
    </div>
  );
}
