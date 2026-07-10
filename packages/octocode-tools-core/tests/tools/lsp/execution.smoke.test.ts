import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { executeLspGetSemantics } from '../../../src/tools/lsp/semantic_content/execution.js';

const tempDirs: string[] = [];

afterEach(async () => {
  await Promise.all(
    tempDirs.splice(0).map(dir => rm(dir, { recursive: true, force: true }))
  );
});

describe('lspGetSemantics tools-core smoke', () => {
  it('returns native documentSymbols for a local TypeScript file', async () => {
    const dir = await mkdtemp(join(process.cwd(), '.tmp-octocode-lsp-smoke-'));
    tempDirs.push(dir);
    const filePath = join(dir, 'fixture.ts');
    await writeFile(
      filePath,
      [
        'export function alpha() {',
        '  return 1;',
        '}',
        'export const beta = 2;',
      ].join('\n')
    );

    const result = await executeLspGetSemantics({
      queries: [
        {
          uri: filePath,
          type: 'documentSymbols',
          format: 'compact',
        },
      ],
    } as never);

    expect(result.isError).not.toBe(true);
    const structured = result.structuredContent as {
      results?: Array<{ data?: { type?: string; payload?: { kind?: string } } }>;
    };
    const row = structured?.results?.[0]?.data;
    expect(row?.type).toBe('documentSymbols');
    expect(row?.payload?.kind).toBe('documentSymbols');
  });
});
