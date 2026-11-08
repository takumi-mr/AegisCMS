/** Escape text for a double-quoted HTML attribute value */
const escapeAttribute = (value: string): string =>
  value.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

const escapeText = (value: string): string =>
  value.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

/**
 * The document opened in a separate window. A `blob:` page shares the admin's origin, so the
 * themed page is never put in it directly: it only holds a script-only sandboxed iframe (opaque
 * origin, like the in-editor preview), which keeps the session token out of reach of theme and
 * plugin scripts.
 */
export const buildPreviewWindowDocument = (pageHtml: string, title: string): string =>
  `<!doctype html><html><head><meta charset="utf-8"><title>${escapeText(title)}</title>` +
  `<style>html,body{margin:0;height:100%;background:#fff}iframe{display:block;width:100%;height:100%;border:0}</style>` +
  `</head><body><iframe sandbox="allow-scripts" srcdoc="${escapeAttribute(pageHtml)}"></iframe></body></html>`;

/**
 * Open `pageHtml` (a snapshot, not live) in a new window. `noopener` drops `window.opener`, so
 * the new window cannot reach the admin.
 */
export const openPreviewWindow = (pageHtml: string, title: string): void => {
  const url = URL.createObjectURL(
    new Blob([buildPreviewWindowDocument(pageHtml, title)], { type: 'text/html' })
  );
  window.open(url, '_blank', 'noopener');
  // Release the URL once the new window has had time to load it
  window.setTimeout(() => URL.revokeObjectURL(url), 60_000);
};
