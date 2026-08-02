import { describe, expect, it } from 'vitest';

import { resolveDateWindow } from '../../src/github/dateWindow.js';

// Regression guard for the ghSearchCommits since/until false-absence bug:
// the GitHub commits API only accepts absolute ISO-8601, so a raw relative
// window like "30d" was silently dropped and returned an empty list. These
// lock the relative→ISO conversion and the honest warning on bad input.
describe('resolveDateWindow (ghSearchCommits since/until)', () => {
  const approxDaysAgo = (iso: string): number =>
    (Date.now() - Date.parse(iso)) / (1000 * 60 * 60 * 24);

  it('converts relative day windows to an ISO timestamp', () => {
    const r = resolveDateWindow('30d');
    expect(r.warning).toBeUndefined();
    expect(r.value).toMatch(/^\d{4}-\d{2}-\d{2}T/);
    expect(approxDaysAgo(r.value!)).toBeGreaterThan(29);
    expect(approxDaysAgo(r.value!)).toBeLessThan(31);
  });

  it('supports h / d / w / m / y units', () => {
    expect(approxDaysAgo(resolveDateWindow('2w').value!)).toBeCloseTo(14, 0);
    expect(approxDaysAgo(resolveDateWindow('48h').value!)).toBeCloseTo(2, 0);
    // months/years land in a sane range (calendar math, not fixed 30/365)
    expect(approxDaysAgo(resolveDateWindow('6m').value!)).toBeGreaterThan(150);
    expect(approxDaysAgo(resolveDateWindow('1y').value!)).toBeGreaterThan(360);
  });

  it('passes an absolute ISO date through unchanged', () => {
    const r = resolveDateWindow('2026-01-01');
    expect(r.warning).toBeUndefined();
    expect(r.value).toBe('2026-01-01');
  });

  it('warns (never silently drops) on an unparseable window', () => {
    const r = resolveDateWindow('banana');
    expect(r.value).toBeUndefined();
    expect(r.warning).toMatch(/not a valid date or relative window/);
  });
});
