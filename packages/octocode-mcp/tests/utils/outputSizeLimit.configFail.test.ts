/**
 * Tests the catch branch (line 80) in outputSizeLimit.ts when
 * getConfigSync() throws — e.g., config file is malformed.
 *
 * Covers: configuredDefaultCharLength = undefined (catch block)
 */

import { describe, it, expect, vi } from 'vitest';

vi.mock('octocode-shared', () => ({
  getConfigSync: vi.fn(() => {
    throw new Error('config unavailable');
  }),
}));

import { applyOutputSizeLimit } from '../../src/utils/pagination/outputSizeLimit.js';

describe('applyOutputSizeLimit — catch branch when getConfigSync throws', () => {
  it('falls back to DEFAULTS when getConfigSync throws', () => {
    const smallContent = 'small content';
    const result = applyOutputSizeLimit(smallContent, {});

    // Should not throw and should return content unchanged
    expect(result.wasLimited).toBe(false);
    expect(result.content).toBe(smallContent);
    expect(result.warnings).toHaveLength(0);
  });

  it('still auto-paginates large content after catch', () => {
    const largeContent = 'x'.repeat(50000);
    const result = applyOutputSizeLimit(largeContent, {});

    expect(result.wasLimited).toBe(true);
    expect(result.pagination).toBeDefined();
  });

  it('still applies explicit charLength after catch', () => {
    const content = 'z'.repeat(5000);
    const result = applyOutputSizeLimit(content, { charLength: 1000 });

    expect(result.wasLimited).toBe(true);
    expect(result.content.length).toBe(1000);
  });
});
