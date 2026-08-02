import { describe, expect, it } from 'vitest';
import { compareIsoDateDescending } from '../../src/utils/core/compare.js';

describe('compareIsoDateDescending', () => {
  it('returns 0 when both are undefined', () => {
    expect(compareIsoDateDescending(undefined, undefined)).toBe(0);
  });

  it('sorts undefined left after defined right (undefined = +1)', () => {
    expect(compareIsoDateDescending(undefined, '2024-01-01')).toBeGreaterThan(
      0
    );
  });

  it('sorts defined left before undefined right (undefined = -1)', () => {
    expect(
      compareIsoDateDescending('2024-01-01', undefined)
    ).toBeLessThan(0);
  });

  it('returns 0 when both are invalid ISO strings', () => {
    expect(compareIsoDateDescending('not-a-date', 'also-not')).toBe(0);
  });

  it('sorts invalid left after valid right', () => {
    expect(
      compareIsoDateDescending('not-a-date', '2024-01-01')
    ).toBeGreaterThan(0);
  });

  it('sorts valid left before invalid right', () => {
    expect(
      compareIsoDateDescending('2024-01-01', 'not-a-date')
    ).toBeLessThan(0);
  });

  it('puts newer dates first (descending)', () => {
    const newer = '2024-06-01T00:00:00Z';
    const older = '2024-01-01T00:00:00Z';
    expect(compareIsoDateDescending(newer, older)).toBeLessThan(0);
    expect(compareIsoDateDescending(older, newer)).toBeGreaterThan(0);
  });

  it('returns 0 for equal ISO dates', () => {
    expect(
      compareIsoDateDescending('2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z')
    ).toBe(0);
  });

  it('handles empty string as valid date (NaN path)', () => {
    // empty string is passed to Date.parse → NaN
    expect(
      compareIsoDateDescending('', '2024-01-01')
    ).toBeGreaterThan(0);
    expect(
      compareIsoDateDescending('2024-01-01', '')
    ).toBeLessThan(0);
  });

  it('can sort an array of items descending by date', () => {
    const dates = [
      '2023-01-01T00:00:00Z',
      '2025-06-01T00:00:00Z',
      '2024-03-15T00:00:00Z',
    ];
    const sorted = [...dates].sort(compareIsoDateDescending);
    expect(sorted).toEqual([
      '2025-06-01T00:00:00Z',
      '2024-03-15T00:00:00Z',
      '2023-01-01T00:00:00Z',
    ]);
  });
});
