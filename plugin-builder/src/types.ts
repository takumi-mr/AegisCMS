export interface PluginManifest {
  name: string;
  version: string;
  entrypoint: string;
  capabilities: {
    hooks: string[];
    network?: {
      allowed_domains: string[];
    };
    storage?: {
      kv_namespace: string;
    };
  };
  resource_limits: {
    max_memory_mb: number;
    max_fuel_units: number;
    execution_timeout_ms: number;
  };
}

export interface PluginInfo {
  manifest: PluginManifest;
  entrypoint: string;
  wasm_exists: boolean;
  wasm_size_bytes: number;
  exports: string[];
  updated_at: number;
}

export interface PluginLogEntry {
  level: string;
  message: string;
  timestamp: string;
}

export interface HttpRequest {
  method: string;
  url: string;
  headers: Record<string, string>;
  body?: string;
}

export interface MockHttpRule {
  url_pattern: string;
  status: number;
  body: string;
  headers?: Record<string, string>;
}

export interface InvokeRequest {
  hook: string;
  export?: string;
  input: unknown;
  config?: Record<string, string>;
  kv?: Record<string, string>;
  mock_http?: MockHttpRule[];
}

export interface InvokeResponse {
  success: boolean;
  output: unknown;
  raw_output: string | null;
  error: string | null;
  fuel_consumed: number;
  duration_ms: number;
  logs: PluginLogEntry[];
  kv_mutations: Record<string, string>;
  http_requests: HttpRequest[];
}
