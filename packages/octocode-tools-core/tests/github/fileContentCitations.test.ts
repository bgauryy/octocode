import { describe, expect, it } from 'vitest';
import { ContentSanitizer } from '@octocodeai/octocode-engine/contentSanitizer';
import { FileContentQueryLocalSchema } from '@octocodeai/octocode-core/schema';
import { processFileContentAPI } from '../../src/github/fileContentProcess.js';
import { applyContentPagination } from '../../src/github/fileContentPagination.js';
import { transformFileContentResult } from '../../src/providers/github/githubContent.js';
import {
  mapFileContentProviderResult,
  mapFileContentToolQuery,
} from '../../src/tools/providerMappers/fileContent.js';
import { buildGithubFetchContentFinalizer } from '../../src/tools/github_fetch_content/finalizer.js';
import type { FileEntry } from '../../src/tools/github_fetch_content/finalizer/types.js';
import { matchContext } from '../../src/utils/file/matchContext.js';

const scope = {
  owner: 'fixture',
  repo: 'source',
  branch: 'a'.repeat(40),
  path: 'source.txt',
  minify: 'none',
};

/** Exercise every production content/result boundary against one immutable raw
 * source; network discovery/cache behavior is independent of page shaping. */
async function readPage(
  source: string,
  input: Record<string, unknown>
): Promise<FileEntry> {
  const query = {
    ...FileContentQueryLocalSchema.parse(input),
    minify: (input.minify ?? 'none') as 'none' | 'standard' | 'symbols',
  };
  const context = matchContext(query);
  const processed = await processFileContentAPI(
    source,
    query.owner,
    query.repo,
    query.branch ?? '',
    query.path,
    query.fullContent ?? false,
    query.startLine,
    query.endLine,
    context.contextLines ?? 0,
    query.matchString,
    query.matchStringIsRegex,
    query.matchStringCaseSensitive,
    query.minify,
    context.contextBytes
  );
  const page = await applyContentPagination(processed, query);
  const provider = transformFileContentResult(
    page,
    mapFileContentToolQuery(query)
  );
  const data = mapFileContentProviderResult(provider, query);
  const finalize = buildGithubFetchContentFinalizer<typeof query>();
  const result = finalize({
    queries: [query],
    results: [{ index: 0, status: 'success', data }],
  } as never);
  return (
    result.structuredContent.results as Array<{ data: { files: FileEntry[] } }>
  )[0]!.data.files[0]!;
}

async function readAll(source: string, initial: Record<string, unknown>) {
  const pages: FileEntry[] = [];
  let query = initial;
  for (let count = 0; count < 30; count++) {
    const page = await readPage(source, query);
    pages.push(page);
    const next = page.next?.continue;
    if (!next) return pages;
    expect(next.tool).toBe('ghGetFileContent');
    expect(FileContentQueryLocalSchema.safeParse(next.query).success).toBe(
      true
    );
    expect(next.query.branch).toBe(scope.branch);
    query = next.query;
  }
  throw new Error('Continuation did not terminate');
}

describe('GitHub exact-content pagination and source citations', () => {
  it('already covers a scoped 250-line region in one 16 KiB page, with exact citations', async () => {
    const lines = Array.from({ length: 400 }, (_, i) => `line ${i + 1}\n`);
    const query = { ...scope, startLine: 101, endLine: 350 };
    const pages = await readAll(lines.join(''), query);
    expect(pages).toHaveLength(1);
    expect(pages[0]!.content).toBe(lines.slice(100, 350).join(''));
    expect(pages[0]!.returnedBytes).toBeLessThanOrEqual(16384);
    expect(pages[0]!.sourceLineRanges).toEqual([{ start: 101, end: 350 }]);
    const explicit = await readAll(lines.join(''), { ...query, limit: 100 });
    expect(explicit).toHaveLength(3);
    expect(explicit.map(page => page.content).join('')).toBe(pages[0]!.content);
    expect(explicit.map(page => page.sourceLineRanges)).toEqual([
      [{ start: 101, end: 200 }],
      [{ start: 201, end: 300 }],
      [{ start: 301, end: 350 }],
    ]);
  });

  it('retains the ordinary 100-line default and cites the page rather than the full selection', async () => {
    const source = Array.from(
      { length: 220 },
      (_, i) => `record ${i + 1}\n`
    ).join('');
    const pages = await readAll(source, scope);
    expect(pages).toHaveLength(3);
    expect(pages.map(page => page.content).join('')).toBe(source);
    expect(pages.map(page => page.sourceLineRanges)).toEqual([
      [{ start: 1, end: 100 }],
      [{ start: 101, end: 200 }],
      [{ start: 201, end: 220 }],
    ]);
  });

  it('preserves Unicode and CRLF bytes while mapping partial-line byte pages', async () => {
    const pages = await readAll('header\n😀x\r\n最後\ntail', {
      ...scope,
      startLine: 2,
      endLine: 3,
      chunkType: 'bytes',
      limit: 4,
    });
    expect(pages.map(page => page.content)).toEqual(['😀', 'x\r\n最', '後\n']);
    expect(pages.map(page => page.content).join('')).toBe('😀x\r\n最後\n');
    expect(pages.map(page => page.sourceLineRanges)).toEqual([
      [{ start: 2, end: 2 }],
      [{ start: 2, end: 3 }],
      [{ start: 3, end: 3 }],
    ]);
  });

  it('maps disjoint line matches without claiming the intervening source', async () => {
    const source = 'head\r\nneedle π\r\nskip\r\nneedle 😀';
    const pages = await readAll(source, {
      ...scope,
      matchString: 'needle',
      contextLines: 0,
      limit: 1,
    });
    expect(pages.map(page => page.content).join('')).toBe(
      'needle π\r\nneedle 😀'
    );
    expect(pages.map(page => page.sourceLineRanges)).toEqual([
      [{ start: 2, end: 2 }],
      [{ start: 4, end: 4 }],
    ]);
    expect(pages.map(page => page.matchedLines)).toEqual([[2], [4]]);
    const combined = await readAll(source, {
      ...scope,
      matchString: 'needle',
      contextLines: 0,
    });
    expect(combined).toHaveLength(1);
    expect(combined[0]!.sourceLineRanges).toEqual([
      { start: 2, end: 2 },
      { start: 4, end: 4 },
    ]);
  });

  it('keeps the byte budget when a selected source line exceeds one page', async () => {
    const line = `${'é'.repeat(10_000)}\n`;
    const pages = await readAll(`head\n${line}tail`, {
      ...scope,
      startLine: 2,
      endLine: 2,
    });
    expect(pages).toHaveLength(2);
    expect(pages.map(page => page.content).join('')).toBe(line);
    expect(pages.every(page => page.returnedBytes! <= 16384)).toBe(true);
    expect(pages.map(page => page.sourceLineRanges)).toEqual([
      [{ start: 2, end: 2 }],
      [{ start: 2, end: 2 }],
    ]);
  });

  it('cites disjoint byte windows when their separator does not invent a view line', async () => {
    const pages = await readAll('skip\nneedle\nnot blank\nneedle\n', {
      ...scope,
      matchString: 'needle',
      contextBytes: 0,
      chunkType: 'bytes',
      limit: 7,
    });
    expect(pages.map(page => page.content)).toEqual(['needle\n', 'needle']);
    expect(pages.map(page => page.sourceLineRanges)).toEqual([
      [{ start: 2, end: 2 }],
      [{ start: 4, end: 4 }],
    ]);
  });

  it('omits original coordinates for synthetic byte separators', async () => {
    const pages = await readAll('skip\nneedle\nnot blank\nneedle\n', {
      ...scope,
      matchString: 'needle',
      contextBytes: 1,
      chunkType: 'bytes',
      limit: 3,
    });
    expect(pages.map(page => page.content).join('')).toBe(
      '\nneedle\n\n\nneedle\n'
    );
    expect(pages.every(page => page.sourceLineRanges === undefined)).toBe(true);
  });

  it('does not restore coordinates after multiline redaction, including byte-match selection', async () => {
    const key = [
      ['-----BEGIN', 'RSA PRIVATE KEY-----'].join(' '),
      'A'.repeat(64),
      'B'.repeat(64),
      ['-----END', 'RSA PRIVATE KEY-----'].join(' '),
    ].join('\n');
    const source = `before\n${key}\nneedle after key\n`;
    const sanitized = ContentSanitizer.sanitizeContent(source, scope.path);
    expect(sanitized.hasSecrets).toBe(true);
    expect(sanitized.content.split('\n').length).toBeLessThan(
      source.split('\n').length
    );
    for (const selector of [
      {},
      { startLine: 1, endLine: 7 },
      { matchString: 'needle', contextBytes: 0, chunkType: 'bytes' },
    ]) {
      const pages = await readAll(source, { ...scope, ...selector });
      expect(pages.every(page => page.sourceLineRanges === undefined)).toBe(
        true
      );
      expect(pages.map(page => page.content).join('')).not.toContain(key);
      if ('matchString' in selector)
        expect(pages.map(page => page.content).join('')).toBe('needle');
    }
  });

  it.each(['standard', 'symbols'])(
    'does not label the %s view as exact source',
    async minify => {
      const pages = await readAll(
        'export function item() {\n  return 1;\n}\n',
        { ...scope, path: 'source.ts', minify }
      );
      expect(pages.every(page => page.sourceLineRanges === undefined)).toBe(
        true
      );
    }
  );
});
