import { describe, expect, it } from 'vitest';
import {
  countSerializedChars,
  attachRawResponseChars,
  getRawResponseChars,
} from '../../src/utils/response/charSavings.js';

// ---------------------------------------------------------------------------
// countSerializedChars
// ---------------------------------------------------------------------------

describe('countSerializedChars', () => {
  it('returns the length of a string directly', () => {
    expect(countSerializedChars('hello')).toBe(5);
  });

  it('returns 0 for an empty string', () => {
    expect(countSerializedChars('')).toBe(0);
  });

  it('serializes objects to JSON and returns their length', () => {
    const obj = { a: 1, b: 'x' };
    const expected = JSON.stringify(obj).length;
    expect(countSerializedChars(obj)).toBe(expected);
  });

  it('handles null (non-string, non-object)', () => {
    // JSON.stringify(null) = "null" → 4
    expect(countSerializedChars(null)).toBe(4);
  });

  it('handles arrays', () => {
    const arr = [1, 2, 3];
    expect(countSerializedChars(arr)).toBe(JSON.stringify(arr).length);
  });

  it('handles numbers', () => {
    // JSON.stringify(42) = "42" → 2
    expect(countSerializedChars(42)).toBe(2);
  });

  it('handles values that cannot be JSON.stringify-ed (circular ref)', () => {
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    // Should fall back to String() without throwing
    expect(() => countSerializedChars(circular)).not.toThrow();
    expect(typeof countSerializedChars(circular)).toBe('number');
  });
});

// ---------------------------------------------------------------------------
// attachRawResponseChars / getRawResponseChars
// ---------------------------------------------------------------------------

describe('attachRawResponseChars and getRawResponseChars', () => {
  it('attaches and retrieves raw char count from a number', () => {
    const result = attachRawResponseChars({ data: 'x' }, 500);
    expect(getRawResponseChars(result)).toBe(500);
  });

  it('attaches and retrieves raw char count from a raw object', () => {
    const rawObj = { content: 'hello world' }; // 11 chars → JSON: 27
    const result = attachRawResponseChars({ data: 'x' }, rawObj);
    expect(getRawResponseChars(result)).toBe(JSON.stringify(rawObj).length);
  });

  it('returns undefined when no raw chars have been attached', () => {
    expect(getRawResponseChars({ data: 'x' })).toBeUndefined();
  });

  it('returns undefined for null', () => {
    expect(getRawResponseChars(null)).toBeUndefined();
  });

  it('returns undefined for non-object types', () => {
    expect(getRawResponseChars('string')).toBeUndefined();
    expect(getRawResponseChars(42)).toBeUndefined();
  });

  it('clamps negative rawResponse numbers to 0', () => {
    const result = attachRawResponseChars({}, -100);
    expect(getRawResponseChars(result)).toBe(0);
  });

  it('rejects non-finite numbers', () => {
    const result = attachRawResponseChars({}, Infinity);
    expect(getRawResponseChars(result)).toBeUndefined();
  });

  it('the symbol property is non-enumerable (not visible in JSON.stringify)', () => {
    const result = attachRawResponseChars({ data: 'x' }, 999);
    const json = JSON.stringify(result);
    expect(json).not.toContain('rawResponseChars');
  });

  it('returns the result object unchanged (chaining)', () => {
    const target = { id: 'test' };
    const returned = attachRawResponseChars(target, 100);
    expect(returned).toBe(target);
  });
});
