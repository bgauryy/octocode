import { describe, it, expect } from 'vitest';
import {
  countSerializedChars,
  attachRawResponseChars,
  getRawResponseChars,
} from '../../src/utils/response/charSavings.js';

describe('countSerializedChars', () => {
  it('returns length for string values', () => {
    expect(countSerializedChars('hello')).toBe(5);
    expect(countSerializedChars('')).toBe(0);
  });

  it('serializes objects to JSON and returns length', () => {
    const obj = { a: 1 };
    expect(countSerializedChars(obj)).toBe(JSON.stringify(obj).length);
  });

  it('handles null (serializes as "null")', () => {
    expect(countSerializedChars(null)).toBe(4);
  });

  it('handles numbers', () => {
    expect(countSerializedChars(42)).toBe(2);
  });

  it('returns 0 when JSON.stringify returns undefined (e.g. undefined value)', () => {
    expect(countSerializedChars(undefined)).toBe(0);
  });

  it('falls back to String() when JSON.stringify throws (circular ref)', () => {
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    const result = countSerializedChars(circular);
    expect(typeof result).toBe('number');
    expect(result).toBeGreaterThan(0);
  });
});

describe('attachRawResponseChars', () => {
  it('attaches char count when rawResponse is a number', () => {
    const result = attachRawResponseChars({ status: 'ok' }, 100);
    expect(getRawResponseChars(result)).toBe(100);
  });

  it('attaches char count when rawResponse is a string', () => {
    const result = attachRawResponseChars({ status: 'ok' }, 'hello');
    expect(getRawResponseChars(result)).toBe(5);
  });

  it('attaches char count when rawResponse is an object', () => {
    const result = attachRawResponseChars({ status: 'ok' }, { x: 1 });
    expect(getRawResponseChars(result)).toBeGreaterThan(0);
  });

  it('returns result unchanged when rawResponse is NaN (rawChars === undefined)', () => {
    const input = { status: 'ok' };
    const result = attachRawResponseChars(input, NaN);
    expect(getRawResponseChars(result)).toBeUndefined();
    expect(result).toBe(input);
  });

  it('returns result unchanged when rawResponse is Infinity', () => {
    const input = { status: 'ok' };
    const result = attachRawResponseChars(input, Infinity);
    expect(getRawResponseChars(result)).toBeUndefined();
    expect(result).toBe(input);
  });

  it('normalizes negative raw count to 0', () => {
    const result = attachRawResponseChars({ status: 'ok' }, -5);
    expect(getRawResponseChars(result)).toBe(0);
  });
});

describe('getRawResponseChars', () => {
  it('returns undefined for non-objects', () => {
    expect(getRawResponseChars(null)).toBeUndefined();
    expect(getRawResponseChars('string')).toBeUndefined();
    expect(getRawResponseChars(42)).toBeUndefined();
  });

  it('returns undefined for objects without the symbol', () => {
    expect(getRawResponseChars({ status: 'ok' })).toBeUndefined();
  });
});
