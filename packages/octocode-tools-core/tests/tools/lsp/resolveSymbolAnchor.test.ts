import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';

const resolverMocks = vi.hoisted(() => ({
  resolvePosition: vi.fn(),
  resolvePositionFromContent: vi.fn(),
}));

vi.mock('@octocodeai/octocode-engine/lsp/uri', () => ({
  toUri: (filePath: string) => `file://${filePath}`,
}));

vi.mock('@octocodeai/octocode-engine/lsp/resolver', () => ({
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
  it.each([
    ['const use = object["123"];', 20],
    ['const use = object["run-task"];', 23],
    ['const use = import("./target.js");', 20],
    ['const use = target;', 15],
    ['const use = "😀" + café;', 22],
  ])(
    'preserves exact UTF-16 positions in %s without fuzzy resolution',
    async (content, character) => {
      tempDir = await mkdtemp(join(process.cwd(), '.tmp-octocode-anchor-'));
      const filePath = join(tempDir, 'fixture.ts');
      await writeFile(filePath, content);
      const position = { line: 0, character };
      const result = await resolveSymbolAnchor(
        { uri: filePath, operation: 'definition', position },
        'lspSearch'
      );
      expect(result.ok).toBe(true);
      if (result.ok)
        expect(result.value.resolvedSymbol.position).toEqual(position);
      expect(resolverMocks.resolvePositionFromContent).not.toHaveBeenCalled();
    }
  );

  it.each([
    { line: 1, character: 0 },
    { line: 0, character: 99 },
  ])('rejects an exact position outside the source: %j', async position => {
    tempDir = await mkdtemp(join(process.cwd(), '.tmp-octocode-anchor-'));
    const filePath = join(tempDir, 'fixture.ts');
    await writeFile(filePath, 'const value = 1;');
    const result = await resolveSymbolAnchor(
      { uri: filePath, operation: 'definition', position },
      'lspSearch'
    );
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error.errorType).toBe('anchor_drift');
    expect(resolverMocks.resolvePositionFromContent).not.toHaveBeenCalled();
  });

  it.each(['étarget', '東京', '𐐀', '$target', 'target\u0301', 'target\u200c'])(
    'counts complete Unicode identifier %s when reporting ambiguous anchors',
    async symbolName => {
      tempDir = await mkdtemp(join(process.cwd(), '.tmp-octocode-anchor-'));
      const filePath = join(tempDir, 'fixture.ts');
      const line = `function ${symbolName}() {}`;
      await writeFile(filePath, `${line}\n${line}\n\n`);
      resolverMocks.resolvePositionFromContent.mockReturnValue({
        position: { line: 1, character: 9 },
        foundAtLine: 2,
        lineOffset: 0,
        lineContent: line,
      });
      const result = await resolveSymbolAnchor(
        {
          uri: filePath,
          operation: 'definition',
          symbolName,
          lineHint: 3,
        } as never,
        'lspSearch'
      );
      expect(result.ok).toBe(false);
      expect(result.error).toMatchObject({ errorType: 'anchor_drift' });
    }
  );

  it.each([
    'étarget',
    'targeté',
    'target\u0301',
    'target\u200c',
    '$target',
    '𐐀target',
  ])(
    'does not count target inside %s as an ambiguous occurrence',
    async identifier => {
      tempDir = await mkdtemp(join(process.cwd(), '.tmp-octocode-anchor-'));
      const filePath = join(tempDir, 'fixture.ts');
      await writeFile(
        filePath,
        `const ${identifier} = 1;\nfunction target() {}\n\n`
      );
      resolverMocks.resolvePositionFromContent.mockReturnValue({
        position: { line: 1, character: 9 },
        foundAtLine: 2,
        lineOffset: 0,
        lineContent: 'function target() {}',
      });
      const result = await resolveSymbolAnchor(
        {
          uri: filePath,
          operation: 'definition',
          symbolName: 'target',
          lineHint: 3,
        } as never,
        'lspSearch'
      );
      expect(result.ok).toBe(false);
      expect(result.error).toMatchObject({ errorType: 'anchor_drift' });
    }
  );

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
        operation: 'definition',
        symbolName: 'target',
        lineHint: 1,
      } as never,
      'lspSearch'
    );

    expect(result.ok).toBe(true);
    expect(result.value.uri).toBe(`file://${filePath}`);
    expect(result.value.absolutePath).toBe(filePath);
    expect(result.value.resolvedSymbol.uri).toBe(`file://${filePath}`);
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

  it('rejects a stale hint instead of silently binding a nearby occurrence', async () => {
    tempDir = await mkdtemp(join(process.cwd(), '.tmp-octocode-anchor-'));
    const filePath = join(tempDir, 'fixture.ts');
    // Two occurrences of `target`; hint points at line 5, resolver finds line 3.
    const content = [
      'function target() {}',
      '',
      'function target(x: number) {}',
      '',
      'const unrelated = 1;',
      '',
    ].join('\n');
    await writeFile(filePath, content);
    resolverMocks.resolvePositionFromContent.mockReturnValue({
      position: { line: 2, character: 9 },
      foundAtLine: 3,
      lineOffset: 0,
      lineContent: 'function target(x: number) {}',
    });

    const result = await resolveSymbolAnchor(
      {
        uri: filePath,
        operation: 'definition',
        symbolName: 'target',
        lineHint: 5,
      } as never,
      'lspSearch'
    );

    expect(result.ok).toBe(false);
    expect(result.error).toMatchObject({
      errorType: 'anchor_drift',
      lineDeviation: 2,
    });
  });

  it('rejects a stale hint even when the symbol is unique', async () => {
    tempDir = await mkdtemp(join(process.cwd(), '.tmp-octocode-anchor-'));
    const filePath = join(tempDir, 'fixture.ts');
    const content = 'const pad = 1;\n\nfunction target() {}\n';
    await writeFile(filePath, content);
    resolverMocks.resolvePositionFromContent.mockReturnValue({
      position: { line: 2, character: 9 },
      foundAtLine: 3,
      lineOffset: 0,
      lineContent: 'function target() {}',
    });

    const result = await resolveSymbolAnchor(
      {
        uri: filePath,
        operation: 'definition',
        symbolName: 'target',
        lineHint: 5,
      } as never,
      'lspSearch'
    );

    expect(result.ok).toBe(false);
    expect(result.error).toMatchObject({ errorType: 'anchor_drift' });
  });

  it('an exact-line resolution of a multi-occurrence symbol stays unflagged', async () => {
    tempDir = await mkdtemp(join(process.cwd(), '.tmp-octocode-anchor-'));
    const filePath = join(tempDir, 'fixture.ts');
    const content = 'function target() {}\nfunction target(x: number) {}\n';
    await writeFile(filePath, content);
    resolverMocks.resolvePositionFromContent.mockReturnValue({
      position: { line: 1, character: 9 },
      foundAtLine: 2,
      lineOffset: 0,
      lineContent: 'function target(x: number) {}',
    });

    const result = await resolveSymbolAnchor(
      {
        uri: filePath,
        operation: 'definition',
        symbolName: 'target',
        lineHint: 2,
      } as never,
      'lspSearch'
    );

    expect(result.ok).toBe(true);
    expect(result.value.resolvedSymbol.isAmbiguous).toBeUndefined();
    expect(result.value.resolvedSymbol.lineDeviation).toBeUndefined();
  });
});
