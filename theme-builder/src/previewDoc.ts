import { dirtyBuffers, type BufferState } from './buffers.ts';
import type { Diagnostic, RenderRequest } from './types.ts';

/** The unsaved templates and samples, to render the page as it would look after saving */
export function renderRequest(sample: string, state: BufferState): RenderRequest {
  const request: RenderRequest = { sample, templates: {}, samples: {} };
  for (const buffer of dirtyBuffers(state)) {
    const template = /^templates\/(.+\.html)$/.exec(buffer.path);
    if (template) request.templates[template[1]] = buffer.content;
    const sampleFile = /^samples\/([^/]+)\.json$/.exec(buffer.path);
    if (sampleFile) request.samples[sampleFile[1]] = buffer.content;
  }
  return request;
}

/** Unsaved stylesheets. The rendered page links the saved ones, so these are added after them */
export function unsavedCss(state: BufferState): { path: string; css: string }[] {
  return dirtyBuffers(state)
    .filter((b) => /^assets\/.+\.css$/.test(b.path))
    .map((b) => ({ path: b.path, css: b.content }));
}

/** Keeps a click on a link in the preview from navigating the frame away from the page */
const KEEP_IN_FRAME =
  "<script>document.addEventListener('click',function(e){var a=e.target&&e.target.closest&&e.target.closest('a[href]');if(a&&!/^#/.test(a.getAttribute('href')))e.preventDefault()})</script>";

function insertBefore(html: string, marker: string, text: string): string {
  const at = html.toLowerCase().lastIndexOf(marker);
  return at === -1 ? html + text : html.slice(0, at) + text + html.slice(at);
}

/** The `srcdoc` of the preview frame */
export function buildPreviewDoc(html: string, state: BufferState): string {
  const styles = unsavedCss(state)
    // A stylesheet cannot contain its own closing tag, so this keeps the content inside <style>
    .map(
      ({ path, css }) =>
        `<style data-unsaved="${path}">${css.replace(/<\/style/gi, '<\\/style')}</style>`
    )
    .join('');
  let doc = styles ? insertBefore(html, '</head>', styles) : html;
  doc = insertBefore(doc, '</body>', KEEP_IN_FRAME);
  return doc;
}

/** The file a diagnostic points at (`article.html` → `templates/article.html`) */
export function diagnosticPath(d: Diagnostic): string | null {
  return d.template ? `templates/${d.template}` : null;
}

export function describeDiagnostic(d: Diagnostic): string {
  const where = d.template ? `${d.template}${d.line ? `:${d.line}` : ''}` : '';
  return where ? `${where} — ${d.message}` : d.message;
}
