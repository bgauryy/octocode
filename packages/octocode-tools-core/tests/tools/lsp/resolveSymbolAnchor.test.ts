import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';

const resolverMocks = vi.hoisted(() => ({
  resolvePosition: vi.fn(),
  resolvePositionFromContent: vi.fn(),
}));

vi.mock('octocode-lsp/resolver', () => ({
  SymbolResolutionError: class SymbolResolutionError extends Error {
    constructor(
      public readonly symbolName: string,
      public readonly lineHint: number,
      public readonly reason: string,
      public readonly searchRadius = 5
    ) {
      super(reason);
    }
  },
  SymbolResolver: vi.fn(function SymbolResolver() {
    return resolverMocks;
  }),
}));

import { resolveSymbolAnchor } from '../../../src/tools/lsp/shared/resolveSymbolAnchor.js';

let tempDir: string | undefined;

afterEach(async () => {
  resolverMocks.resolvePosition.mockReset();
  resolverMocks.resolvePositionFromContent.mockReset();
  if (tempDir) {
    await rm(tempDir, { recursive: true, force: true });
    tempDir = undefined;
  }
});

describe('resolveSymbolAnchor', () => {
  it('resolves symbols from already-read file content', async () => {
    tempDir = await mkdtemp(join(process.cwd(), '.tmp-octocode-anchor-'));
    const filePath = join(tempDir, 'fixture.ts');
    const content = 'export function target() {}\n';
    const resolved = {
      position: { line: 0, character: 16 },
      foundAtLine: 1,
      lineOffset: 0,
      lineContent: 'export function target() {}',
    };
    await writeFile(filePath, content);
    resolverMocks.resolvePosition.mockReturnValue(resolved);
    resolverMocks.resolvePositionFromContent.mockReturnValue(resolved);

    const result = await resolveSymbolAnchor(
      {
        uri: filePath,
        type: 'definition',
        symbolName: 'target',
        lineHint: 1,
      } as never,
      'lspGetSemantics'
    );

    expect(result.ok).toBe(true);
    expect(resolverMocks.resolvePosition).not.toHaveBeenCalled();
    expect(resolverMocks.resolvePositionFromContent).toHaveBeenCalledWith(
      content,
      {
        symbolName: 'target',
        lineHint: 1,
        orderHint: 0,
      }
    );
  });
});
