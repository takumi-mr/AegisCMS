/**
 * Where a new file may go: the same places the dev server accepts (`templates/`, `samples/`,
 * `assets/`, text extensions). No segment starts with a dot, so `..` and hidden files are out.
 */
const NEW_FILE_PATTERN =
  /^(templates|samples|assets)\/[\w-][\w.-]*(\/[\w-][\w.-]*)*\.(html|css|js|mjs|json|svg|txt|md)$/;

export function isValidNewFilePath(path: string): boolean {
  return NEW_FILE_PATTERN.test(path);
}
