import { describe, expect, it } from 'vitest';
import {
  getBulkDefaultCharLength,
  MAX_DEFAULT_OUTPUT_CHAR_LENGTH,
} from '../../src/utils/pagination/charLimit.js';

describe('getBulkDefaultCharLength', () => {
  it('returns a positive number for query count 1', () => {
    const result = getBulkDefaultCharLength(1);
    expect(result).toBeGreaterThan(0);
  });

  it('scales proportionally with query count up to the max', () => {
    const single = getBulkDefaultCharLength(1);
    const triple = getBulkDefaultCharLength(3);
    // triple should be at least as large as single (may be capped at max)
    expect(triple).toBeGreaterThanOrEqual(single);
  });

  it('never exceeds MAX_DEFAULT_OUTPUT_CHAR_LENGTH', () => {
    expect(getBulkDefaultCharLength(1000)).toBeLessThanOrEqual(
      MAX_DEFAULT_OUTPUT_CHAR_LENGTH
    );
    expect(getBulkDefaultCharLength(1)).toBeLessThanOrEqual(
      MAX_DEFAULT_OUTPUT_CHAR_LENGTH
    );
  });

  it('treats 0 as 1 query (floor clamp)', () => {
    // Math.max(Math.floor(0) || 0, 1) = 1
    const result = getBulkDefaultCharLength(0);
    expect(result).toBeGreaterThan(0);
    expect(result).toBe(getBulkDefaultCharLength(1));
  });

  it('treats negative counts as 1 query', () => {
    expect(getBulkDefaultCharLength(-5)).toBe(getBulkDefaultCharLength(1));
  });

  it('treats non-integer counts by flooring', () => {
    // Math.floor(2.9) = 2
    expect(getBulkDefaultCharLength(2.9)).toBe(getBulkDefaultCharLength(2));
  });

  it('MAX_DEFAULT_OUTPUT_CHAR_LENGTH is 100_000', () => {
    expect(MAX_DEFAULT_OUTPUT_CHAR_LENGTH).toBe(100_000);
  });
});
