import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

import { afterEach, describe, expect, it } from 'vitest';

import { analyzeGraph } from '../../../src/tools/local_analyze_graph/analyzeGraph.js';

const tempDirs: string[] = [];

afterEach(async () => {
  await Promise.all(
    tempDirs.splice(0).map(dir => rm(dir, { recursive: true, force: true }))
  );
});

describe('localAnalyzeGraph deadCode pagination', () => {
  it('applies limit before pagination', async () => {
    const dir = await mkdtemp(join(process.cwd(), '.tmp-dead-code-page-'));
    tempDirs.push(dir);
    await writeFile(
      join(dir, 'package.json'),
      JSON.stringify({ name: 'fixture', main: 'index.js' })
    );
    await writeFile(join(dir, 'index.js'), 'export const publicApi = 1;\n');
    await writeFile(
      join(dir, 'dead.js'),
      'export const first = 1;\nexport const second = 2;\nexport const third = 3;\n'
    );

    const result = await analyzeGraph({
      operation: 'deadCode',
      path: dir,
      includeTests: false,
      limit: 2,
      itemsPerPage: 1,
    });

    expect(result.pagination).toMatchObject({
      totalEntries: 2,
      totalPages: 2,
      entriesPerPage: 1,
    });
    expect(result.results).toHaveLength(1);
  });
});
