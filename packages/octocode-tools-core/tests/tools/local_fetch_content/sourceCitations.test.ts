import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { executeFetchContent } from '../../../src/tools/local_fetch_content/execution.js';
import { prepareDirectToolInput } from '../../helpers/prepareDirectToolInput.js';

describe('localFetch source citations and bounded selection cost', () => {
  let directory: string;
  beforeEach(async () => {
    directory = await mkdtemp(join(process.cwd(), '.tmp-citations-'));
  });
  afterEach(async () => {
    await rm(directory, { recursive: true, force: true });
  });
  async function fixture(content: string) {
    const path = join(directory, 'source.ts');
    await writeFile(path, content);
    return path;
  }
  async function read(query: Record<string, unknown>) {
    const result = await executeFetchContent(
      prepareDirectToolInput('localFetch', query) as Parameters<typeof executeFetchContent>[0]
    );
    const data = (result.structuredContent as { results: Array<{ data: Record<string, any> }> }).results[0]!.data;
    return { data, text: result.content?.filter(item => item.type === 'text').map(item => item.text).join('') ?? '' };
  }

  it('returns a requested 221-line region in one bounded response', async () => {
    const source = Array.from({ length: 300 }, (_, i) => `const n${i} = ${i};\n`);
    const path = await fixture(source.join(''));
    const { data } = await read({ path, startLine: 40, endLine: 260 });
    expect(data.content).toBe(source.slice(39, 260).join(''));
    expect(data.pagination.hasMore).toBe(false);
    expect(data.sourceLineRanges).toEqual([{ start: 40, end: 260 }]);
    expect(data.returnedBytes).toBeLessThanOrEqual(16384);
  });

  it('keeps path-only reads at 100 lines and honors explicit smaller limits', async () => {
    const path = await fixture(Array.from({ length: 250 }, (_, i) => `${i}\n`).join(''));
    expect((await read({ path })).data.returnedLines).toBe(100);
    expect((await read({ path, startLine: 20, endLine: 240, limit: 3 })).data.returnedLines).toBe(3);
  });

  it('numbers rendered matches with original source lines while JSON stays exact', async () => {
    const path = await fixture('skip\r\nneedle 🌍\r\nskip\r\nneedle two\r\n');
    const first = await read({ path, matchString: 'needle', contextLines: 0, limit: 1 });
    expect(first.data.content).toBe('needle 🌍\r\n');
    expect(first.data.sourceLineRanges).toEqual([{ start: 2, end: 2 }]);
    expect(first.text).toContain('2: needle 🌍');
    const second = await read(first.data.next.continue.query);
    expect(second.data.content).toBe('needle two\r\n');
    expect(second.data.sourceLineRanges).toEqual([{ start: 4, end: 4 }]);
    expect(second.text).toContain('4: needle two');
    expect(second.data.pagination.hasMore).toBe(false);
  });

  it('replays large selected views without dropping bytes or source anchors', async () => {
    const source = Array.from({ length: 350 }, (_, i) => `// ${i} ${'x'.repeat(150)}\n`);
    const path = await fixture(source.join(''));
    let query: Record<string, unknown> = { path, startLine: 20, endLine: 330 };
    const chunks: string[] = [];
    const lines: number[] = [];
    for (let attempt = 0; attempt < 10; attempt++) {
      const { data } = await read(query);
      chunks.push(data.content);
      expect(data.returnedBytes).toBeLessThanOrEqual(16384);
      for (const range of data.sourceLineRanges) {
        for (let line = range.start; line <= range.end; line++) lines.push(line);
      }
      if (!data.next?.continue) break;
      query = data.next.continue.query;
    }
    expect(chunks.join('')).toBe(source.slice(19, 330).join(''));
    expect(lines).toEqual(Array.from({ length: 311 }, (_, i) => i + 20));
  });

  it('does not invent source positions for transformed output', async () => {
    const path = await fixture('// comment\nexport function f() { return 1; }\n');
    const { data, text } = await read({ path, minify: 'standard' });
    expect(data.sourceLineRanges).toBeUndefined();
    expect(text).not.toContain('content (source lines)');
  });

  it('keeps byte continuations on the original source line', async () => {
    const path = await fixture('skip\nneedle 🌍 tail\nend\n');
    let query: Record<string, unknown> = { path, startLine: 2, endLine: 2, chunkType: 'bytes', limit: 3 };
    const chunks: string[] = [];
    for (let attempt = 0; attempt < 10; attempt++) {
      const { data } = await read(query);
      expect(data.sourceLineRanges).toEqual([{ start: 2, end: 2 }]);
      chunks.push(data.content);
      if (!data.next?.continue) break;
      query = data.next.continue.query;
    }
    expect(chunks.join('')).toBe('needle 🌍 tail\n');
  });

  it('omits source maps when redaction has changed the selected text', async () => {
    const token = `ghp_${'A'.repeat(36)}`;
    const path = await fixture(`before\n${token}\nafter\n`);
    const { data, text } = await read({ path });
    expect(data.content).not.toContain(token);
    expect(data.content).toContain('REDACTED');
    expect(data.sourceLineRanges).toBeUndefined();
    expect(text).not.toContain('content (source lines)');
  });
});
