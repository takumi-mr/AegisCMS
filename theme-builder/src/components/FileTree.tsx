import { useState, type FormEvent } from 'react';
import { FilePlus } from 'lucide-react';
import { isDirty, type BufferState } from '../buffers.ts';
import { isValidNewFilePath } from '../newFile.ts';
import type { FileKind, ThemeFile } from '../types.ts';

const GROUPS: { kind: FileKind; label: string }[] = [
  { kind: 'manifest', label: 'テーマ' },
  { kind: 'template', label: 'テンプレート' },
  { kind: 'sample', label: 'サンプルデータ' },
  { kind: 'asset', label: 'アセット' },
];

export interface FileTreeProps {
  files: ThemeFile[];
  buffers: BufferState;
  writable: boolean;
  onOpen: (path: string) => void;
  onCreate: (path: string) => void;
}

export function FileTree({ files, buffers, writable, onOpen, onCreate }: FileTreeProps) {
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState('templates/');
  const valid = isValidNewFilePath(name) && !files.some((f) => f.path === name);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!valid) return;
    onCreate(name);
    setCreating(false);
    setName('templates/');
  };

  return (
    <nav className="files" aria-label="ファイル">
      {GROUPS.map(({ kind, label }) => {
        const group = files.filter((f) => f.kind === kind);
        if (group.length === 0) return null;
        return (
          <section key={kind}>
            <h2>{label}</h2>
            <ul>
              {group.map((file) => {
                const buffer = buffers.buffers[file.path];
                return (
                  <li key={file.path}>
                    <button
                      type="button"
                      className={buffers.active === file.path ? 'file active' : 'file'}
                      disabled={!file.editable}
                      title={file.editable ? file.path : `${file.path}（編集できない形式）`}
                      onClick={() => onOpen(file.path)}
                    >
                      <span className="file-name">
                        {kind === 'manifest'
                          ? file.path
                          : file.path.slice(file.path.indexOf('/') + 1)}
                      </span>
                      {buffer && isDirty(buffer) && <span className="dot" aria-label="未保存" />}
                    </button>
                  </li>
                );
              })}
            </ul>
          </section>
        );
      })}
      {writable &&
        (creating ? (
          <form className="new-file" onSubmit={submit}>
            <input
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              aria-label="新しいファイルのパス"
              spellCheck={false}
            />
            <div className="row">
              <button type="submit" disabled={!valid}>
                作成
              </button>
              <button type="button" onClick={() => setCreating(false)}>
                やめる
              </button>
            </div>
            <p className="hint">templates/・samples/・assets/ の下に作れます</p>
          </form>
        ) : (
          <button type="button" className="add" onClick={() => setCreating(true)}>
            <FilePlus size={14} /> 新しいファイル
          </button>
        ))}
    </nav>
  );
}
