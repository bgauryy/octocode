import { describe, expect, it } from 'vitest';
import {
  applyMaskToSpans,
  deduplicateSpans,
  maskEveryOtherChar,
  type Span,
} from '../../src/security/maskUtils.js';

describe('maskEveryOtherChar', () => {
  it('replaces even-indexed characters with *', () => {
    expect(maskEveryOtherChar('abcdef')).toBe('*b*d*f');
  });

  it('returns empty string unchanged', () => {
    expect(maskEveryOtherChar('')).toBe('');
  });

  it('single character becomes *', () => {
    expect(maskEveryOtherChar('x')).toBe('*');
  });

  it('two characters: first masked, second preserved', () => {
    expect(maskEveryOtherChar('ab')).toBe('*b');
  });
});

describe('deduplicateSpans', () => {
  it('returns sorted non-overlapping spans unchanged', () => {
    const spans: Span[] = [
      { start: 0, end: 3 },
      { start: 5, end: 8 },
    ];
    expect(deduplicateSpans(spans)).toEqual([
      { start: 0, end: 3 },
      { start: 5, end: 8 },
    ]);
  });

  it('sorts spans by start position', () => {
    const spans: Span[] = [
      { start: 5, end: 8 },
      { start: 0, end: 3 },
    ];
    const result = deduplicateSpans(spans);
    expect(result[0].start).toBe(0);
    expect(result[1].start).toBe(5);
  });

  it('drops a span whose start is inside a previous span', () => {
    const spans: Span[] = [
      { start: 0, end: 10 },
      { start: 5, end: 15 }, // overlaps with first
    ];
    const result = deduplicateSpans(spans);
    expect(result).toHaveLength(1);
    expect(result[0]).toEqual({ start: 0, end: 10 });
  });

  it('keeps a span that starts exactly where the previous ended', () => {
    const spans: Span[] = [
      { start: 0, end: 5 },
      { start: 5, end: 10 },
    ];
    expect(deduplicateSpans(spans)).toHaveLength(2);
  });

  it('handles empty input', () => {
    expect(deduplicateSpans([])).toEqual([]);
  });
});

describe('applyMaskToSpans', () => {
  it('masks specified spans and leaves the rest intact', () => {
    const text = 'hello world';
    const spans: Span[] = [{ start: 6, end: 11 }]; // 'world'
    const result = applyMaskToSpans(text, spans);
    expect(result.startsWith('hello ')).toBe(true);
    expect(result).toContain('*'); // some chars masked
    expect(result).not.toContain('world'); // raw value gone
  });

  it('masks the full string when span covers everything', () => {
    const text = 'abcd';
    const result = applyMaskToSpans(text, [{ start: 0, end: 4 }]);
    expect(result).toBe('*b*d');
  });

  it('leaves text untouched when no spans', () => {
    const text = 'no secrets here';
    expect(applyMaskToSpans(text, [])).toBe(text);
  });

  it('handles multiple non-overlapping spans', () => {
    const text = 'aXbYc';
    const result = applyMaskToSpans(text, [
      { start: 1, end: 2 }, // 'X'
      { start: 3, end: 4 }, // 'Y'
    ]);
    expect(result.at(0)).toBe('a');
    expect(result.at(2)).toBe('b');
    expect(result.at(4)).toBe('c');
    expect(result).toHaveLength(text.length);
  });
});
