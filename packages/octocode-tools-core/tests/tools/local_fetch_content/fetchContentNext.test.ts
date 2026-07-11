import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { mkdtemp, rm, writeFile } from 'fs/promises';
import { tmpdir, homedir } from 'os';
import { join } from 'path';

import { fetchContent } from '../../../src/tools/local_fetch_content/fetchContent.js';

// The global pathValidator allows HOME by default, so create the fixture under
// HOME rather than the OS temp dir (which is outside allowed roots on macOS).
const ROOT = process.env.HOME || homedir() || tmpdir();

describe('fetchContent next.continueChars', () => {
  let dir: string;
  let bigFile: string;
  let smallFile: string;

  beforeAll(async () => {
    dir = await mkdtemp(join(ROOT, 'octocode-fetch-next-'));
    bigFile = join(dir, 'big.txt');
    smallFile = join(dir, 'small.txt');
    // Plain prose (no code) so minification leaves length comfortably > limit.
    await writeFile(
      bigFile,
      'lorem ipsum dolor sit amet '.repeat(400),
      'utf-8'
    );
    await writeFile(smallFile, 'tiny content', 'utf-8');
  });

  afterAll(async () => {
    await rm(dir, { recursive: true, force: true });
  });

  it('emits a ready continuation query when char pagination hasMore', async () => {
    // Explicit charLength forces a partial page regardless of the configured
    // default output limit, so hasMore is deterministic.
    const result = await fetchContent({
      path: bigFile,
      minify: 'none',
      charOffset: 0,
      charLength: 2000,
    } as never);

    const pagination = result.pagination as {
      hasMore?: boolean;
      nextCharOffset?: number;
    };
    expect(pagination?.hasMore).toBe(true);

    const next = (result as { next?: { continueChars?: unknown } }).next;
    expect(next?.continueChars).toMatchObject({
      tool: 'localGetFileContent',
      query: {
        path: bigFile,
        charOffset: pagination.nextCharOffset,
        charLength: 2000,
        minify: 'none',
      },
    });
  });

  it('omits next when the whole file fits in one page', async () => {
    const result = await fetchContent({
      path: smallFile,
      minify: 'none',
    } as never);

    expect((result as { next?: unknown }).next).toBeUndefined();
  });

  it('fullContent:true returns the WHOLE file in one shot for content over the limit; default still paginates', async () => {
    const hugeFile = join(dir, 'huge.txt');
    // ~32k chars of prose — comfortably over the default output char limit.
    const body = 'lorem ipsum dolor sit amet '.repeat(1200);
    await writeFile(hugeFile, body, 'utf-8');

    // Default (no fullContent): a large file auto-paginates.
    const paged = await fetchContent({
      path: hugeFile,
      minify: 'none',
    } as never);
    expect((paged.pagination as { hasMore?: boolean })?.hasMore).toBe(true);
    expect((paged.content as string).length).toBeLessThan(body.length);

    // fullContent:true: the WHOLE file, no char-window pagination.
    const whole = await fetchContent({
      path: hugeFile,
      minify: 'none',
      fullContent: true,
    } as never);
    expect(whole.content).toBe(body);
    expect(
      (whole.pagination as { hasMore?: boolean } | undefined)?.hasMore
    ).toBeFalsy();
    expect((whole as { next?: unknown }).next).toBeUndefined();
  });
});

describe('fetchContent minify:"symbols" char pagination', () => {
  let dir: string;
  let manyFnFile: string;
  let smallFnFile: string;

  const buildFns = (count: number): string => {
    let body = '';
    for (let i = 0; i < count; i++) {
      body +=
        `export function fn${i}(a${i}: number, b${i}: string): boolean {\n` +
        `  const x = a${i} + ${i};\n` +
        `  return x > 0 && b${i}.length > 0;\n` +
        `}\n\n`;
    }
    return body;
  };

  beforeAll(async () => {
    dir = await mkdtemp(join(ROOT, 'octocode-fetch-symbols-'));
    manyFnFile = join(dir, 'many.ts');
    smallFnFile = join(dir, 'few.ts');
    await writeFile(manyFnFile, buildFns(60), 'utf-8');
    await writeFile(smallFnFile, buildFns(3), 'utf-8');
  });

  afterAll(async () => {
    await rm(dir, { recursive: true, force: true });
  });

  it('windows the skeleton and emits a symbols continuation when hasMore', async () => {
    const result = await fetchContent({
      path: manyFnFile,
      minify: 'symbols',
      charOffset: 0,
      charLength: 400,
    } as never);

    expect(result.contentView).toBe('symbols');

    const pagination = result.pagination as {
      hasMore?: boolean;
      nextCharOffset?: number;
      totalChars?: number;
      charOffset?: number;
      charLength?: number;
    };
    expect(pagination?.hasMore).toBe(true);
    expect(pagination?.charOffset).toBe(0);
    expect(typeof pagination?.nextCharOffset).toBe('number');
    // Pagination reflects the SKELETON's totalChars, not the raw file length.
    expect(pagination?.totalChars).toBeLessThan(buildFns(60).length);
    // Partial window is shorter than the whole skeleton.
    expect((result.content as string).length).toBeLessThan(
      pagination!.totalChars!
    );

    const next = (result as { next?: { continueChars?: unknown } }).next;
    expect(next?.continueChars).toMatchObject({
      tool: 'localGetFileContent',
      query: {
        path: manyFnFile,
        charOffset: pagination.nextCharOffset,
        // The continuation echoes the actual (semantic-snapped) page length,
        // not the originally requested charLength — a page that snapped to a
        // boundary continues at that snapped width.
        charLength: pagination.charLength,
        minify: 'symbols',
      },
    });
  });

  it('following the continuation returns the next skeleton window', async () => {
    const first = await fetchContent({
      path: manyFnFile,
      minify: 'symbols',
      charOffset: 0,
      charLength: 400,
    } as never);

    const continuation = (
      first as {
        next?: {
          continueChars?: {
            query?: Record<string, unknown>;
          };
        };
      }
    ).next?.continueChars?.query;
    expect(continuation).toBeDefined();

    const second = await fetchContent(continuation as never);

    expect(second.contentView).toBe('symbols');
    const secondPagination = second.pagination as {
      charOffset?: number;
      hasMore?: boolean;
    };
    // Second window starts where the first left off.
    expect(secondPagination?.charOffset).toBe(
      (first.pagination as { nextCharOffset?: number }).nextCharOffset
    );
    // Different slice of the skeleton than the first window.
    expect(second.content).not.toBe(first.content);
  });

  it('returns the whole skeleton with no next when it fits in one page', async () => {
    const result = await fetchContent({
      path: smallFnFile,
      minify: 'symbols',
    } as never);

    expect(result.contentView).toBe('symbols');
    expect((result as { next?: unknown }).next).toBeUndefined();
    expect(result.pagination).toBeUndefined();
    // Skeleton lists all three function signatures.
    expect(result.content).toContain('fn0');
    expect(result.content).toContain('fn2');
  });
});
