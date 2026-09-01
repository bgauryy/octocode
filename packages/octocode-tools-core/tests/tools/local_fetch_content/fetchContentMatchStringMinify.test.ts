import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { mkdtemp, rm, writeFile } from 'fs/promises';
import { join } from 'path';

import { fetchContent } from '../../../src/tools/local_fetch_content/fetchContent.js';

const ROOT = process.cwd();

/**
 * matchString extractions must NEVER be minified: standard minification
 * strips comments/blank lines AFTER extraction, so a match inside a comment
 * could be deleted from the very content whose matchRanges point at it —
 * returned evidence contradicting its own anchors. Policy (by design):
 * matchString blocks minify at the code level; an explicit minify request is
 * answered with a warning, not applied.
 */
describe('fetchContent matchString blocks minification', () => {
  let dir: string;
  let file: string;

  beforeAll(async () => {
    dir = await mkdtemp(join(ROOT, 'octocode-fetch-matchstring-minify-'));
    file = join(dir, 'code.ts');
    await writeFile(
      file,
      [
        'const before = 1;',
        '',
        '// IMPORTANT: needleXyz lives in this comment',
        'const after = 2;',
        '',
        'export function unrelated() {',
        '  return before + after;',
        '}',
        '',
      ].join('\n'),
      'utf-8'
    );
  });

  afterAll(async () => {
    await rm(dir, { recursive: true, force: true });
  });

  it('returns the matched comment line verbatim even with an explicit minify:"standard", plus a warning', async () => {
    const result = await fetchContent({
      path: file,
      matchString: 'needleXyz',
      contextLines: 2,
      minify: 'standard',
    } as never);

    expect(result.status).not.toBe('error');
    const content = (result as { content?: string }).content ?? '';
    expect(content).toContain('needleXyz lives in this comment');
    expect(
      ((result as { warnings?: string[] }).warnings ?? []).some(w =>
        w.toLowerCase().includes('minify')
      )
    ).toBe(true);
  });

  it('returns the matched comment line verbatim with no minify specified (default must not strip it)', async () => {
    const result = await fetchContent({
      path: file,
      matchString: 'needleXyz',
      contextLines: 2,
    } as never);

    expect(result.status).not.toBe('error');
    const content = (result as { content?: string }).content ?? '';
    expect(content).toContain('needleXyz lives in this comment');
  });

  it('non-matchString reads still minify normally (no regression)', async () => {
    const result = await fetchContent({
      path: file,
      minify: 'standard',
    } as never);

    expect(result.status).not.toBe('error');
    const content = (result as { content?: string }).content ?? '';
    // Standard minification strips comment lines for a whole-file read.
    expect(content).not.toContain('needleXyz lives in this comment');
    expect(content).toContain('export function unrelated()');
  });
});
