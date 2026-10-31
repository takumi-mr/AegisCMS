import { useCallback, useEffect, useMemo, useReducer, useState } from 'react';
import { CircleAlert, Save, WifiOff, X } from 'lucide-react';
import { api } from './api.ts';
import { bufferReducer, dirtyBuffers, emptyBuffers, isDirty, type BufferState } from './buffers.ts';
import { Editor } from './components/Editor.tsx';
import { FileTree } from './components/FileTree.tsx';
import { Preview } from './components/Preview.tsx';
import { Problems } from './components/Problems.tsx';
import { useLatest } from './useLatest.ts';
import { buildPreviewDoc, diagnosticPath, renderRequest, unsavedCss } from './previewDoc.ts';
import type { ViewportId } from './viewports.ts';
import type { CheckResult, Diagnostic, FileList, ThemeInfo } from './types.ts';

/** Wait before rendering, so typing does not render on every key */
const RENDER_DELAY_MS = 250;
const RETRY_MS = 2000;

interface RenderState {
  html: string | null;
  /** The buffers the page was rendered with (for the unsaved CSS) */
  snapshot: BufferState;
  error: Diagnostic | null;
  sampleErrors: string[];
  pending: boolean;
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export function App() {
  const [info, setInfo] = useState<ThemeInfo | null>(null);
  const [fileList, setFileList] = useState<FileList>({ files: [], writable: false });
  const [buffers, dispatch] = useReducer(bufferReducer, emptyBuffers);
  const [sample, setSample] = useState('');
  const [viewport, setViewport] = useState<ViewportId>('fit');
  const [render, setRender] = useState<RenderState>({
    html: null,
    snapshot: emptyBuffers,
    error: null,
    sampleErrors: [],
    pending: false,
  });
  const [check, setCheck] = useState<CheckResult | null>(null);
  const [checking, setChecking] = useState(false);
  const [jump, setJump] = useState<{ line: number; nonce: number } | null>(null);
  const [offline, setOffline] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  // Bumped when files change on disk, to render again
  const [diskVersion, setDiskVersion] = useState(0);

  const buffersRef = useLatest(buffers);

  const fail = useCallback((e: unknown) => {
    setNotice(e instanceof Error ? e.message : String(e));
  }, []);

  // --- files -----------------------------------------------------------------------------------

  const openFile = useCallback(
    async (path: string) => {
      if (buffersRef.current.buffers[path]) {
        dispatch({ type: 'activate', path });
        return;
      }
      try {
        const file = await api.readFile(path);
        dispatch({ type: 'loaded', path, content: file.content });
      } catch (e) {
        fail(e);
      }
    },
    [fail, buffersRef]
  );

  const reloadFromDisk = useCallback(
    async (path: string) => {
      try {
        const file = await api.readFile(path);
        dispatch({ type: 'loaded', path, content: file.content });
      } catch (e) {
        fail(e);
      }
    },
    [fail]
  );

  const save = useCallback(
    async (path: string) => {
      const buffer = buffersRef.current.buffers[path];
      if (!buffer || !isDirty(buffer)) return;
      try {
        await api.writeFile(path, buffer.content);
        dispatch({ type: 'saved', path, content: buffer.content });
        setNotice(null);
      } catch (e) {
        fail(e);
      }
    },
    [fail, buffersRef]
  );

  const saveAll = useCallback(async () => {
    for (const buffer of dirtyBuffers(buffersRef.current)) await save(buffer.path);
  }, [save, buffersRef]);

  const createFile = useCallback(
    async (path: string) => {
      try {
        await api.writeFile(path, '');
        setFileList(await api.files());
        await reloadFromDisk(path);
      } catch (e) {
        fail(e);
      }
    },
    [fail, reloadFromDisk]
  );

  const deleteFile = useCallback(
    async (path: string) => {
      try {
        await api.deleteFile(path);
        dispatch({ type: 'close', path });
        const [themeInfo, files] = await Promise.all([api.theme(), api.files()]);
        setInfo(themeInfo);
        setFileList(files);
        setNotice(null);
      } catch (e) {
        fail(e);
      }
    },
    [fail]
  );

  const renameFile = useCallback(
    async (from: string, to: string) => {
      try {
        const buffer = buffersRef.current.buffers[from];
        if (buffer && isDirty(buffer)) {
          await api.writeFile(from, buffer.content);
        }
        await api.renameFile(from, to);
        dispatch({ type: 'renamed', from, to });
        const [themeInfo, files] = await Promise.all([api.theme(), api.files()]);
        setInfo(themeInfo);
        setFileList(files);
        setNotice(null);
      } catch (e) {
        fail(e);
      }
    },
    [fail, buffersRef]
  );

  // --- first load ------------------------------------------------------------------------------

  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const [themeInfo, files] = await Promise.all([api.theme(), api.files()]);
        if (!alive) return;
        setInfo(themeInfo);
        setFileList(files);
        const first = themeInfo.samples.find((s) => s.name === 'index') ?? themeInfo.samples[0];
        if (first) {
          setSample(first.name);
          await openFile(`templates/${first.template}`);
        }
      } catch (e) {
        if (alive) {
          setOffline(true);
          fail(e);
        }
      }
    })();
    return () => {
      alive = false;
    };
  }, [openFile, fail]);

  // --- changes on disk (an editor outside this app, or our own save) --------------------------

  const refresh = useCallback(async () => {
    try {
      const [themeInfo, files] = await Promise.all([api.theme(), api.files()]);
      setInfo(themeInfo);
      setFileList(files);
      setCheck(null);
      await Promise.all(
        buffersRef.current.open.map(async (path) => {
          try {
            const file = await api.readFile(path);
            dispatch({ type: 'external', path, content: file.content });
          } catch {
            // deleted or unreadable: keep the buffer
          }
        })
      );
      setDiskVersion((v) => v + 1);
    } catch {
      // the version loop reports the connection
    }
  }, [buffersRef]);

  useEffect(() => {
    const controller = new AbortController();
    (async () => {
      let since: number | undefined;
      while (!controller.signal.aborted) {
        try {
          const { version } = await api.version(since, controller.signal);
          setOffline(false);
          if (since !== undefined && version !== since) await refresh();
          since = version;
        } catch {
          if (controller.signal.aborted) return;
          setOffline(true);
          await sleep(RETRY_MS);
        }
      }
    })();
    return () => controller.abort();
  }, [refresh]);

  // --- preview ---------------------------------------------------------------------------------

  // Show the page that uses the file being edited
  const active = buffers.active;
  const infoRef = useLatest(info);
  useEffect(() => {
    if (!infoRef.current || !active) return;
    const template = /^templates\/(.+)$/.exec(active)?.[1];
    const sampleFile = /^samples\/([^/]+)\.json$/.exec(active)?.[1];
    const match = infoRef.current.samples.find(
      (s) => s.template === template || s.name === sampleFile
    );
    // Only when the tab changes: a page picked by hand stays until another file is opened
    if (match) setSample(match.name);
  }, [active, infoRef]);

  const request = useMemo(() => renderRequest(sample, buffers), [sample, buffers]);
  const requestRef = useLatest(request);
  const renderKey = JSON.stringify([request, unsavedCss(buffers), diskVersion]);
  useEffect(() => {
    if (!sample) return;
    const controller = new AbortController();
    const snapshot = buffersRef.current;
    const timer = setTimeout(async () => {
      setRender((r) => ({ ...r, pending: true }));
      try {
        const result = await api.render(requestRef.current, controller.signal);
        setRender((r) => ({
          html: result.ok ? result.html : r.html,
          snapshot: result.ok ? snapshot : r.snapshot,
          error: result.ok ? null : result.error,
          sampleErrors: result.sample_errors,
          pending: false,
        }));
      } catch (e) {
        if (!controller.signal.aborted) {
          setRender((r) => ({ ...r, pending: false }));
          fail(e);
        }
      }
    }, RENDER_DELAY_MS);
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
    // `request` is derived from `renderKey`, which already covers every input of the render
  }, [renderKey, sample, fail, buffersRef, requestRef]);

  const doc = useMemo(
    () => (render.html === null ? null : buildPreviewDoc(render.html, render.snapshot)),
    [render.html, render.snapshot]
  );

  // --- problems --------------------------------------------------------------------------------

  const runCheck = useCallback(async () => {
    setChecking(true);
    try {
      setCheck(await api.check());
    } catch (e) {
      fail(e);
    } finally {
      setChecking(false);
    }
  }, [fail]);

  const jumpTo = useCallback(
    async (d: Diagnostic) => {
      const path = diagnosticPath(d);
      if (!path) return;
      await openFile(path);
      if (d.line) setJump({ line: d.line, nonce: Date.now() });
    },
    [openFile]
  );

  const activeBuffer = active ? buffers.buffers[active] : undefined;
  const problems = useMemo(() => {
    if (!render.error || !active || diagnosticPath(render.error) !== active || !render.error.line) {
      return [];
    }
    return [{ line: render.error.line, message: render.error.message }];
  }, [render.error, active]);

  const dirtyCount = dirtyBuffers(buffers).length;
  const readOnly = !fileList.writable;

  return (
    <div className="app">
      <header>
        <h1>
          Theme Builder
          {info?.manifest && (
            <span className="theme-name">
              {info.manifest.name} <small>v{info.manifest.version}</small>
            </span>
          )}
        </h1>
        <div className="header-actions">
          {offline && (
            <span className="badge bad" role="alert">
              <WifiOff size={14} /> 開発サーバーにつながりません
            </span>
          )}
          {readOnly && fileList.files.length > 0 && (
            <span className="badge" title="ループバック以外で待ち受けているため、保存できません">
              読み取り専用
            </span>
          )}
          <button
            type="button"
            className="primary"
            onClick={saveAll}
            disabled={dirtyCount === 0 || readOnly}
          >
            <Save size={14} /> すべて保存{dirtyCount > 0 ? `（${dirtyCount}）` : ''}
          </button>
        </div>
      </header>

      {notice && (
        <div className="notice" role="alert">
          <CircleAlert size={14} /> {notice}
          <button type="button" aria-label="閉じる" onClick={() => setNotice(null)}>
            <X size={14} />
          </button>
        </div>
      )}

      <main>
        <FileTree
          files={fileList.files}
          buffers={buffers}
          writable={fileList.writable}
          onOpen={openFile}
          onCreate={createFile}
          onRename={renameFile}
          onDelete={deleteFile}
        />

        <section className="editing" aria-label="エディター">
          <div className="tabs" role="tablist">
            {buffers.open.map((path) => {
              const buffer = buffers.buffers[path];
              return (
                <div key={path} className={path === active ? 'tab active' : 'tab'}>
                  <button
                    type="button"
                    role="tab"
                    aria-selected={path === active}
                    onClick={() => dispatch({ type: 'activate', path })}
                  >
                    {path.split('/').pop()}
                    {buffer && isDirty(buffer) && <span className="dot" aria-label="未保存" />}
                  </button>
                  <button
                    type="button"
                    className="close"
                    aria-label={`${path} を閉じる`}
                    onClick={() => dispatch({ type: 'close', path })}
                  >
                    <X size={12} />
                  </button>
                </div>
              );
            })}
          </div>

          {activeBuffer?.conflict && (
            <div className="notice" role="alert">
              <CircleAlert size={14} /> 編集中に、ディスク上のファイルが変わりました。
              <button type="button" onClick={() => reloadFromDisk(activeBuffer.path)}>
                ディスクの内容を読み込む（編集は捨てる）
              </button>
            </div>
          )}

          {activeBuffer ? (
            <Editor
              key={activeBuffer.path}
              path={activeBuffer.path}
              value={activeBuffer.content}
              readOnly={readOnly}
              problems={problems}
              jump={jump}
              onChange={(content) => dispatch({ type: 'edit', path: activeBuffer.path, content })}
              onSave={() => save(activeBuffer.path)}
            />
          ) : (
            <p className="empty">左のファイルを選んでください</p>
          )}

          <Problems
            current={render.error}
            sampleErrors={
              render.sampleErrors.length ? render.sampleErrors : (info?.sample_errors ?? [])
            }
            check={check}
            checking={checking}
            onCheck={runCheck}
            onJump={jumpTo}
          />
        </section>

        <Preview
          samples={info?.samples ?? []}
          sample={sample}
          viewport={viewport}
          doc={doc}
          failed={render.error !== null}
          pending={render.pending}
          onSample={setSample}
          onViewport={setViewport}
        />
      </main>
    </div>
  );
}
