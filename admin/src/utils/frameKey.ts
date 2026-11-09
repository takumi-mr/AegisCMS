/**
 * A random name (32 lowercase hex digits) for where the server keeps a page for a preview frame
 * (`POST /api/v1/preview`, `POST /api/v1/notifications/preview`). The server serves the page at
 * `/api/v1/preview/frame/{name}`, so the name is the only thing that keeps it from other people:
 * it has to be unguessable.
 */
export const newFrameKey = (): string =>
  Array.from(crypto.getRandomValues(new Uint8Array(16)), (b) =>
    b.toString(16).padStart(2, '0')
  ).join('');
