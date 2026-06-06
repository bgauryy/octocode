import { describe, expect, it } from 'vitest';
import { resolveRipgrepBinary } from '../../src/utils/exec/ripgrepBinary.js';

describe('T3.3 — resolveRipgrepBinary', () => {
  it('returns a non-empty string (never undefined)', () => {
    const path = resolveRipgrepBinary();
    expect(typeof path).toBe('string');
    expect(path.length).toBeGreaterThan(0);
  });

  it('returns the bundled binary path, never a PATH fallback', () => {
    const path = resolveRipgrepBinary();
    expect(path).not.toBe('rg');
    expect(path.startsWith('/') || /^[A-Z]:\\/.test(path)).toBe(true);
  });
});
