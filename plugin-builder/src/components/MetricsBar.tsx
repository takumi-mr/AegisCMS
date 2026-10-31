import { Zap, Clock, Check, X } from 'lucide-react';
import type { InvokeResponse } from '../types.ts';

interface MetricsBarProps {
  response: InvokeResponse | null;
  maxFuel?: number;
}

export function MetricsBar({ response, maxFuel = 100_000_000 }: MetricsBarProps) {
  if (!response) {
    return (
      <div
        style={{
          padding: '8px 12px',
          fontSize: '12px',
          color: '#6e7781',
          borderBottom: '1px solid #d0d7de',
          backgroundColor: '#f6f8fa',
        }}
      >
        Ready to execute. Press Run or Ctrl+Enter.
      </div>
    );
  }

  const fuelPercent = Math.min(100, (response.fuel_consumed / maxFuel) * 100);

  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: '16px',
        padding: '8px 12px',
        fontSize: '12px',
        borderBottom: '1px solid #d0d7de',
        backgroundColor: response.success ? '#f6f8fa' : '#ffebe9',
      }}
    >
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: '4px',
          fontWeight: 600,
          color: response.success ? '#1a7f37' : '#cf222e',
        }}
      >
        {response.success ? <Check size={14} /> : <X size={14} />}
        <span>{response.success ? 'Success' : 'Failed'}</span>
      </div>

      <div style={{ display: 'flex', alignItems: 'center', gap: '4px', color: '#24292f' }}>
        <Clock size={13} color="#57606a" />
        <span>{response.duration_ms.toFixed(2)} ms</span>
      </div>

      <div style={{ display: 'flex', alignItems: 'center', gap: '6px', color: '#24292f' }}>
        <Zap size={13} color="#9a6700" />
        <span>{response.fuel_consumed.toLocaleString()} fuel</span>
        <div
          style={{
            width: '60px',
            height: '6px',
            backgroundColor: '#d0d7de',
            borderRadius: '3px',
            overflow: 'hidden',
          }}
        >
          <div
            style={{
              width: `${fuelPercent}%`,
              height: '100%',
              backgroundColor: fuelPercent > 80 ? '#cf222e' : '#1f883d',
            }}
          />
        </div>
      </div>

      {response.error && (
        <div
          style={{
            color: '#cf222e',
            fontWeight: 500,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}
        >
          {response.error}
        </div>
      )}
    </div>
  );
}
