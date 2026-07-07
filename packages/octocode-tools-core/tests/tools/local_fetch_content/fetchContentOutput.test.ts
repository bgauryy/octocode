import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { homedir, tmpdir } from 'node:os';
import { join } from 'node:path';

import { _resetConfigCache, setRuntimeSurface } from '@octocodeai/config';
import { executeDirectTool } from '../../../src/tools/directToolCatalog.js';

const ROOT = process.env.HOME || homedir() || tmpdir();

function firstText(result: Awaited<ReturnType<typeof executeDirectTool>>): string {
  const block = result.content?.find(part => 'text' in part && typeof part.text === 'string');
  return block && 'text' in block ? block.text : '';
}

function firstData<T>(result: Awaited<ReturnType<typeof executeDirectTool>>): T | undefined {
  return (
    result.structuredContent as { results?: Array<{ data?: T }> } | undefined
  )?.results?.[0]?.data;
}

describe('localGetFileContent direct text output', () => {
  let dir: string;

  beforeAll(async () => {
    process.env.ENABLE_LOCAL = 'true';
    setRuntimeSurface('cli');
    _resetConfigCache();
    dir = await mkdtemp(join(ROOT, 'octocode-local-content-output-'));
  });

  afterAll(async () => {
    await rm(dir, { recursive: true, force: true });
  });

  it('renders fetched content lines without YAML block-scalar indentation drift', async () => {
    const file = join(dir, 'nested.ts');
    const source = [
      'export function demo() {',
      '    const nested = 1;',
      '      return nested;',
      '}',
      '',
    ].join('\n');
    await writeFile(file, source, 'utf8');

    const result = await executeDirectTool('localGetFileContent', {
      queries: [{ path: file, minify: 'none', fullContent: true }],
    });

    expect(firstData<{ content?: string }>(result)?.content).toBe(source);

    const text = firstText(result);
    expect(text).toContain(source);
    expect(text).not.toContain('content: |');
    expect(text).not.toContain('\n        const nested = 1;');
    expect(text).not.toContain('\n          return nested;');
  });

  it('rejects minify:"symbols" combined with a line range instead of silently ignoring it', async () => {
    const file = join(dir, 'symbols-range.ts');
    await writeFile(file, 'export const a = 1;\nexport const b = 2;\n', 'utf8');

    const result = await executeDirectTool('localGetFileContent', {
      queries: [{ path: file, minify: 'symbols', startLine: 1, endLine: 1 }],
    });

    const data = firstData<{ error?: string }>(result);
    expect(data?.error ?? firstText(result)).toContain('symbols');
    expect(data?.error ?? firstText(result)).toMatch(/startLine|matchString/);
  });

  it('rejects minify:"symbols" combined with matchString', async () => {
    const file = join(dir, 'symbols-match.ts');
    await writeFile(file, 'export const a = 1;\nexport const b = 2;\n', 'utf8');

    const result = await executeDirectTool('localGetFileContent', {
      queries: [{ path: file, minify: 'symbols', matchString: 'a' }],
    });

    const data = firstData<{ error?: string }>(result);
    expect(data?.error ?? firstText(result)).toContain('symbols');
  });
});
