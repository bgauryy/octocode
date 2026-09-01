import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { mkdtemp, rm, writeFile } from 'fs/promises';
import { join } from 'path';

import { fetchContent } from '../../../src/tools/local_fetch_content/fetchContent.js';

// Keep fixtures inside the package workspace for sandboxed verification.
const ROOT = process.cwd();

describe('fetchContent large-file gate applies to raw reads only', () => {
  let dir: string;
  let bigFile: string;

  beforeAll(async () => {
    dir = await mkdtemp(join(ROOT, 'octocode-fetch-large-gate-'));
    bigFile = join(dir, 'big.ts');
    // Real function declarations (not just prose) so minify:"symbols" has a
    // genuine skeleton to extract, well over the 100KB LARGE_FILE_THRESHOLD_KB.
    const functions = Array.from(
      { length: 5000 },
      (_, i) => `export function fn${i}() {\n  return ${i};\n}\n`
    ).join('');
    await writeFile(bigFile, functions, 'utf-8');
  });

  afterAll(async () => {
    await rm(dir, { recursive: true, force: true });
  });

  it('minify:"symbols" succeeds on a file over the raw-size threshold', async () => {
    const result = await fetchContent({
      path: bigFile,
      minify: 'symbols',
    } as never);

    expect(result.status).not.toBe('error');
    expect((result as { errorCode?: string }).errorCode).not.toBe(
      'fileTooLarge'
    );
    expect(typeof (result as { content?: string }).content).toBe('string');
  });

  it('minify:"standard" (no bounds) succeeds on a file over the raw-size threshold', async () => {
    const result = await fetchContent({
      path: bigFile,
      minify: 'standard',
    } as never);

    expect(result.status).not.toBe('error');
    expect((result as { errorCode?: string }).errorCode).not.toBe(
      'fileTooLarge'
    );
  });

  it('an unbounded read with no minify specified (defaults to standard) succeeds', async () => {
    const result = await fetchContent({ path: bigFile } as never);

    expect(result.status).not.toBe('error');
    expect((result as { errorCode?: string }).errorCode).not.toBe(
      'fileTooLarge'
    );
  });

  it('minify:"none" with no bounds is still rejected as too large (regression guard)', async () => {
    const result = await fetchContent({
      path: bigFile,
      minify: 'none',
    } as never);

    expect(result.status).toBe('error');
    expect((result as { errorCode?: string }).errorCode).toBe('fileTooLarge');
  });

  it('fullContent:true is still rejected as too large regardless of file size (regression guard)', async () => {
    const result = await fetchContent({
      path: bigFile,
      fullContent: true,
    } as never);

    expect(result.status).toBe('error');
    expect((result as { errorCode?: string }).errorCode).toBe('fileTooLarge');
  });

  it('minify:"none" with an explicit bound (matchString) still succeeds (existing exemption)', async () => {
    const result = await fetchContent({
      path: bigFile,
      minify: 'none',
      matchString: 'export function fn0()',
    } as never);

    expect(result.status).not.toBe('error');
    expect((result as { errorCode?: string }).errorCode).not.toBe(
      'fileTooLarge'
    );
  });
});
