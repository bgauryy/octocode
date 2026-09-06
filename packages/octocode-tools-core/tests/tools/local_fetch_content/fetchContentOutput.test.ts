import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

import { setRuntimeSurface } from '@octocodeai/config';
import { executeDirectTool } from '../../../src/tools/directToolCatalog.exec.js';

const ROOT = process.cwd();

function firstText(
  result: Awaited<ReturnType<typeof executeDirectTool>>
): string {
  const block = result.content?.find(
    part => 'text' in part && typeof part.text === 'string'
  );
  return block && 'text' in block ? block.text : '';
}

function firstData<T>(
  result: Awaited<ReturnType<typeof executeDirectTool>>
): T | undefined {
  return (
    result.structuredContent as { results?: Array<{ data?: T }> } | undefined
  )?.results?.[0]?.data;
}

describe('localGetFileContent direct text output', () => {
  let dir: string;

  beforeAll(async () => {
    process.env.ENABLE_LOCAL = 'true';
    setRuntimeSurface('cli');
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

  it('fullContent defaults to verbatim (minify:none) through the real executor — comments survive', async () => {
    const file = join(dir, 'commented.ts');
    const source = [
      '// leading comment',
      'const x = 1;',
      '',
      '/* block comment */',
      'const y = 2;',
      '',
    ].join('\n');
    await writeFile(file, source, 'utf8');

    // No explicit minify: the schema must not inject 'standard', and execution
    // must resolve fullContent→none so the whole file comes back byte-exact.
    const result = await executeDirectTool('localGetFileContent', {
      queries: [{ path: file, fullContent: true }],
    });

    const data = firstData<{ content?: string; contentView?: string }>(result);
    expect(data?.contentView).toBe('none');
    expect(data?.content).toBe(source);
    expect(data?.content).toContain('// leading comment');
    expect(data?.content).toContain('/* block comment */');
  });

  it('a plain read (no fullContent, no minify) still defaults to standard', async () => {
    const file = join(dir, 'plain.ts');
    await writeFile(
      file,
      '// strip me\nconst a = 1;\n\nconst b = 2;\n',
      'utf8'
    );

    const result = await executeDirectTool('localGetFileContent', {
      queries: [{ path: file }],
    });

    expect(firstData<{ contentView?: string }>(result)?.contentView).toBe(
      'standard'
    );
  });

  it('line ranges default to verbatim numbered slices', async () => {
    const file = join(dir, 'range.ts');
    await writeFile(
      file,
      ['// keep me', 'const a = 1;', '', 'const b = 2;', ''].join('\n'),
      'utf8'
    );

    const result = await executeDirectTool('localGetFileContent', {
      queries: [{ path: file, startLine: 1, endLine: 4 }],
    });

    const data = firstData<{
      content?: string;
      contentView?: string;
      startLine?: number;
      endLine?: number;
    }>(result);
    expect(data?.contentView).toBe('none');
    expect(data?.startLine).toBe(1);
    expect(data?.endLine).toBe(4);
    expect(data?.content).toBe(
      ['1→ // keep me', '2→ const a = 1;', '3→ ', '4→ const b = 2;'].join('\n')
    );
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
