import { useState, type FormEvent } from 'react';
import { Check, FilePlus, Pencil, Trash2, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { isDirty, type BufferState } from '../buffers.ts';
import { isValidNewFilePath } from '../newFile.ts';
import type { ThemeFile } from '../types.ts';

const GROUPS = [
  { kind: 'manifest' as const, key: 'files.theme' as const },
  { kind: 'template' as const, key: 'files.template' as const },
  { kind: 'sample' as const, key: 'files.sample' as const },
  { kind: 'asset' as const, key: 'files.asset' as const },
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
  const { t } = useTranslation();
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
    <nav className="files" aria-label={t('files.title')}>
      {GROUPS.map(({ kind, key }) => {
        const group = files.filter((f) => f.kind === kind);
        if (group.length === 0) return null;
        return (
          <section key={kind}>
            <h2>{t(key)}</h2>
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
                          aria-label={t('files.newPathAria')}
                          spellCheck={false}
                        />
                        <div className="rename-actions">
                          <button
                            type="submit"
                            disabled={!isRenameValid}
                            title={t('files.confirmRename')}
                            aria-label={t('files.confirmRename')}
                          >
                            <Check size={12} />
                          </button>
                          <button
                            type="button"
                            onClick={() => setRenamingPath(null)}
                            title={t('common.cancel')}
                            aria-label={t('common.cancel')}
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
                      title={file.editable ? file.path : `${file.path}${t('files.nonEditable')}`}
                      onClick={() => onOpen(file.path)}
                    >
                      <span className="file-name">
                        {kind === 'manifest'
                          ? file.path
                          : file.path.slice(file.path.indexOf('/') + 1)}
                      </span>
                      {buffer && isDirty(buffer) && (
                        <span className="dot" aria-label={t('app.unsaved')} />
                      )}
                    </button>
                    {writable && kind !== 'manifest' && (
                      <div className="file-actions">
                        <button
                          type="button"
                          className="action-btn"
                          title={t('files.rename')}
                          aria-label={`${file.path} ${t('files.rename')}`}
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
                          title={t('files.delete')}
                          aria-label={`${file.path} ${t('files.delete')}`}
                          onClick={(e) => {
                            e.stopPropagation();
                            if (window.confirm(t('files.deleteConfirm', { path: file.path }))) {
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
              aria-label={t('files.newFileAria')}
              spellCheck={false}
            />
            <div className="row">
              <button type="submit" disabled={!valid}>
                {t('common.create')}
              </button>
              <button type="button" onClick={() => setCreating(false)}>
                {t('common.cancel')}
              </button>
            </div>
            <p className="hint">{t('files.newFileHint')}</p>
          </form>
        ) : (
          <button type="button" className="add" onClick={() => setCreating(true)}>
            <FilePlus size={14} /> {t('files.newFile')}
          </button>
        ))}
    </nav>
  );
}
