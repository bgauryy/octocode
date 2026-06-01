/**
 * #4 — minifier consolidation guard.
 *
 * After the basic=verbatim fix (#1), minification is owned solely by the
 * concise verbosity finalizers, both using the sync `applyMinification`
 * wrapper. That wrapper must live in the shared `utils/minifier/` module — NOT
 * under one tool's directory where another tool reaches across to import it.
 */
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
    // Behaviour preserved: returns minified only when smaller, else original.
    expect(mod.applyMinification('x', 'f.txt')).toBe('x');
  });

  it('github_fetch_content does NOT reach into local_fetch_content', () => {
    const finalizer = readSrc('tools/github_fetch_content/finalizer.ts');
    expect(finalizer).not.toMatch(/from\s+'\.\.\/local_fetch_content\//);
  });

  it('both fetch finalizers import applyMinification from the shared util', () => {
    const gh = readSrc('tools/github_fetch_content/finalizer.ts');
    const local = readSrc('tools/local_fetch_content/fetchContent.ts');
    expect(gh).toMatch(/utils\/minifier\/applyMinification\.js/);
    expect(local).toMatch(/utils\/minifier\/applyMinification\.js/);
  });
});
