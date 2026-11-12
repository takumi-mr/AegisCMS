import { describe, expect, it } from 'vitest';
import { diffLines, hasChanges } from './diff';

const render = (before: string, after: string) =>
  diffLines(before, after).map((l) => `${{ same: ' ', add: '+', remove: '-' }[l.kind]}${l.text}`);

describe('diffLines', () => {
  it('marks identical text as unchanged', () => {
    expect(hasChanges(diffLines('a\nb', 'a\nb'))).toBe(false);
    expect(diffLines('', '')).toEqual([]);
  });

  it('shows a changed line as a removal and an addition', () => {
    expect(render('a\nb\nc', 'a\nB\nc')).toEqual([' a', '-b', '+B', ' c']);
  });

  it('shows inserted and deleted lines', () => {
    expect(render('a\nc', 'a\nb\nc')).toEqual([' a', '+b', ' c']);
    expect(render('a\nb\nc', 'a\nc')).toEqual([' a', '-b', ' c']);
  });

  it('handles an empty side', () => {
    expect(render('', 'x\ny')).toEqual(['+x', '+y']);
    expect(render('x\ny', '')).toEqual(['-x', '-y']);
  });

  it('keeps the common lines between distant edits', () => {
    expect(render('1\n2\n3\n4\n5', '1\nx\n3\ny\n5')).toEqual([
      ' 1',
      '-2',
      '+x',
      ' 3',
      '-4',
      '+y',
      ' 5',
    ]);
  });

  it('falls back to a full replacement for huge middles instead of freezing', () => {
    const before = Array.from({ length: 2500 }, (_, i) => `a${i}`).join('\n');
    const after = Array.from({ length: 2500 }, (_, i) => `b${i}`).join('\n');
    const lines = diffLines(before, after);
    expect(lines.filter((l) => l.kind === 'remove')).toHaveLength(2500);
    expect(lines.filter((l) => l.kind === 'add')).toHaveLength(2500);
  });
});
