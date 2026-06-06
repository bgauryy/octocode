import { describe, it, expect } from 'vitest';
import { toNumber, toBoolean, toArray, safePath, numericString, booleanString, stringArray } from '../../validation/httpPreprocess.js';

describe('toNumber', () => {
  it('passes through numbers', () => {
    expect(toNumber(42)).toBe(42);
    expect(toNumber(0)).toBe(0);
  });

  it('converts numeric strings to integers', () => {
    expect(toNumber('42')).toBe(42);
    expect(toNumber('0')).toBe(0);
    expect(toNumber('100')).toBe(100);
  });

  it('returns non-numeric strings as-is', () => {
    expect(toNumber('abc')).toBe('abc');
    expect(toNumber('12.5')).toBe('12.5');
    expect(toNumber('12abc')).toBe('12abc');
  });

  it('returns non-string/non-number values as-is', () => {
    expect(toNumber(null)).toBeNull();
    expect(toNumber(undefined)).toBeUndefined();
    expect(toNumber(true)).toBe(true);
    expect(toNumber([])).toEqual([]);
  });
});

describe('toBoolean', () => {
  it('passes through booleans', () => {
    expect(toBoolean(true)).toBe(true);
    expect(toBoolean(false)).toBe(false);
  });

  it('converts string "true" and "false"', () => {
    expect(toBoolean('true')).toBe(true);
    expect(toBoolean('false')).toBe(false);
  });

  it('returns other values as-is', () => {
    expect(toBoolean('yes')).toBe('yes');
    expect(toBoolean('1')).toBe('1');
    expect(toBoolean(null)).toBeNull();
    expect(toBoolean(undefined)).toBeUndefined();
  });
});

describe('toArray', () => {
  it('passes through arrays', () => {
    expect(toArray(['a', 'b'])).toEqual(['a', 'b']);
    expect(toArray([])).toEqual([]);
  });

  it('splits comma-separated strings', () => {
    expect(toArray('a,b,c')).toEqual(['a', 'b', 'c']);
    expect(toArray('ts,js')).toEqual(['ts', 'js']);
  });

  it('trims whitespace around items', () => {
    expect(toArray('a , b , c')).toEqual(['a', 'b', 'c']);
  });

  it('returns empty array for empty string', () => {
    expect(toArray('')).toEqual([]);
  });

  it('returns empty array for whitespace-only string', () => {
    expect(toArray('  ')).toEqual([]);
  });

  it('filters out empty items from trailing commas', () => {
    expect(toArray('a,b,')).toEqual(['a', 'b']);
    expect(toArray(',a')).toEqual(['a']);
  });

  it('handles single value strings', () => {
    expect(toArray('typescript')).toEqual(['typescript']);
  });

  it('returns non-string/non-array values as-is', () => {
    expect(toArray(null)).toBeNull();
    expect(toArray(undefined)).toBeUndefined();
    expect(toArray(42)).toBe(42);
  });
});

describe('safePath', () => {
  it('accepts valid paths', () => {
    expect(safePath.safeParse('/Users/dev/project').success).toBe(true);
    expect(safePath.safeParse('/tmp/test').success).toBe(true);
    expect(safePath.safeParse('src/utils.ts').success).toBe(true);
  });

  it('rejects null bytes', () => {
    expect(safePath.safeParse('/path/\0/exploit').success).toBe(false);
  });

  it('rejects directory traversal', () => {
    expect(safePath.safeParse('../../../etc/passwd').success).toBe(false);
    expect(safePath.safeParse('src/../../etc/passwd').success).toBe(false);
  });

  it('rejects URL-encoded traversal', () => {
    expect(safePath.safeParse('%2e%2e%2fetc/passwd').success).toBe(false);
    expect(safePath.safeParse('/path/%2e%2e/exploit').success).toBe(false);
  });

  it('rejects backslashes on non-Windows', () => {
    if (process.platform !== 'win32') {
      expect(safePath.safeParse('path\\to\\file').success).toBe(false);
    }
  });
});

describe('numericString (Zod schema)', () => {
  it('parses string numbers', () => {
    expect(numericString.parse('10')).toBe(10);
  });

  it('handles undefined', () => {
    expect(numericString.parse(undefined)).toBeUndefined();
  });
});

describe('booleanString (Zod schema)', () => {
  it('parses string booleans', () => {
    expect(booleanString.parse('true')).toBe(true);
    expect(booleanString.parse('false')).toBe(false);
  });

  it('handles undefined', () => {
    expect(booleanString.parse(undefined)).toBeUndefined();
  });
});

describe('stringArray (Zod schema)', () => {
  it('parses comma-separated strings', () => {
    expect(stringArray.parse('a,b,c')).toEqual(['a', 'b', 'c']);
  });

  it('passes through arrays', () => {
    expect(stringArray.parse(['x', 'y'])).toEqual(['x', 'y']);
  });

  it('handles empty string', () => {
    expect(stringArray.parse('')).toEqual([]);
  });
});
