import { describe, expect, it } from 'vitest';
import { buildPreviewWindowDocument } from './previewWindow';

describe('buildPreviewWindowDocument', () => {
  it('puts the page in a script-only sandboxed iframe, never inline', () => {
    const doc = buildPreviewWindowDocument('<p>hi</p><script>alert(1)</script>', 'T');
    expect(doc).toContain('<iframe sandbox="allow-scripts" srcdoc="');
    expect(doc).not.toContain('allow-same-origin');
    // The page markup only appears escaped, inside the attribute
    expect(doc).not.toContain('<script>');
    expect(doc).toContain('&lt;p&gt;hi&lt;/p&gt;');
  });

  it('escapes quotes and ampersands so the page cannot break out of the attribute', () => {
    const doc = buildPreviewWindowDocument('<a href="x" title=\'a&b\'>', 'T');
    expect(doc).toContain('href=&quot;x&quot;');
    expect(doc).toContain('a&amp;b');
    expect(doc.match(/srcdoc="/g)).toHaveLength(1);
  });

  it('escapes the window title', () => {
    expect(buildPreviewWindowDocument('', '</title><script>x</script>')).toContain(
      '<title>&lt;/title&gt;&lt;script&gt;x&lt;/script&gt;</title>'
    );
  });
});
