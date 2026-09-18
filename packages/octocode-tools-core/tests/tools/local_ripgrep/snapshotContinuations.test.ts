import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import {
  LocalSearchQuerySchema,
  type LocalSearchQuery,
} from '@octocodeai/octocode-core/schema';
import { executeDirectTool } from '../../helpers/executeDirectTool.js';
import { contextUtils } from '../../../src/utils/contextUtils.js';

type Next = { tool: string; query: LocalSearchQuery };
type Row = {
  status?: string;
  data: {
    files?: { path: string; matches?: { line: number; value: string }[] }[];
    pagination?: { snapshot?: string };
    next?: { nextPage?: Next; nextMatchPage?: Next; restart?: Next };
    errorCode?: string;
    snapshotReason?: string;
  };
};

async function search(query: LocalSearchQuery): Promise<Row> {
  const result = await executeDirectTool('localSearch', { queries: [query] });
  return (result.structuredContent as { results: Row[] }).results[0]!;
}

async function continuation(next: Next | undefined): Promise<Row> {
  expect(next?.tool).toBe('localSearch');
  expect(LocalSearchQuerySchema.safeParse(next?.query).success).toBe(true);
  return search(next!.query);
}

describe('native lexical snapshot continuations', () => {
  let home: string;
  let root: string;
  let query: LocalSearchQuery;

  beforeEach(async () => {
    await mkdir(join(process.cwd(), '.octocode', 'tmp'), { recursive: true });
    home = await mkdtemp(
      join(process.cwd(), '.octocode', 'tmp', 'lexical-snapshot-')
    );
    root = join(home, 'repo');
    await mkdir(root);
    for (const name of ['a.ts', 'b.ts', 'c.ts'])
      await writeFile(
        join(root, name),
        'needle first\nneedle second\nneedle third\n'
      );
    vi.stubEnv('OCTOCODE_HOME', home);
    query = LocalSearchQuerySchema.parse({
      path: root,
      searchText: 'needle',
      noIgnore: true,
      pageSize: 1,
      maxMatchesPerFile: 1,
      contextLines: 0,
      sort: 'path',
    });
  });

  afterEach(async () => {
    vi.restoreAllMocks();
    vi.unstubAllEnvs();
    await rm(home, { recursive: true, force: true });
  });

  it('executes both continuation axes with one native scan and a complete union', async () => {
    const scanner = vi.spyOn(contextUtils, 'searchRipgrep');
    const first = await search(query);
    const snapshot = first.data.pagination?.snapshot;
    expect(snapshot).toMatch(/^lexical-v1:[a-f0-9]{64}$/);
    const anchors = new Set<string>();
    let page: Row | undefined = first;
    while (page) {
      let matches: Row | undefined = page;
      while (matches) {
        expect(matches.status).not.toBe('error');
        expect(matches.data.pagination?.snapshot).toBe(snapshot);
        for (const file of matches.data.files ?? [])
          for (const match of file.matches ?? [])
            anchors.add(`${file.path}:${match.line}`);
        const nextMatches: Next | undefined = matches.data.next?.nextMatchPage;
        if (nextMatches) expect(nextMatches.query.snapshot).toBe(snapshot);
        matches = nextMatches ? await continuation(nextMatches) : undefined;
      }
      const nextPage: Next | undefined = page.data.next?.nextPage;
      if (nextPage) expect(nextPage.query.snapshot).toBe(snapshot);
      page = nextPage ? await continuation(nextPage) : undefined;
    }
    expect([...anchors].sort()).toEqual([
      'a.ts:1',
      'a.ts:2',
      'a.ts:3',
      'b.ts:1',
      'b.ts:2',
      'b.ts:3',
      'c.ts:1',
      'c.ts:2',
      'c.ts:3',
    ]);
    expect(scanner).toHaveBeenCalledTimes(1);
  });

  it('fails closed after an unmatched file changes, and executes a fresh restart', async () => {
    const other = join(root, 'd.ts');
    await writeFile(other, 'not a match\n');
    const scanner = vi.spyOn(contextUtils, 'searchRipgrep');
    const first = await search(query);
    await writeFile(other, 'needle newly matched\n');
    const stale = await continuation(first.data.next?.nextPage);
    expect(stale).toMatchObject({
      status: 'error',
      data: { errorCode: 'staleSnapshot', snapshotReason: 'sourceChanged' },
    });
    expect(scanner).toHaveBeenCalledTimes(1);
    expect(stale.data.next?.restart?.query.snapshot).toBeUndefined();
    const restarted = await continuation(stale.data.next?.restart);
    expect(restarted.status).not.toBe('error');
    expect(restarted.data.pagination?.snapshot).not.toBe(
      first.data.pagination?.snapshot
    );
    expect(scanner).toHaveBeenCalledTimes(2);
    const stillStale = await continuation(first.data.next?.nextPage);
    expect(stillStale.status).toBe('error');
    expect(scanner).toHaveBeenCalledTimes(2);
  });

  it('keeps ordinary ignore-aware searches live and refuses an arbitrary token', async () => {
    const scanner = vi.spyOn(contextUtils, 'searchRipgrep');
    const first = await search({ ...query, noIgnore: false });
    expect(first.data.pagination?.snapshot).toMatch(/^lexical-live-v1:/);
    await continuation(first.data.next?.nextPage);
    expect(scanner).toHaveBeenCalledTimes(2);
    const invalid = await search({ ...query, snapshot: 'not-a-snapshot' });
    expect(invalid).toMatchObject({
      status: 'error',
      data: { snapshotReason: 'invalidSnapshot' },
    });
    expect(scanner).toHaveBeenCalledTimes(2);
  });

  it('retains native regex diagnostics and never freezes an errored scan', async () => {
    const scanner = vi.spyOn(contextUtils, 'searchRipgrep');
    const invalid = await search({ ...query, searchText: 'def invoke(' });
    expect(invalid.status).toBe('error');
    expect(invalid.data.pagination?.snapshot).toBeUndefined();
    expect(scanner).toHaveBeenCalledTimes(1);
  });

  it('does not persist a snapshot on native incomplete coverage', async () => {
    const scan = await contextUtils.searchRipgrep({
      path: root,
      pattern: 'needle',
      noIgnore: true,
    });
    vi.spyOn(contextUtils, 'searchRipgrep').mockResolvedValueOnce({
      ...scan,
      stats: { ...scan.stats, errorCount: 1, firstError: 'permission denied' },
    });
    const first = await search(query);
    expect(first.data.pagination?.snapshot ?? '').not.toMatch(/^lexical-v1:/);
    expect(first.data.next?.nextPage?.query.snapshot ?? '').not.toMatch(
      /^lexical-v1:/
    );
  });

  it('does not freeze results if the scope changes during native search', async () => {
    const nativeSearch = contextUtils.searchRipgrep.bind(contextUtils);
    vi.spyOn(contextUtils, 'searchRipgrep').mockImplementationOnce(
      async options => {
        const result = await nativeSearch(options);
        await writeFile(join(root, 'changed-during-search.ts'), 'needle\n');
        return result;
      }
    );
    const first = await search(query);
    expect(first.data.pagination?.snapshot ?? '').not.toMatch(/^lexical-v1:/);
    expect(first.data.next?.nextPage?.query.snapshot ?? '').not.toMatch(
      /^lexical-v1:/
    );
  });

  it('reports wall cost separately from eliminated content scans', async () => {
    // A metadata-heavy fixture makes the tradeoff visible: this is a sensor,
    // not a timing assertion or a claim that every snapshot is faster.
    for (let i = 0; i < 30; i++)
      await writeFile(
        join(root, `file-${i}.ts`),
        `${'ordinary source line\n'.repeat(4_000)}needle\n`
      );
    const bootstrapStart = performance.now();
    await search({
      ...query,
      noIgnore: false,
      pageSize: 100,
      maxMatchesPerFile: 100,
    });
    const bootstrapMs = performance.now() - bootstrapStart;
    const scanner = vi.spyOn(contextUtils, 'searchRipgrep');
    const begin = performance.now();
    const first = await search({ ...query, maxMatchesPerFile: 100 });
    const firstMs = performance.now() - begin;
    const replayStart = performance.now();
    const replay = await continuation(first.data.next?.nextPage);
    const replayMs = performance.now() - replayStart;
    const liveStart = performance.now();
    // This fixture has no ignore rules; disabling eligibility gives the same
    // results while measuring the ordinary live path without snapshot checks.
    const live = await search({
      ...query,
      noIgnore: false,
      page: 2,
      maxMatchesPerFile: 100,
    });
    const liveMs = performance.now() - liveStart;
    expect(scanner).toHaveBeenCalledTimes(2);
    expect(live.data.files).toEqual(replay.data.files);
    // Also verify this actually read source bytes rather than a no-match fixture.
    expect(
      (await readFile(join(root, 'file-0.ts'))).byteLength
    ).toBeGreaterThan(50_000);
    process.stdout.write(
      `${JSON.stringify({
        sensor: 'lexical snapshot',
        bootstrapMs,
        firstMs,
        replayMs,
        liveMs,
        nativeScans: scanner.mock.calls.length,
      })}\n`
    );
  });
});
