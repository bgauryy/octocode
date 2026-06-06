import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const readSrc = (p: string): string =>
  readFileSync(resolve(here, '../../src', p), 'utf8');

describe('minifier consolidation (#4)', () => {
  it('applyMinification is exported from the shared minifier util', async () => {
    const mod = await import('../../src/utils/minifier/applyMinification.js');
    expect(typeof mod.applyMinification).toBe('function');
    expect(mod.applyMinification('x', 'f.txt')).toBe('x');
  });

  it('github_fetch_content does NOT reach into local_fetch_content', () => {
    const finalizer = readSrc('tools/github_fetch_content/finalizer.ts');
    expect(finalizer).not.toMatch(/from\s+'\.\.\/local_fetch_content\//);
  });

  it('applyMinification shared util is available and functional', async () => {
    const mod = await import('../../src/utils/minifier/applyMinification.js');
    expect(typeof mod.applyMinification).toBe('function');
  });
});
