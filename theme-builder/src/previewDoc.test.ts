import { describe, expect, it } from 'vitest';
import { bufferReducer, emptyBuffers, type BufferState } from './buffers.ts';
import {
  buildPreviewDoc,
  describeDiagnostic,
  diagnosticPath,
  renderRequest,
} from './previewDoc.ts';

function withEdits(edits: Record<string, string>): BufferState {
  let state = emptyBuffers;
  for (const [path, content] of Object.entries(edits)) {
    state = bufferReducer(state, { type: 'loaded', path, content: 'saved' });
    state = bufferReducer(state, { type: 'edit', path, content });
  }
  return state;
}

describe('renderRequest', () => {
  it('sends only the unsaved templates and samples, by their short names', () => {
    const state = withEdits({
      'templates/article.html': '<b>x</b>',
      'templates/partials/card.html': 'card',
      'samples/long.json': '{}',
      'assets/style.css': 'body{}',
      'theme.json': '{}',
    });
    expect(renderRequest('article', state)).toEqual({
      sample: 'article',
      templates: { 'article.html': '<b>x</b>', 'partials/card.html': 'card' },
      samples: { long: '{}' },
    });
  });

  it('sends nothing for files that are saved', () => {
    const state = bufferReducer(emptyBuffers, {
      type: 'loaded',
      path: 'templates/a.html',
      content: 'x',
    });
    expect(renderRequest('a', state)).toEqual({ sample: 'a', templates: {}, samples: {} });
  });

  it('includes block parameter when provided', () => {
    expect(renderRequest('a', emptyBuffers, 'content')).toEqual({
      sample: 'a',
      block: 'content',
      templates: {},
      samples: {},
    });
  });
});

describe('buildPreviewDoc', () => {
  const page = '<html><head><title>t</title></head><body><p>hi</p></body></html>';

  it('adds unsaved CSS at the end of <head> and keeps links inside the frame', () => {
    const doc = buildPreviewDoc(page, withEdits({ 'assets/style.css': 'p{color:red}' }));
    expect(doc).toContain('<style data-unsaved="assets/style.css">p{color:red}</style></head>');
    expect(doc.indexOf('<script>')).toBeGreaterThan(doc.indexOf('<p>hi</p>'));
    expect(doc.endsWith('</body></html>')).toBe(true);
  });

  it('cannot be broken out of the <style> element by the CSS', () => {
    const doc = buildPreviewDoc(
      page,
      withEdits({ 'assets/a.css': 'a{}</style><script>x</script>' })
    );
    expect(doc.match(/<\/style>/g)).toHaveLength(1);
  });

  it('works for a fragment without <head> and <body>', () => {
    const doc = buildPreviewDoc('<p>x</p>', withEdits({ 'assets/a.css': 'a{}' }));
    expect(doc.startsWith('<p>x</p>')).toBe(true);
    expect(doc).toContain('<script>');
  });
});

describe('diagnostics', () => {
  it('points at the template and line', () => {
    const d = { message: 'syntax error', template: 'article.html', line: 3 };
    expect(diagnosticPath(d)).toBe('templates/article.html');
    expect(describeDiagnostic(d)).toBe('article.html:3 — syntax error');
    expect(diagnosticPath({ message: 'm', template: null, line: null })).toBeNull();
    expect(describeDiagnostic({ message: 'm', template: null, line: null })).toBe('m');
  });
});
