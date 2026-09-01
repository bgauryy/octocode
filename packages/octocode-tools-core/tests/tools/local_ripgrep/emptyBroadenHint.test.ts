import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

import { afterEach, describe, expect, it } from 'vitest';

import { searchContentRipgrep } from '../../../src/tools/local_ripgrep/searchContentRipgrep.js';

const tempDirs: string[] = [];

afterEach(async () => {
  await Promise.all(
    tempDirs.splice(0).map(dir => rm(dir, { recursive: true, force: true }))
  );
});

/**
 * The tool doc promises "empty + broaden hint" — an honest empty must point
 * somewhere useful (looser case/regex, wider path), not leave the caller at a
 * bare `status:"empty"` with stats only.
 */
describe('local.text honest-empty broaden hint', () => {
  it('a zero-match search carries an actionable broaden hint', async () => {
    const dir = await mkdtemp(join(process.cwd(), '.tmp-rg-empty-'));
    tempDirs.push(dir);
    await writeFile(join(dir, 'a.ts'), 'const hello = 1;\n');

    const result = await searchContentRipgrep({
      path: dir,
      searchText: 'zqxwvutplokm_nonexistent_needle',
      regex: 'fixed',
    } as Parameters<typeof searchContentRipgrep>[0]);

    expect(result.status).toBe('empty');
    const hints = (result as { hints?: string[] }).hints ?? [];
    expect(hints.length).toBeGreaterThan(0);
    expect(
      hints.some(
        h =>
          h.toLowerCase().includes('broaden') ||
          h.toLowerCase().includes('casemode') ||
          h.toLowerCase().includes('wider')
      )
    ).toBe(true);
  });
});
