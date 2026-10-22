export const formatSize = (bytes: number): string =>
  bytes >= 1024 * 1024
    ? `${(bytes / (1024 * 1024)).toFixed(1)} MB`
    : `${Math.ceil(bytes / 1024)} KB`;

/** File name part of a storage key (`2026/10/abcd1234-photo.png` -> `abcd1234-photo.png`) */
export const fileNameFromKey = (key: string): string => key.slice(key.lastIndexOf('/') + 1);

/**
 * Alt text derived from a storage key: drops the folders, the 8-character id prefix the
 * upload API adds, and the extension (`2026/10/abcd1234-red_car.png` -> `red car`).
 */
export const altFromKey = (key: string): string =>
  fileNameFromKey(key)
    .replace(/^[0-9a-f]{8}-/, '')
    .replace(/\.[^.]+$/, '')
    .replace(/_/g, ' ');
