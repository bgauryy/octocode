import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, beforeEach, expect, it } from 'vitest';
import { fetchContent } from '../../../src/tools/local_fetch_content/fetchContent.js';
import { prepareDirectToolInput } from '../../helpers/prepareDirectToolInput.js';
import { executeFetchContent } from '../../../src/tools/local_fetch_content/execution.js';

let directory: string;
beforeEach(async () => {
  directory = await mkdtemp(join(process.cwd(), '.tmp-byte-context-'));
});
afterEach(async () => {
  await rm(directory, { recursive: true, force: true });
});
async function fixture(content: string) {
  const path = join(directory, 'source.txt');
  await writeFile(path, content);
  return path;
}
async function walk(query: Record<string, unknown>) {
  const pages: any[] = [];
  for (let i = 0; i < 100; i++) {
    const input = prepareDirectToolInput('localFetch', query);
    const result = await executeFetchContent(
      input as Parameters<typeof executeFetchContent>[0]
    );
    const page = (result.structuredContent as any).results[0].data;
    expect(page.error).toBeUndefined();
    pages.push(page);
    if (!page.next?.continue) return pages;
    query = page.next.continue.query;
  }
  throw new Error('Byte context continuation failed to terminate');
}

it('defaults byte reads to 256 bytes around the occurrence rather than whole lines', async () => {
  const path = await fixture('a'.repeat(2000) + 'needle' + 'b'.repeat(2000));
  const pages = await walk({
    path,
    matchString: 'needle',
    chunkType: 'bytes',
    limit: 73,
  });
  expect(pages.map(p => p.content).join('')).toBe(
    'a'.repeat(256) + 'needle' + 'b'.repeat(256)
  );
});

it('keeps UTF-8 characters whole and executes contextBytes continuations', async () => {
  const path = await fixture('before🌍needleéafter');
  const pages = await walk({
    path,
    matchString: 'needle',
    chunkType: 'bytes',
    contextBytes: 1,
    limit: 3,
  });
  expect(pages.map(p => p.content).join('')).toBe('🌍needleé');
  expect(pages.every(p => !p.content.includes('\uFFFD'))).toBe(true);
});

it('pins implicit line context when oversized lines switch pagination units', async () => {
  const content = 'before\n' + 'x'.repeat(18000) + 'needle\nafter\n';
  const path = await fixture(content);
  const pages = await walk({ path, matchString: 'needle' });
  expect(pages.map(p => p.content).join('')).toBe(content);
  expect(pages.some(p => p.next?.continue.query.contextLines === 5)).toBe(true);
});

it('redacts complete secrets before slicing byte context', async () => {
  const secret = 'ghp_' + 'A'.repeat(36);
  const path = await fixture('needle ' + secret);
  const result = await fetchContent({
    path,
    matchString: 'needle',
    chunkType: 'bytes',
    contextBytes: 12,
  } as Parameters<typeof fetchContent>[0]);
  expect(result.content).not.toContain('ghp_');
  expect(result.content).not.toContain('AAAA');
});
