import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

import { afterEach, describe, expect, it } from 'vitest';

import { findFiles } from '../../../src/tools/local_find_files/findFiles.js';

const tempDirs: string[] = [];

async function createTempDir(): Promise<string> {
  const dir = await mkdtemp(join(process.cwd(), '.tmp-find-files-oor-'));
  tempDirs.push(dir);
  return dir;
}

afterEach(async () => {
  await Promise.all(
    tempDirs.splice(0).map(dir => rm(dir, { recursive: true, force: true }))
  );
});

describe('findFiles out-of-range page', () => {
  it('does not silently return an empty page with no signal when page exceeds totalPages', async () => {
    const dir = await createTempDir();
    for (let i = 0; i < 4; i++) {
      await writeFile(join(dir, `file${i}.ts`), 'x');
    }

    const result = await findFiles({
      path: dir,
      entryType: 'f',
      names: ['*.ts'],
      itemsPerPage: 2,
      page: 50,
      detail: 'full',
    });

    // totalPages should be 2 (4 files / 2 per page); page:50 is out of range.
    expect(result.pagination.totalPages).toBe(2);
    expect(result.files).toHaveLength(0);
    // The bug: totalFiles>0 but files:[] with no honest signal — must not
    // be indistinguishable from a valid (if coincidentally empty) response.
    const outOfRange = (
      result.pagination as { outOfRange?: boolean }
    ).outOfRange;
    const warned = (result.warnings ?? []).some(w =>
      w.toLowerCase().includes('page')
    );
    expect(outOfRange === true || warned).toBe(true);
  });

  it('a valid page still returns its files with no out-of-range signal', async () => {
    const dir = await createTempDir();
    for (let i = 0; i < 4; i++) {
      await writeFile(join(dir, `file${i}.ts`), 'x');
    }

    const result = await findFiles({
      path: dir,
      entryType: 'f',
      names: ['*.ts'],
      itemsPerPage: 2,
      page: 2,
      detail: 'full',
    });

    expect(result.files).toHaveLength(2);
    expect(
      (result.pagination as { outOfRange?: boolean }).outOfRange
    ).toBeFalsy();
  });
});
