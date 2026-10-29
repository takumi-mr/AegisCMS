import { describe, expect, it } from 'vitest';
import { detectEol } from './eol.ts';

describe('detectEol', () => {
  it('follows the majority of the line breaks', () => {
    expect(detectEol('a\r\nb\r\nc')).toBe('\r\n');
    expect(detectEol('a\nb\nc')).toBe('\n');
    expect(detectEol('a\r\nb\nc\nd')).toBe('\n');
  });

  it('uses LF for a file without line breaks', () => {
    expect(detectEol('')).toBe('\n');
    expect(detectEol('one line')).toBe('\n');
  });
});
