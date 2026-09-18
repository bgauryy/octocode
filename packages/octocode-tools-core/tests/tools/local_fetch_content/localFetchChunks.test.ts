import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { fetchContent } from '../../../src/tools/local_fetch_content/fetchContent.js';
import { executeFetchContent } from '../../../src/tools/local_fetch_content/execution.js';
import { prepareDirectToolInput } from '../../helpers/prepareDirectToolInput.js';

describe('localFetch selected-view pagination', () => {
  let directory: string;
  beforeEach(async () => {
    directory = await mkdtemp(join(process.cwd(), '.tmp-local-fetch-'));
  });
  afterEach(async () => {
    await rm(directory, { recursive: true, force: true });
  });

  async function fixture(content: string, name = 'source.txt') {
    const path = join(directory, name);
    await writeFile(path, content);
    return path;
  }

  async function walk(initial: Record<string, unknown>) {
    let query = initial;
    const pages: Array<Record<string, any>> = [];
    const visited = new Set<string>();
    for (let i = 0; i < 1000; i++) {
      const key = JSON.stringify(query);
      expect(visited.has(key)).toBe(false);
      visited.add(key);
      const input = prepareDirectToolInput('localFetch', query);
      const response = await executeFetchContent(
        input as Parameters<typeof executeFetchContent>[0]
      );
      const row = (response.structuredContent as any).results[0];
      const page = row.data;
      expect(page.error).toBeUndefined();
      pages.push(page);
      const continuation = page.next?.continue;
      if (!continuation) return pages;
      expect(continuation.tool).toBe('localFetch');
      expect(continuation.query.goal).toBeUndefined();
      query = continuation.query;
    }
    throw new Error('Continuation did not terminate');
  }

  it.each(['lines', 'bytes'])(
    'reconstructs Unicode and CRLF in %s mode',
    async chunkType => {
      const content = 'alpha 🌍 café\r\n\r\nשלום\nlast\n';
      const path = await fixture(content);
      const pages = await walk({
        path,
        chunkType,
        limit: chunkType === 'lines' ? 1 : 3,
      });
      expect(pages.map(p => p.content).join('')).toBe(content);
      for (const page of pages) {
        expect(page.totalLines).toBe(4);
        expect(page.sourceBytes).toBe(Buffer.byteLength(content));
        expect(page.returnedBytes).toBe(Buffer.byteLength(page.content));
        expect(page.content).not.toContain('\uFFFD');
      }
    }
  );

  it('keeps a source range bounded and undecorated', async () => {
    const path = await fixture('one\r\ntwo\r\nthree\r\nfour\n');
    const pages = await walk({ path, startLine: 2, endLine: 3, limit: 1 });
    expect(pages.map(p => p.content).join('')).toBe('two\r\nthree\r\n');
    expect(pages.at(-1)!.isPartial).not.toBe(true);
    expect(pages.every(p => p.totalLines === 4)).toBe(true);
  });

  it.each(['lines', 'bytes'])(
    'preserves matched source windows in %s mode',
    async chunkType => {
      const path = await fixture(
        'skip\nneedle 🌍\nkeep\nskip\nneedle two\ntail\n'
      );
      const pages = await walk({
        path,
        matchString: '^needle',
        matchStringIsRegex: true,
        contextLines: 0,
        chunkType,
        limit: 1,
        minify: 'standard',
      });
      expect(pages.map(p => p.content).join('')).toBe(
        'needle 🌍\nneedle two\n'
      );
      expect(pages.every(p => p.contentView === 'none')).toBe(true);
      expect(
        pages.every(p => p.minifyFallback?.reason === 'match-evidence')
      ).toBe(true);
      expect([...new Set(pages.flatMap(p => p.matchedLines ?? []))]).toEqual([
        2, 5,
      ]);
    }
  );

  it('switches an oversized line to byte continuation without losing content', async () => {
    const content = 'short\n' + '🌍'.repeat(9000) + '\nafter\n';
    const path = await fixture(content);
    const pages = await walk({ path });
    expect(pages.map(p => p.content).join('')).toBe(content);
    expect(pages.some(p => p.pagination.chunkType === 'bytes')).toBe(true);
    expect(pages.every(p => p.returnedBytes <= 16387)).toBe(true);
  });

  it.each(['', ' \r\n\n'])(
    'preserves empty and whitespace content %j',
    async content => {
      const path = await fixture(content);
      const pages = await walk({ path });
      expect(pages.map(p => p.content).join('')).toBe(content);
      expect(pages[0]!.sourceBytes).toBe(Buffer.byteLength(content));
    }
  );

  it('rejects byte offsets inside a code point', async () => {
    const path = await fixture('🌍');
    const result = await fetchContent({
      path,
      chunkType: 'bytes',
      offset: 1,
    } as Parameters<typeof fetchContent>[0]);
    expect(result.status).toBe('error');
    expect(result.error).toMatch(/UTF-8|code point/);
  });

  it.each(['standard', 'symbols'])(
    'reconstructs the %s view after transformation',
    async minify => {
      const content =
        '// a comment\nexport function alpha() { return "🌍"; }\nexport function beta() { return 42; }\n';
      const path = await fixture(content, 'source.ts');
      const whole = await fetchContent({
        path,
        minify,
        fullContent: true,
      } as Parameters<typeof fetchContent>[0]);
      const pages = await walk({ path, minify, chunkType: 'bytes', limit: 7 });
      expect(pages.map(p => p.content).join('')).toBe(whole.content);
      expect(pages.every(p => p.contentView === minify)).toBe(true);
      expect(whole.content!.length).toBeLessThan(content.length);
    }
  );
});
