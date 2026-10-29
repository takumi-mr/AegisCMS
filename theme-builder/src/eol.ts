/**
 * The line break a file uses. CodeMirror treats every kind of line break as one and joins lines with
 * "\n" unless told otherwise, so without this a CRLF file would count as edited the moment it opens,
 * and saving would change its line endings.
 */
export function detectEol(text: string): '\n' | '\r\n' {
  const crlf = (text.match(/\r\n/g) ?? []).length;
  const lf = (text.match(/\n/g) ?? []).length - crlf;
  return crlf > lf ? '\r\n' : '\n';
}
