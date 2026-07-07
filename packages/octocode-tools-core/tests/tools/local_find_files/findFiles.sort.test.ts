import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { basename, join } from 'node:path';

import { afterEach, describe, expect, it } from 'vitest';

import { findFiles } from '../../../src/tools/local_find_files/findFiles.js';

const tempDirs: string[] = [];

async function createTempDir(): Promise<string> {
  const dir = await mkdtemp(join(process.cwd(), '.tmp-find-files-sort-'));
  tempDirs.push(dir);
  return dir;
}

afterEach(async () => {
  await Promise.all(
    tempDirs.splice(0).map(dir => rm(dir, { recursive: true, force: true }))
  );
});

describe('findFiles sorting', () => {
  it('sorts by size before applying the result limit', async () => {
    const dir = await createTempDir();
    await writeFile(join(dir, 'small.ts'), 'x');
    await writeFile(join(dir, 'large.ts'), 'x'.repeat(100));
    await writeFile(join(dir, 'medium.ts'), 'x'.repeat(50));

    const result = await findFiles({
      path: dir,
      entryType: 'f',
      names: ['*.ts'],
      sortBy: 'size',
      limit: 2,
      itemsPerPage: 2,
      details: true,
    });

    expect(result.files.map(file => basename(file.path))).toEqual([
      'large.ts',
      'medium.ts',
    ]);
    expect(result.pagination.totalFiles).toBe(2);
    expect(result.pagination.totalFilesFound).toBe(3);
  });
});

describe('findFiles malformed time filters', () => {
  it('strips an invalid time filter so it matches the "skipped" warning', async () => {
    const dir = await createTempDir();
    await writeFile(join(dir, 'a.ts'), 'x');
    await writeFile(join(dir, 'b.ts'), 'y');

    const baseline = await findFiles({
      path: dir,
      entryType: 'f',
      names: ['*.ts'],
      details: true,
    });
    const withBadFilter = await findFiles({
      path: dir,
      entryType: 'f',
      names: ['*.ts'],
      details: true,
      // Not a relative duration ("7d"/"2h"/…): the filter must be dropped, not
      // forwarded to the native walk where it could silently suppress results.
      modifiedWithin: 'banana',
    } as Parameters<typeof findFiles>[0]);

    expect(withBadFilter.warnings?.some(w => w.includes('modifiedWithin'))).toBe(
      true
    );
    expect(withBadFilter.files.map(f => basename(f.path)).sort()).toEqual(
      baseline.files.map(f => basename(f.path)).sort()
    );
  });
});
