import { useEffect, useRef } from 'react';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { css } from '@codemirror/lang-css';
import { html } from '@codemirror/lang-html';
import { javascript } from '@codemirror/lang-javascript';
import { jinja } from '@codemirror/lang-jinja';
import { json } from '@codemirror/lang-json';
import {
  bracketMatching,
  HighlightStyle,
  indentOnInput,
  syntaxHighlighting,
} from '@codemirror/language';
import { lintGutter, setDiagnostics, type Diagnostic as CmDiagnostic } from '@codemirror/lint';
import { EditorState, type Extension } from '@codemirror/state';
import {
  drawSelection,
  EditorView,
  highlightActiveLine,
  keymap,
  lineNumbers,
} from '@codemirror/view';
import { tags as t } from '@lezer/highlight';
import { detectEol } from '../eol.ts';
import { useLatest } from '../useLatest.ts';

export interface EditorProps {
  path: string;
  value: string;
  readOnly: boolean;
  /** Error lines (1-based) with their messages, drawn in the gutter and the text */
  problems: { line: number; message: string }[];
  /** Move the cursor to this line when `nonce` changes (a click on a problem) */
  jump: { line: number; nonce: number } | null;
  onChange: (value: string) => void;
  onSave: () => void;
}

/** Colours come from CSS variables, so the editor follows the page's light/dark theme */
const highlight = HighlightStyle.define([
  { tag: [t.keyword, t.operatorKeyword, t.controlKeyword], color: 'var(--tok-keyword)' },
  { tag: [t.string, t.special(t.string)], color: 'var(--tok-string)' },
  { tag: [t.number, t.bool, t.null, t.atom], color: 'var(--tok-number)' },
  {
    tag: [t.comment, t.lineComment, t.blockComment],
    color: 'var(--tok-comment)',
    fontStyle: 'italic',
  },
  { tag: [t.tagName, t.typeName, t.className], color: 'var(--tok-tag)' },
  { tag: [t.attributeName, t.propertyName], color: 'var(--tok-attr)' },
  { tag: [t.variableName, t.definition(t.variableName)], color: 'var(--tok-variable)' },
  { tag: [t.function(t.variableName), t.labelName], color: 'var(--tok-function)' },
  { tag: [t.processingInstruction, t.meta, t.angleBracket], color: 'var(--tok-meta)' },
]);

const theme = EditorView.theme({
  '&': { height: '100%', backgroundColor: 'var(--surface)', color: 'var(--text)' },
  '.cm-scroller': { fontFamily: 'var(--mono)', fontSize: '13px', lineHeight: '1.55' },
  '.cm-gutters': {
    backgroundColor: 'var(--surface-2)',
    color: 'var(--muted)',
    border: 'none',
    borderRight: '1px solid var(--border)',
  },
  '.cm-activeLine': { backgroundColor: 'var(--active-line)' },
  '.cm-activeLineGutter': { backgroundColor: 'var(--active-line)' },
  '&.cm-focused': { outline: 'none' },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground': {
    backgroundColor: 'var(--selection) !important',
  },
  '.cm-cursor': { borderLeftColor: 'var(--text)' },
  '.cm-lintRange-error': {
    backgroundImage: 'none',
    textDecoration: 'underline wavy var(--danger)',
  },
});

function language(path: string): Extension {
  const ext = path.slice(path.lastIndexOf('.') + 1).toLowerCase();
  switch (ext) {
    case 'html':
      // Jinja tags inside HTML (minijinja uses the same syntax)
      return jinja({ base: html() });
    case 'svg':
      return html();
    case 'css':
      return css();
    case 'json':
      return json();
    case 'js':
    case 'mjs':
      return javascript();
    default:
      return [];
  }
}

function toDiagnostics(view: EditorView, problems: EditorProps['problems']): CmDiagnostic[] {
  const doc = view.state.doc;
  return problems
    .filter((p) => p.line >= 1 && p.line <= doc.lines)
    .map((p) => {
      const line = doc.line(p.line);
      return { from: line.from, to: line.to, severity: 'error', message: p.message };
    });
}

/** One CodeMirror view per file (remount with `key={path}`); `value` follows changes made elsewhere */
export function Editor({ path, value, readOnly, problems, jump, onChange, onSave }: EditorProps) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  // The latest callbacks, without rebuilding the editor when they change
  const handlers = useLatest({ onChange, onSave });
  // The content at mount: later changes are applied by the effect below
  const initial = useRef(value);

  useEffect(() => {
    if (!host.current) return;
    const editor = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: initial.current,
        extensions: [
          lineNumbers(),
          history(),
          drawSelection(),
          indentOnInput(),
          bracketMatching(),
          highlightActiveLine(),
          lintGutter(),
          syntaxHighlighting(highlight),
          language(path),
          theme,
          EditorState.readOnly.of(readOnly),
          // Keep the file's own line breaks: `sliceDoc()` joins the lines with them
          EditorState.lineSeparator.of(detectEol(initial.current)),
          EditorView.lineWrapping,
          keymap.of([
            {
              key: 'Mod-s',
              preventDefault: true,
              run: () => {
                handlers.current.onSave();
                return true;
              },
            },
            indentWithTab,
            ...defaultKeymap,
            ...historyKeymap,
          ]),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) handlers.current.onChange(update.state.sliceDoc());
          }),
        ],
      }),
    });
    view.current = editor;
    return () => {
      editor.destroy();
      view.current = null;
    };
  }, [path, readOnly, handlers]);

  useEffect(() => {
    const editor = view.current;
    if (editor && editor.state.sliceDoc() !== value) {
      editor.dispatch({ changes: { from: 0, to: editor.state.doc.length, insert: value } });
    }
  }, [value]);

  useEffect(() => {
    const editor = view.current;
    if (editor) editor.dispatch(setDiagnostics(editor.state, toDiagnostics(editor, problems)));
  }, [problems, path, value]);

  useEffect(() => {
    const editor = view.current;
    if (!editor || !jump || jump.line < 1 || jump.line > editor.state.doc.lines) return;
    const line = editor.state.doc.line(jump.line);
    editor.dispatch({
      selection: { anchor: line.from },
      effects: EditorView.scrollIntoView(line.from, { y: 'center' }),
    });
    editor.focus();
  }, [jump, path]);

  return <div className="editor" ref={host} data-testid="editor" />;
}
