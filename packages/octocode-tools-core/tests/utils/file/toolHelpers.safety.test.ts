import { describe, expect, it } from 'vitest';
import { checkLargeOutputSafety } from '../../../src/utils/file/toolHelpers.js';

describe('checkLargeOutputSafety', () => {
  it('never blocks when hasCharLength is true', () => {
    const result = checkLargeOutputSafety(9999, true);
    expect(result.shouldBlock).toBe(false);
    expect(result.errorCode).toBeUndefined();
  });

  it('does not block when itemCount is at or below threshold', () => {
    const at = checkLargeOutputSafety(100, false, { threshold: 100 });
    expect(at.shouldBlock).toBe(false);

    const below = checkLargeOutputSafety(50, false, { threshold: 100 });
    expect(below.shouldBlock).toBe(false);
  });

  it('blocks when itemCount exceeds threshold', () => {
    const result = checkLargeOutputSafety(101, false, { threshold: 100 });
    expect(result.shouldBlock).toBe(true);
    expect(result.errorCode).toBeDefined();
    expect(result.hints?.length).toBeGreaterThan(0);
    expect(result.hints?.some(h => h.includes('101'))).toBe(true);
  });

  it('uses default threshold of 100', () => {
    expect(checkLargeOutputSafety(100, false).shouldBlock).toBe(false);
    expect(checkLargeOutputSafety(101, false).shouldBlock).toBe(true);
  });

  it('includes detailed hint when detailed option is true', () => {
    const result = checkLargeOutputSafety(200, false, { detailed: true });
    expect(result.hints?.some(h => h.includes('Detailed'))).toBe(true);
  });

  it('uses plural suffix for count > 1', () => {
    const result = checkLargeOutputSafety(101, false, {
      threshold: 100,
      itemType: 'file',
    });
    expect(result.hints?.some(h => h.includes('files'))).toBe(true);
  });

  it('uses singular suffix when count is exactly 1', () => {
    const result = checkLargeOutputSafety(1, false, {
      threshold: 0,
      itemType: 'file',
    });
    expect(result.shouldBlock).toBe(true);
    expect(result.hints?.some(h => /1 file[^s]/.test(h))).toBe(true);
  });
});
