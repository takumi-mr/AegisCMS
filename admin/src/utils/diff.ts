export type DiffKind = 'same' | 'add' | 'remove';

export interface DiffLine {
  kind: DiffKind;
  text: string;
}

/** Above this many cells the line-by-line comparison is skipped (the middle is shown as replaced) */
const MAX_CELLS = 4_000_000;

const toLines = (text: string): string[] => (text === '' ? [] : text.split('\n'));

/**
 * Line diff from `before` to `after`: lines only in `before` are `remove`, lines only in `after`
 * are `add`. Identical text at the start and end is trimmed first, so a small edit in a long
 * article stays cheap.
 */
export function diffLines(before: string, after: string): DiffLine[] {
  const a = toLines(before);
  const b = toLines(after);
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) {
    endA--;
    endB--;
  }
  const head = a.slice(0, start).map((text): DiffLine => ({ kind: 'same', text }));
  const tail = a.slice(endA).map((text): DiffLine => ({ kind: 'same', text }));
  const midA = a.slice(start, endA);
  const midB = b.slice(start, endB);
  return [...head, ...diffMiddle(midA, midB), ...tail];
}

function diffMiddle(a: string[], b: string[]): DiffLine[] {
  if (a.length === 0 || b.length === 0 || a.length * b.length > MAX_CELLS) {
    return [
      ...a.map((text): DiffLine => ({ kind: 'remove', text })),
      ...b.map((text): DiffLine => ({ kind: 'add', text })),
    ];
  }
  // Longest common subsequence table, filled from the end so the walk below goes forward
  const width = b.length + 1;
  const table = new Uint32Array((a.length + 1) * width);
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      table[i * width + j] =
        a[i] === b[j]
          ? table[(i + 1) * width + j + 1] + 1
          : Math.max(table[(i + 1) * width + j], table[i * width + j + 1]);
    }
  }
  const out: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    if (a[i] === b[j]) {
      out.push({ kind: 'same', text: a[i] });
      i++;
      j++;
    } else if (table[(i + 1) * width + j] >= table[i * width + j + 1]) {
      out.push({ kind: 'remove', text: a[i++] });
    } else {
      out.push({ kind: 'add', text: b[j++] });
    }
  }
  while (i < a.length) out.push({ kind: 'remove', text: a[i++] });
  while (j < b.length) out.push({ kind: 'add', text: b[j++] });
  return out;
}

/** True when the diff has an added or removed line */
export const hasChanges = (lines: DiffLine[]): boolean => lines.some((l) => l.kind !== 'same');
