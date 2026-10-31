import { dirtyBuffers, type BufferState } from './buffers.ts';
import type { Diagnostic, RenderRequest } from './types.ts';

/** The unsaved templates and samples, to render the page as it would look after saving */
export function renderRequest(
  sample: string,
  state: BufferState,
  block?: string | null
): RenderRequest {
  const request: RenderRequest = { sample, templates: {}, samples: {} };
  if (block) request.block = block;
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

/** Script injected into the preview frame for link interception and click-to-jump to source line */
const PREVIEW_ENHANCEMENT =
  '<script>(function(){' +
  "var s=document.createElement('style');s.textContent='body.aegis-inspecting [data-aegis-source]{outline:1px dashed #3b82f6!important;outline-offset:1px!important;cursor:crosshair!important}body.aegis-inspecting [data-aegis-source]:hover{outline:2px solid #2563eb!important;background:rgba(37,99,235,0.06)!important}';document.head&&document.head.appendChild(s);" +
  "var inspecting=document.body&&document.body.classList.contains('aegis-inspecting');" +
  "window.addEventListener('message',function(e){if(e.data&&e.data.type==='aegis-toggle-inspect'){inspecting=!!e.data.enabled;if(document.body)document.body.classList.toggle('aegis-inspecting',inspecting)}});" +
  "document.addEventListener('click',function(e){" +
  "var a=e.target&&e.target.closest&&e.target.closest('a[href]');if(a&&!/^#/.test(a.getAttribute('href')))e.preventDefault();" +
  "var active=inspecting||(document.body&&document.body.classList.contains('aegis-inspecting'))||e.altKey||e.metaKey;" +
  'if(active){' +
  "var el=e.target&&e.target.closest&&e.target.closest('[data-aegis-source]');" +
  'if(el){e.preventDefault();e.stopPropagation();' +
  "var source=el.getAttribute('data-aegis-source');" +
  "window.parent.postMessage({type:'aegis-jump-to-source',source:source},'*')}}" +
  '},true)' +
  '})()</script>';

function insertBefore(html: string, marker: string, text: string): string {
  const at = html.toLowerCase().lastIndexOf(marker);
  return at === -1 ? html + text : html.slice(0, at) + text + html.slice(at);
}

/** The `srcdoc` of the preview frame */
export function buildPreviewDoc(html: string, state: BufferState, inspecting = false): string {
  const styles = unsavedCss(state)
    // A stylesheet cannot contain its own closing tag, so this keeps the content inside <style>
    .map(
      ({ path, css }) =>
        `<style data-unsaved="${path}">${css.replace(/<\/style/gi, '<\\/style')}</style>`
    )
    .join('');
  let doc = styles ? insertBefore(html, '</head>', styles) : html;
  doc = insertBefore(doc, '</body>', PREVIEW_ENHANCEMENT);
  if (inspecting && !doc.includes('class="aegis-inspecting"')) {
    doc = doc.replace('<body', '<body class="aegis-inspecting"');
  }
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
