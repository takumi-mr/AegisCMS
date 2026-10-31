import { useState, type FormEvent } from 'react';
import { Check, FilePlus, Pencil, Trash2, X } from 'lucide-react';
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
  onRename: (from: string, to: string) => void;
  onDelete: (path: string) => void;
}

export function FileTree({
  files,
  buffers,
  writable,
  onOpen,
  onCreate,
  onRename,
  onDelete,
}: FileTreeProps) {
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState('templates/');
  const [renamingPath, setRenamingPath] = useState<string | null>(null);
  const [renamingValue, setRenamingValue] = useState('');

  const valid = isValidNewFilePath(name) && !files.some((f) => f.path === name);

  const submitCreate = (event: FormEvent) => {
    event.preventDefault();
    if (!valid) return;
    onCreate(name);
    setCreating(false);
    setName('templates/');
  };

  const isRenameValid =
    renamingPath !== null &&
    isValidNewFilePath(renamingValue) &&
    renamingValue !== renamingPath &&
    !files.some((f) => f.path === renamingValue);

  const submitRename = (event: FormEvent) => {
    event.preventDefault();
    if (!renamingPath || !isRenameValid) return;
    onRename(renamingPath, renamingValue);
    setRenamingPath(null);
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
                const isRenaming = renamingPath === file.path;

                if (isRenaming) {
                  return (
                    <li key={file.path}>
                      <form className="rename-form" onSubmit={submitRename}>
                        <input
                          autoFocus
                          value={renamingValue}
                          onChange={(e) => setRenamingValue(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === 'Escape') setRenamingPath(null);
                          }}
                          aria-label="改名後のパス"
                          spellCheck={false}
                        />
                        <div className="rename-actions">
                          <button
                            type="submit"
                            disabled={!isRenameValid}
                            title="改名を確定"
                            aria-label="改名を確定"
                          >
                            <Check size={12} />
                          </button>
                          <button
                            type="button"
                            onClick={() => setRenamingPath(null)}
                            title="キャンセル"
                            aria-label="キャンセル"
                          >
                            <X size={12} />
                          </button>
                        </div>
                      </form>
                    </li>
                  );
                }

                return (
                  <li key={file.path} className="file-item">
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
                    {writable && kind !== 'manifest' && (
                      <div className="file-actions">
                        <button
                          type="button"
                          className="action-btn"
                          title="改名"
                          aria-label={`${file.path} を改名`}
                          onClick={(e) => {
                            e.stopPropagation();
                            setRenamingPath(file.path);
                            setRenamingValue(file.path);
                          }}
                        >
                          <Pencil size={12} />
                        </button>
                        <button
                          type="button"
                          className="action-btn danger"
                          title="削除"
                          aria-label={`${file.path} を削除`}
                          onClick={(e) => {
                            e.stopPropagation();
                            if (window.confirm(`${file.path} を削除しますか？`)) {
                              onDelete(file.path);
                            }
                          }}
                        >
                          <Trash2 size={12} />
                        </button>
                      </div>
                    )}
                  </li>
                );
              })}
            </ul>
          </section>
        );
      })}
      {writable &&
        (creating ? (
          <form className="new-file" onSubmit={submitCreate}>
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
