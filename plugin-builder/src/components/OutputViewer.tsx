import { useState } from 'react';
import { CodeEditor } from './CodeEditor.tsx';
import { LogsViewer } from './LogsViewer.tsx';
import type { InvokeResponse } from '../types.ts';

interface OutputViewerProps {
  response: InvokeResponse | null;
}

export function OutputViewer({ response }: OutputViewerProps) {
  const [activeTab, setActiveTab] = useState<'output' | 'logs' | 'kv' | 'http'>('output');

  const tabs = [
    { id: 'output', label: 'Output' },
    { id: 'logs', label: `Logs (${response?.logs.length ?? 0})` },
    { id: 'kv', label: `KV Changes (${response ? Object.keys(response.kv_mutations).length : 0})` },
    { id: 'http', label: `HTTP (${response?.http_requests.length ?? 0})` },
  ] as const;

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
            onClick={() => setActiveTab(tab.id)}
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
        {activeTab === 'logs' && <LogsViewer logs={response?.logs ?? []} />}
        {activeTab === 'kv' && <CodeEditor value={kvString} readOnly />}
        {activeTab === 'http' && <CodeEditor value={httpString} readOnly />}
      </div>
    </div>
  );
}
