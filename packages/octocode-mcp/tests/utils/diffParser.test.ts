import { describe, it, expect } from 'vitest';
import { filterPatch } from '../../src/utils/parsers/diff.js';

describe('filterPatch', () => {
  const samplePatch = `@@ -1,3 +1,4 @@
 line 1
-deleted line
+added line 1
+added line 2
 line 3`;

  it('returns full patch when no filter arrays provided', () => {
    const result = filterPatch(samplePatch);
    expect(result).toBe(samplePatch);
  });

  it('returns full patch when both filters are undefined', () => {
    const result = filterPatch(samplePatch, undefined, undefined);
    expect(result).toBe(samplePatch);
  });

  it('returns empty when both filters are empty arrays', () => {
    const result = filterPatch(samplePatch, [], []);
    expect(result).toBe('');
  });

  it('filters to specific addition lines', () => {
    const result = filterPatch(samplePatch, [2], undefined);
    expect(result).toContain('+2: added line 1');
    expect(result).not.toContain('added line 2');
  });

  it('filters to specific deletion lines', () => {
    const result = filterPatch(samplePatch, undefined, [2]);
    expect(result).toContain('-2: deleted line');
  });
});
