import { describe, expect, it } from 'vitest';

import { countLines, splitLines } from '../../../src/utils/core/lines.js';

describe('countLines', () => {
  it('returns 0 for an empty file', () => {
    expect(countLines('')).toBe(0);
  });

  it('counts a single line with no trailing newline', () => {
    expect(countLines('AAA')).toBe(1);
  });

  it('does NOT count a phantom line for a single trailing newline', () => {
    // Regression: content.split('\n').length reported 2 here.
    expect(countLines('AAA\n')).toBe(1);
  });

  it('counts multiple lines without a trailing newline', () => {
    expect(countLines('AAA\nBBB\nCCC')).toBe(3);
  });

  it('counts multiple lines with a trailing newline as the same total', () => {
    // Regression: the trailing newline must not add a 4th line.
    expect(countLines('AAA\nBBB\nCCC\n')).toBe(3);
  });

  it('counts a lone newline as one empty line', () => {
    expect(countLines('\n')).toBe(1);
  });

  it('counts interior blank lines', () => {
    expect(countLines('AAA\n\nBBB')).toBe(3);
    expect(countLines('AAA\n\nBBB\n')).toBe(3);
  });

  it('handles a file that is only blank lines', () => {
    expect(countLines('\n\n')).toBe(2);
  });
});

describe('splitLines', () => {
  it('returns no lines for empty content', () => {
    expect(splitLines('')).toEqual([]);
  });

  it('splits content with no trailing newline', () => {
    expect(splitLines('a\nb\nc')).toEqual(['a', 'b', 'c']);
  });

  it('drops the phantom element from a single trailing newline', () => {
    // Regression: a plain split('\n') would yield ['a','b','c',''].
    expect(splitLines('a\nb\nc\n')).toEqual(['a', 'b', 'c']);
  });

  it('handles CRLF line endings and a trailing CRLF', () => {
    expect(splitLines('a\r\nb\r\n')).toEqual(['a', 'b']);
  });

  it('preserves a genuine trailing blank line (only one terminator dropped)', () => {
    expect(splitLines('a\n\n')).toEqual(['a', '']);
  });

  it('agrees with countLines on length', () => {
    for (const c of [
      '',
      'a',
      'a\n',
      'a\nb',
      'a\nb\n',
      '\n',
      '\n\n',
      'a\n\nb',
    ]) {
      expect(splitLines(c).length).toBe(countLines(c));
    }
  });
});
