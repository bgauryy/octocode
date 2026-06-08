import { describe, it, expect } from 'vitest';
import { compactExports } from '../../src/tools/package_search/execution.js';

describe('compactExports — collapse exports to one line per subpath', () => {
  it('collapses subpath:condition:target into one line per subpath (prefers import)', () => {
    const out = compactExports([
      '.:types:./index.d.cts',
      '.:import:./index.js',
      '.:require:./index.cjs',
      './v4:types:./v4/index.d.cts',
      './v4:import:./v4/index.js',
      './v4:require:./v4/index.cjs',
    ])!;
    expect(out).toEqual(['. → ./index.js', './v4 → ./v4/index.js']);
  });

  it('keeps plain path exports as-is', () => {
    expect(compactExports(['./dist/index.mjs', './dist/index.d.ts'])).toEqual([
      './dist/index.mjs',
      './dist/index.d.ts',
    ]);
  });

  it('handles subpath:target (string export) form', () => {
    expect(compactExports(['.:./index.js', './sub:./sub/index.js'])).toEqual([
      '. → ./index.js',
      './sub → ./sub/index.js',
    ]);
  });

  it('caps at 8 subpaths and returns undefined for empty', () => {
    expect(compactExports([])).toBeUndefined();
    expect(compactExports(undefined)).toBeUndefined();
    const many = Array.from(
      { length: 20 },
      (_, i) => `./s${i}:import:./s${i}.js`
    );
    expect(compactExports(many)!.length).toBe(8);
  });
});
