import { mkdtemp, rm, mkdir, writeFile } from 'node:fs/promises';
import { basename, join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';

import { executeRipgrepSearchInternal } from '../../../src/tools/local_ripgrep/ripgrepExecutor.js';

const tempDirs: string[] = [];

async function createTempDir(): Promise<string> {
  const dir = await mkdtemp(join(process.cwd(), '.tmp-ripgrep-depth-'));
  tempDirs.push(dir);
  return dir;
}

afterEach(async () => {
  await Promise.all(
    tempDirs.splice(0).map(dir => rm(dir, { recursive: true, force: true }))
  );
});

describe('local.text text maxDepth', () => {
  it('filters text/regex search results to files within maxDepth', async () => {
    const dir = await createTempDir();
    await writeFile(join(dir, 'root.ts'), 'needle\n');
    await mkdir(join(dir, 'one', 'two'), { recursive: true });
    await writeFile(join(dir, 'one', 'nested.ts'), 'needle\n');
    await writeFile(join(dir, 'one', 'two', 'deep.ts'), 'needle\n');

    const result = await executeRipgrepSearchInternal({
      path: dir,
      searchText: 'needle',
      regex: 'fixed',
      maxDepth: 0,
      sort: 'path',
      itemsPerPage: 10,
    } as never);

    expect((result.files ?? []).map(file => basename(file.path))).toEqual([
      'root.ts',
    ]);
    expect(result.stats).toMatchObject({
      totalOccurrences: 1,
      matchedLines: 1,
      filesMatched: 1,
      filesSearched: 1,
    });
    expect(result.warnings?.join('\n') ?? '').not.toContain(
      'after native text search'
    );
  });
});
