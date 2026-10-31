import { useEffect, useRef } from 'react';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { json } from '@codemirror/lang-json';
import {
  bracketMatching,
  HighlightStyle,
  indentOnInput,
  syntaxHighlighting,
} from '@codemirror/language';
import { EditorState } from '@codemirror/state';
import {
  drawSelection,
  EditorView,
  highlightActiveLine,
  keymap,
  lineNumbers,
} from '@codemirror/view';
import { tags as t } from '@lezer/highlight';
import { useLatest } from '../useLatest.ts';

export interface CodeEditorProps {
  value: string;
  readOnly?: boolean;
  onChange?: (value: string) => void;
  onExecute?: () => void;
}

const highlight = HighlightStyle.define([
  { tag: [t.keyword, t.operatorKeyword, t.controlKeyword], color: '#cf222e' },
  { tag: [t.string, t.special(t.string)], color: '#0a3069' },
  { tag: [t.number, t.bool, t.null, t.atom], color: '#0550ae' },
  { tag: [t.propertyName], color: '#116329' },
  { tag: [t.comment], color: '#6e7781', fontStyle: 'italic' },
]);

const editorTheme = EditorView.theme({
  '&': { height: '100%', backgroundColor: '#ffffff', color: '#1f2328' },
  '.cm-scroller': {
    fontFamily: 'ui-monospace, SFMono-Regular, monospace',
    fontSize: '13px',
    lineHeight: '1.5',
  },
  '.cm-gutters': {
    backgroundColor: '#f6f8fa',
    color: '#8c959f',
    border: 'none',
    borderRight: '1px solid #d0d7de',
  },
  '.cm-activeLine': { backgroundColor: '#f6f8fa' },
  '.cm-activeLineGutter': { backgroundColor: '#eaeef2' },
  '&.cm-focused': { outline: 'none' },
});

export function CodeEditor({ value, readOnly = false, onChange, onExecute }: CodeEditorProps) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const callbacks = useLatest({ onChange, onExecute });
  const initialValue = useRef(value);

  useEffect(() => {
    if (!host.current) return;

    const state = EditorState.create({
      doc: initialValue.current,
      extensions: [
        lineNumbers(),
        history(),
        drawSelection(),
        indentOnInput(),
        bracketMatching(),
        highlightActiveLine(),
        syntaxHighlighting(highlight),
        json(),
        editorTheme,
        EditorState.readOnly.of(readOnly),
        EditorView.lineWrapping,
        keymap.of([
          {
            key: 'Mod-Enter',
            preventDefault: true,
            run: () => {
              callbacks.current.onExecute?.();
              return true;
            },
          },
          indentWithTab,
          ...defaultKeymap,
          ...historyKeymap,
        ]),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            callbacks.current.onChange?.(update.state.doc.toString());
          }
        }),
      ],
    });

    const editor = new EditorView({
      state,
      parent: host.current,
    });
    view.current = editor;

    return () => {
      editor.destroy();
      view.current = null;
    };
  }, [readOnly, callbacks]);

  useEffect(() => {
    const editor = view.current;
    if (editor && editor.state.doc.toString() !== value) {
      editor.dispatch({
        changes: { from: 0, to: editor.state.doc.length, insert: value },
      });
    }
  }, [value]);

  return <div style={{ height: '100%', width: '100%', overflow: 'hidden' }} ref={host} />;
}
