/** `GET /__dev/api/files` */
export type FileKind = 'manifest' | 'template' | 'sample' | 'asset';

export interface ThemeFile {
  path: string;
  kind: FileKind;
  editable: boolean;
}

export interface FileList {
  files: ThemeFile[];
  /** false when the dev server does not listen on loopback (read only) */
  writable: boolean;
}

/** A failed render: which template, which line (1-based) */
export interface Diagnostic {
  message: string;
  template: string | null;
  line: number | null;
}

export interface SampleInfo {
  name: string;
  template: string;
  source: 'file' | 'builtin';
  preview_url: string;
}

/** `GET /__dev/api/theme` */
export interface ThemeInfo {
  manifest: { id: string; name: string; version: string } | null;
  templates: string[];
  samples: SampleInfo[];
  sample_errors: string[];
  version: number;
}

/** `POST /__dev/api/render` */
export interface BlockInfo {
  name: string;
  template: string;
  line: number;
}

export type RenderResult =
  | { ok: true; html: string; sample_errors: string[]; blocks?: BlockInfo[] }
  | { ok: false; error: Diagnostic; sample_errors: string[]; blocks?: BlockInfo[] };

export interface RenderRequest {
  sample: string;
  block?: string | null;
  /** Unsaved templates, keyed by the name under templates/ (`article.html`) */
  templates: Record<string, string>;
  /** Unsaved sample files, keyed by the file name without `.json` */
  samples: Record<string, string>;
}

/** `GET /__dev/api/check` */
export interface CheckResult {
  ok: boolean;
  results: { name: string; template: string; ok: boolean; error?: Diagnostic }[];
  sample_errors: string[];
}
