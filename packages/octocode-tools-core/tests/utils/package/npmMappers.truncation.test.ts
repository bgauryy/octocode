import { describe, expect, it } from 'vitest';

import { mapToResult } from '../../../src/utils/package/npm/npmMappers.js';

describe('npm metadata bounds', () => {
  it('reports totals when exports and bins exceed their previews', () => {
    const exports = Object.fromEntries(
      Array.from({ length: 14 }, (_, index) => [
        `./entry-${index}`,
        `./dist/entry-${index}.js`,
      ])
    );
    const bin = Object.fromEntries(
      Array.from({ length: 10 }, (_, index) => [
        `command-${index}`,
        `./bin/command-${index}.js`,
      ])
    );

    const result = mapToResult({
      name: 'bounded-package',
      exports,
      bin,
    } as never);

    expect(result.exports).toHaveLength(12);
    expect(result.exportsTotal).toBe(14);
    expect(result.exportsTruncated).toBe(true);
    expect(result.bin).toHaveLength(8);
    expect(result.binTotal).toBe(10);
    expect(result.binTruncated).toBe(true);
  });
});
