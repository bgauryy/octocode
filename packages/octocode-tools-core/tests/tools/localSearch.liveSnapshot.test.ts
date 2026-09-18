import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { LocalSearchQuerySchema } from '@octocodeai/octocode-core/schema';
import { executeDirectTool } from '../helpers/executeDirectTool.js';
import { contextUtils } from '../../src/utils/contextUtils.js';
import { regexErrorRecovery } from '../../src/tools/local_ripgrep/regexErrorRecovery.js';
import { LocalRipgrepQuerySchema } from '@octocodeai/octocode-core/schema';

type Query = Record<string, unknown>;
type Row = {
  status?: string;
  data: {
    files?: Array<{ path: string; matches?: Array<{ line: number }> }>;
    errorCode?: string;
    pagination?: { snapshot?: string };
    next?: Record<string, { tool: string; query: Query }>;
  };
};
async function search(query: Query): Promise<Row> {
  const result = await executeDirectTool('localSearch', {
    queries: [LocalSearchQuerySchema.parse(query)],
  });
  return (result.structuredContent as { results: Row[] }).results[0]!;
}

describe('live lexical continuation identity', () => {
  let root: string;
  let query: Query;
  beforeEach(async () => {
    await mkdir(join(process.cwd(), '.octocode', 'tmp'), { recursive: true });
    root = await mkdtemp(
      join(process.cwd(), '.octocode', 'tmp', 'live-snapshot-')
    );
    for (const name of ['a.txt', 'b.txt', 'c.txt'])
      await writeFile(join(root, name), 'needle one\nneedle two\n');
    query = {
      path: root,
      searchText: 'needle',
      regex: 'literal',
      sort: 'path',
      pageSize: 1,
      maxMatchesPerFile: 1,
    };
  });
  afterEach(async () => {
    vi.restoreAllMocks();
    await rm(root, { recursive: true, force: true });
  });

  it('starts regex repairs without the previous result identity', () => {
    const recovery = regexErrorRecovery(
      new Error('regex parse error:\nerror: unclosed group'),
      LocalRipgrepQuerySchema.parse({
        path: root,
        searchText: '(',
        snapshot: `lexical-live-v1:${'a'.repeat(64)}`,
      })
    );
    const next = recovery.next as { repair: { query: Query } };
    expect(next.repair.query.snapshot).toBeUndefined();
    expect(next.repair.query.regex).toBe('fixed');
  });

  it('covers every file and match using live continuations without filesystem inventories', async () => {
    const native = vi.spyOn(contextUtils, 'searchRipgrep');
    const inventory = vi.spyOn(contextUtils, 'queryFileSystem');
    const first = await search(query);
    expect(first.data.pagination?.snapshot).toMatch(/^lexical-live-v1:/);
    const pending = [first];
    const visited = new Set<string>();
    const found = new Set<string>();
    while (pending.length) {
      const row = pending.shift()!;
      expect(row.status).not.toBe('error');
      for (const file of row.data.files ?? [])
        for (const match of file.matches ?? [])
          found.add(`${file.path}:${match.line}`);
      for (const [name, next] of Object.entries(row.data.next ?? {})) {
        if (!['nextPage', 'nextMatchPage'].includes(name)) continue;
        const key = JSON.stringify(next.query);
        if (visited.has(key)) continue;
        visited.add(key);
        expect(next.tool).toBe('localSearch');
        expect(next.query.snapshot).toBe(first.data.pagination?.snapshot);
        pending.push(await search(next.query));
      }
      expect(visited.size).toBeLessThan(20);
    }
    expect(found.size).toBe(6);
    expect(native.mock.calls.length).toBeGreaterThan(1);
    expect(inventory).not.toHaveBeenCalled();
  });

  it.each(['add', 'remove-all'] as const)(
    'rejects changed results (%s) and executes a fresh restart',
    async change => {
      const first = await search(query);
      const next = first.data.next!.nextPage!;
      if (change === 'add')
        await writeFile(join(root, 'new.txt'), 'needle new\n');
      else
        for (const name of ['a.txt', 'b.txt', 'c.txt'])
          await writeFile(join(root, name), 'no hit\n');
      const stale = await search(next.query);
      expect(stale.status).toBe('error');
      expect(stale.data.errorCode).toBe('staleSnapshot');
      const restart = stale.data.next!.restart!;
      expect(restart.tool).toBe('localSearch');
      expect(restart.query.snapshot).toBeUndefined();
      expect((await search(restart.query)).status).not.toBe('error');
    }
  );
});
