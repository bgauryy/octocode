import { describe, it, expect } from 'vitest';
import { compareIsoDateDescending } from '../../../octocode-tools-core/src/utils/core/compare.js';

describe('compareIsoDateDescending', () => {
  it('returns 0 for two valid equal dates', () => {
    expect(compareIsoDateDescending('2024-01-01', '2024-01-01')).toBe(0);
  });

  it('returns negative when left is newer (right comes second)', () => {
    const result = compareIsoDateDescending('2024-06-01', '2024-01-01');
    expect(result).toBeLessThan(0);
  });

  it('returns positive when right is newer', () => {
    const result = compareIsoDateDescending('2024-01-01', '2024-06-01');
    expect(result).toBeGreaterThan(0);
  });

  it('returns 0 when both are undefined', () => {
    expect(compareIsoDateDescending(undefined, undefined)).toBe(0);
  });

  it('returns 1 when left is undefined (missing = sorted last)', () => {
    expect(compareIsoDateDescending(undefined, '2024-01-01')).toBe(1);
  });

  it('returns -1 when right is undefined', () => {
    expect(compareIsoDateDescending('2024-01-01', undefined)).toBe(-1);
  });

  it('returns 0 when both are unparseable (NaN + NaN)', () => {
    expect(compareIsoDateDescending('not-a-date', 'also-not-a-date')).toBe(0);
  });

  it('returns 1 when left is unparseable but right is valid', () => {
    expect(compareIsoDateDescending('bad-date', '2024-01-01')).toBe(1);
  });

  it('returns -1 when right is unparseable but left is valid', () => {
    expect(compareIsoDateDescending('2024-01-01', 'bad-date')).toBe(-1);
  });
});
