import { expectExecutableNext } from '../helpers/executableNext.js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { GitHubProvider } from '../../src/providers/github/GitHubProvider.js';
import { executeDirectTool } from '../helpers/executeDirectTool.js';
import { FileContentQueryLocalSchema } from '@octocodeai/octocode-core/schema';
import type { FileEntry } from '../../src/tools/github_fetch_content/finalizer/types.js';
import { cleanup } from '../../src/serverConfig.js';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

const fixture = vi.hoisted(() => ({ source: '' }));
vi.mock('../../src/providers/factory.js', async importOriginal => ({
  ...(await importOriginal<typeof import('../../src/providers/factory.js')>()),
  getProvider: () => new GitHubProvider(),
}));
// Only acquisition is stubbed: the native transform, real provider mappings,
// direct execution, envelope, and executable continuations remain in the path.
vi.mock('../../src/github/fileContentRaw/cache.js', () => ({
  fetchCachedRawGitHubFileContent: vi.fn(async () => ({
    auth: 'fixture',
    rawResult: { data: { rawContent: fixture.source, branch: 'main' } },
  })),
}));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: vi.fn(async () => ({
    rest: { repos: { listCommits: vi.fn(async () => ({ data: [] })) } },
  })),
}));
vi.mock('../../src/github/codeSearch.js', () => ({
  searchGitHubCodeAPI: vi.fn(async () => ({
    data: {
      total_count: 1,
      items: [
        {
          path: 'source.ts',
          url: 'https://github.com/octo/fixture/blob/main/source.ts',
          repository: {
            nameWithOwner: 'octo/fixture',
            url: 'https://github.com/octo/fixture',
          },
          matches: [{ context: '// needle 🌍', positions: [[3, 9]] }],
        },
      ],
      pagination: {
        currentPage: 1,
        totalPages: 1,
        hasMore: false,
        totalMatches: 1,
      },
    },
    status: 200,
  })),
}));

const base = {
  owner: 'octo',
  repo: 'fixture',
  path: 'source.ts',
  branch: 'main',
};
beforeEach(() => {
  vi.stubEnv('ENABLE_CLONE', 'false');
  cleanup();
});
afterEach(() => {
  vi.unstubAllEnvs();
  cleanup();
});

async function read(query: Record<string, unknown>): Promise<FileEntry> {
  query = {
    reasoning: 'Exercise exact GitHub file evidence and continuations.',
    debug: true,
    ...query,
  };
  expect(FileContentQueryLocalSchema.safeParse(query).success).toBe(true);
  const out = await executeDirectTool('ghGetFileContent', { queries: [query] });
  expect(
    (out.structuredContent as { results: unknown[] }).results,
    JSON.stringify(out)
  ).toHaveLength(1);
  expectExecutableNext(out.structuredContent);
  const row = (
    out.structuredContent as {
      results: Array<{ status?: string; data: { files?: FileEntry[] } }>;
    }
  ).results[0]!;
  expect(row.status, JSON.stringify(row)).not.toBe('error');
  expect(row.data.files, JSON.stringify(row)).toHaveLength(1);
  return row.data.files![0]!;
}

async function collect(initial: Record<string, unknown>) {
  let query = initial;
  let content = '';
  const pages: FileEntry[] = [];
  for (;;) {
    const page = await read(query);
    pages.push(page);
    content += page.content;
    expect(pages.length).toBeLessThan(200);
    const next = page.next?.continue;
    if (!next) break;
    expect(page.content.length).toBeGreaterThan(0);
    expect(next.tool).toBe('ghGetFileContent');
    expect(next.query.offset).toBe(page.pagination?.nextOffset);
    if (page.pagination?.chunkType === 'bytes')
      expect(next.query.offset).toBe(Buffer.byteLength(content));
    expect(next.query.limit).toBe(page.pagination?.limit);
    for (const key of [
      'minify',
      'matchString',
      'matchStringIsRegex',
      'fullContent',
    ]) {
      expect(next.query[key]).toEqual(
        initial[key] ??
          (key === 'minify'
            ? 'none'
            : key === 'fullContent'
              ? false
              : undefined)
      );
    }
    query = next.query;
  }
  return { content, pages };
}

describe('GitHub evidence through the complete public provider path', () => {
  it('recovers an oversized full view with bounded, schema-valid chunks', async () => {
    fixture.source = '// header\n' + '🌍'.repeat(15000) + '\nend\n';
    const limited = await read({
      ...base,
      path: 'source.txt',
      fullContent: true,
    });
    expect(limited.errorCode).toBe('fullContentLimit');
    expect(limited.sourceBytes).toBe(Buffer.byteLength(fixture.source));
    expect(limited.next?.continue).toBeDefined();
    const { content, pages } = await collect(limited.next!.continue!.query);
    expect(content).toBe(fixture.source);
    expect(pages[0]!.pagination?.chunkType).toBe('lines');
    expect(
      pages.slice(1).every(page => page.pagination?.chunkType === 'bytes')
    ).toBe(true);
    expect(pages.every(page => !page.content.includes('\uFFFD'))).toBe(true);
  });

  it('reports empty match selection totals and rejects a split UTF-8 offset', async () => {
    fixture.source = '🌍\r\nlast\n';
    const empty = await read({
      ...base,
      matchString: 'absent',
      chunkType: 'bytes',
      limit: 3,
    });
    expect(empty.content).toBe('');
    expect(empty.selectedMatchCount).toBe(0);
    expect(empty.totalLines).toBe(2);
    expect(empty.pagination?.hasMore).toBe(false);
    const invalid = await executeDirectTool('ghGetFileContent', {
      queries: [{ ...base, chunkType: 'bytes', offset: 1, limit: 3 }],
    });
    expect(JSON.stringify(invalid)).toContain('UTF-8 code point boundary');
    expect(
      (invalid.structuredContent as { results: Array<{ status?: string }> })
        .results[0]?.status
    ).toBe('error');
  });
  it.each(['lines', 'bytes'] as const)(
    'search → match fetch → %s continuations has local/GitHub parity',
    async chunkType => {
      fixture.source =
        'head\r\n// needle 🌍  \r\nspacer\r\n// needle café\nend\n';
      const expected = '// needle 🌍  \r\n// needle café\n';
      const parent = join(process.cwd(), '.octocode', 'tmp');
      await mkdir(parent, { recursive: true });
      const root = await mkdtemp(join(parent, 'search-fetch-parity-'));
      const path = join(root, 'source.ts');
      await writeFile(path, fixture.source);
      try {
        for (const remote of [false, true]) {
          const searchTool = remote ? 'ghSearch' : 'localSearch';
          const fetchTool = remote ? 'ghGetFileContent' : 'localFetch';
          const search = await executeDirectTool(searchTool, {
            queries: [
              remote
                ? {
                    operation: 'code',
                    owner: 'octo',
                    repo: 'fixture',
                    keywords: ['needle'],
                  }
                : { path: root, searchText: 'needle' },
            ],
          });
          expectExecutableNext(search.structuredContent);
          const searched = search.structuredContent as {
            base?: string;
            results: Array<{
              data: { files: Array<{ path: string }>; next?: unknown };
            }>;
          };
          const file = searched.results[0]!.data.files[0]!;
          expect(file.path).toBe('source.ts');
          expect(searched.results[0]!.data.next).toBeUndefined();
          let query: Record<string, unknown> = {
            ...(remote
              ? { ...base, path: file.path }
              : { path: join(searched.base!, file.path) }),
            matchString: 'needle',
            contextLines: 0,
            minify: 'standard',
            chunkType,
            limit: chunkType === 'lines' ? 1 : 3,
          };
          delete query.startLine;
          delete query.endLine;
          let joined = '';
          const matched = new Set<number>();
          for (let i = 0; ; i++) {
            expect(i).toBeLessThan(100);
            const out = await executeDirectTool(fetchTool, {
              queries: [query],
            });
            expectExecutableNext(out.structuredContent);
            const row = (
              out.structuredContent as {
                results: Array<{ status?: string; data: any }>;
              }
            ).results[0]!;
            expect(row.status, JSON.stringify(row)).not.toBe('error');
            const page = remote ? row.data.files[0] : row.data;
            expect(page.totalLines).toBe(5);
            expect(page.sourceBytes).toBe(Buffer.byteLength(fixture.source));
            expect(page.pagination.totalBytes).toBe(
              Buffer.byteLength(expected)
            );
            expect(page.pagination.totalLines).toBe(2);
            expect(page.selectedMatchCount).toBe(2);
            expect(page.minifyFallback).toEqual({
              requested: 'standard',
              applied: 'none',
              reason: 'match-evidence',
            });
            expect(page.returnedBytes).toBe(Buffer.byteLength(page.content));
            expect(page.content).not.toContain('\uFFFD');
            joined += page.content;
            for (const line of page.matchedLines ?? []) matched.add(line);
            const next = page.next?.continue;
            if (!next) {
              expect(page.pagination.hasMore).toBe(false);
              break;
            }
            expect(next.query.matchString).toBe('needle');
            expect(next.query.contextLines).toBe(0);
            query = next.query;
          }
          expect(joined).toBe(expected);
          expect([...matched]).toEqual([2, 4]);
        }
      } finally {
        await rm(root, { recursive: true, force: true });
      }
    }
  );
  it.each(['py', 'rs', 'html', 'vue', 'svelte'])(
    'preserves independent literal payloads through local and GitHub %s pages',
    async extension => {
      const payload = 'alpha  \n\n\nbeta\t\n';
      fixture.source =
        extension === 'py'
          ? `value = """${payload}"""\n`
          : extension === 'rs'
            ? `pub const VALUE: &str = r#"${payload}"#;\n`
            : `<script>const marker = "<!-- retained literal -->"; console.log(marker);</script>\n<pre>${payload}</pre>\n`;
      const remote = await collect({
        ...base,
        path: `literal.${extension}`,
        minify: 'standard',
        chunkType: 'bytes',
        limit: 7,
      });
      expect(remote.content).toContain(payload);
      if (!['py', 'rs'].includes(extension))
        expect(remote.content).toContain('<!-- retained literal -->');
      const parent = join(process.cwd(), '.octocode', 'tmp');
      await mkdir(parent, { recursive: true });
      const root = await mkdtemp(join(parent, 'literal-fidelity-'));
      try {
        const path = join(root, `literal.${extension}`);
        await writeFile(path, fixture.source);
        let query: Record<string, unknown> = {
          path,
          minify: 'standard',
          chunkType: 'bytes',
          limit: 7,
        };
        let content = '';
        for (let page = 0; ; page++) {
          expect(page).toBeLessThan(100);
          const out = await executeDirectTool('localFetch', {
            queries: [query],
          });
          expectExecutableNext(out.structuredContent);
          const row = (
            out.structuredContent as {
              results: Array<{ status?: string; data: FileEntry }>;
            }
          ).results[0]!;
          expect(row.status, JSON.stringify(row)).not.toBe('error');
          content += row.data.content;
          const next = row.data.next?.continue;
          if (!next) break;
          query = next.query;
        }
        expect(content).toContain(payload);
        expect(content).toBe(remote.content);
      } finally {
        await rm(root, { recursive: true, force: true });
      }
    }
  );

  const anchors = [
    { line: '// only retries once', matchString: 'only retries once' },
    {
      line: 'import type { Anchor } from "./types";',
      matchString: 'import type',
    },
    {
      line: 'interface Anchor { value: number }',
      matchString: 'interface Anchor',
    },
    { line: 'type Anchor = { value: number };', matchString: 'type Anchor' },
    { line: '  // anchor\t  spacing  ', matchString: 'anchor\t  spacing' },
    {
      line: '// anchor 123',
      matchString: 'anchor\\s+\\d+',
      matchStringIsRegex: true,
    },
  ];
  describe.each(anchors)('$line', ({ line, ...selector }) => {
    it.each([undefined, 'none', 'standard'] as const)(
      'preserves exact selected source with minify=%s across pages',
      async minify => {
        fixture.source = [line, 'const unrelated = 1;', line].join('\n');
        const initial = {
          ...base,
          ...selector,
          contextLines: 0,
          chunkType: 'bytes',
          limit: 7,
          ...(minify === undefined ? {} : { minify }),
        };
        const { content, pages } = await collect(initial);
        const whole = await read({
          ...initial,
          chunkType: 'bytes',
          limit: 50000,
        });
        expect(content).toBe(whole.content);
        expect(pages.length).toBeGreaterThan(1);
        for (const page of pages) {
          expect(page.contentView).toBe('none');
          expect(page.selectedMatchCount).toBe(2);
          expect(
            (page.matchedLines ?? []).every(line => [1, 3].includes(line))
          ).toBe(true);
        }
        expect(content.split(line)).toHaveLength(3);
        expect(content).not.toContain('unrelated');
      }
    );
  });

  it('preserves research declarations and bindings in a standard full-file view', async () => {
    fixture.source = [
      'import type { Input } from "./input";',
      'interface LocalShape { value: number }',
      'type LocalAlias = LocalShape;',
      'export function target(input: Input): LocalAlias {',
      '  const retainedBinding = input.value + 1;',
      '  return { value: retainedBinding };',
      '}',
    ].join('\n');
    const whole = await read({
      ...base,
      fullContent: true,
      minify: 'standard',
    });
    expect(whole.contentView).toBe('standard');
    for (const anchor of [
      'import type',
      'interface LocalShape',
      'type LocalAlias',
      'retainedBinding',
    ]) {
      expect(whole.content).toContain(anchor);
    }
    const { content } = await collect({
      ...base,
      minify: 'standard',
      chunkType: 'bytes',
      limit: 7,
    });
    expect(content).toBe(whole.content);
  });

  it('redacts a matching secret before paging, retaining the surrounding anchor', async () => {
    const secret = 'ghp_' + 'AbCdEf0123456789'.repeat(2) + 'abcdef';
    fixture.source = `// anchor ${secret}\n// anchor visible`;
    const { content, pages } = await collect({
      ...base,
      matchString: 'anchor',
      contextLines: 0,
      chunkType: 'bytes',
      limit: 7,
    });
    expect(pages.length).toBeGreaterThan(1);
    expect(content).not.toContain(secret);
    expect(content).not.toContain(secret.slice(10, 25));
    expect(content).toContain('anchor');
    expect(content).toContain('REDACTED');
    expect(pages.every(page => page.contentView === 'none')).toBe(true);
  });

  it.each([false, true])(
    'reconstructs symbol windows with fullContent=%s',
    async fullContent => {
      fixture.source = Array.from(
        { length: 20 },
        (_, i) =>
          `export function target${i}(value: number): number {\n  const result = value + ${i};\n  return result;\n}`
      ).join('\n');
      const whole = await read({
        ...base,
        minify: 'symbols',
        fullContent: true,
      });
      const { content, pages } = await collect({
        ...base,
        minify: 'symbols',
        ...(fullContent
          ? { fullContent: true }
          : { chunkType: 'bytes', limit: 40 }),
      });
      expect(pages.length).toBe(
        fullContent ? 1 : Math.ceil(Buffer.byteLength(whole.content) / 40)
      );
      expect(
        pages.every(page => page.contentView === 'symbols'),
        JSON.stringify(pages[0])
      ).toBe(true);
      expect(content).toBe(whole.content);
      expect(content).toContain('target19');
    }
  );

  it.each(['none', 'standard', 'symbols'] as const)(
    'retains typed scanner limits and executable recovery in %s mode',
    async minify => {
      fixture.source = 'first line\n' + 'x'.repeat(10_000_001);
      const page = await read({
        ...base,
        path: 'source.txt',
        minify,
        chunkType: 'bytes',
        limit: 7,
      });
      expect(page.errorCode).toBe('contentSecurityLimit');
      expect(page.terminalLimit).toBe(true);
      expect(page.partialReasons).toContain(
        'security-selected-view-size-limit'
      );
      expect(page.contentView).toBeUndefined();
      expect(page.next?.continue).toBeUndefined();
      const next = page.next?.readBoundedLines;
      expect(next?.tool).toBe('ghGetFileContent');
      const recovered = await read(next!.query);
      expect(recovered.content).toBe('first line\n');
      expect(recovered.errorCode).toBeUndefined();
    }
  );
});

it('uses byte context defaults through the GitHub provider and every continuation', async () => {
  fixture.source = 'a'.repeat(2000) + 'needle' + 'b'.repeat(2000);
  const { content, pages } = await collect({
    ...base,
    matchString: 'needle',
    chunkType: 'bytes',
    limit: 61,
  });
  expect(content).toBe('a'.repeat(256) + 'needle' + 'b'.repeat(256));
  expect(pages[0]!.next?.continue?.query.contextBytes).toBe(256);
});

it('supports zero byte context and separate same-line matches through GitHub', async () => {
  fixture.source = 'one needle gap needle two';
  const { content } = await collect({
    ...base,
    matchString: 'needle',
    chunkType: 'bytes',
    contextBytes: 0,
    limit: 4,
  });
  expect(content).toBe('needle\nneedle');
});

it('preserves implicit line selection when GitHub pagination switches to bytes', async () => {
  fixture.source = 'before\n' + 'x'.repeat(18000) + 'needle\nafter\n';
  const { content } = await collect({ ...base, matchString: 'needle' });
  expect(content).toBe(fixture.source);
});
