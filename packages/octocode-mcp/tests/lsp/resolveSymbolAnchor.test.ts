import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  resolveFileAnchor,
  resolveSymbolAnchor,
} from '../../src/tools/lsp/shared/resolveSymbolAnchor.js';

const TOOL_NAME = 'test-tool';

let tempDir: string;
let filePath: string;

beforeEach(async () => {
  tempDir = await mkdtemp(join(process.cwd(), '.tmp-anchor-test-'));
  filePath = join(tempDir, 'fixture.ts');
  await writeFile(
    filePath,
    'export function target() { return 1; }\nexport function target() { return 2; }\n'
  );
});

afterEach(async () => {
  await rm(tempDir, { recursive: true, force: true });
});

describe('resolveFileAnchor', () => {
  it('resolves using uri field', async () => {
    const result = await resolveFileAnchor({ uri: filePath }, TOOL_NAME);
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.value.absolutePath).toBe(filePath);
      expect(result.value.content).toContain('target');
    }
  });

  it('returns ok:false for missing files', async () => {
    const result = await resolveFileAnchor(
      { uri: join(tempDir, 'nonexistent.ts') },
      TOOL_NAME
    );
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.errorType).toBe('file_not_found');
      expect(typeof result.error.error).toBe('string');
    }
  });

  it('returns ok:false for empty path', async () => {
    const result = await resolveFileAnchor({}, TOOL_NAME);
    expect(result.ok).toBe(false);
  });
});

describe('resolveSymbolAnchor', () => {
  it('resolves symbol anchor with uri field', async () => {
    const result = await resolveSymbolAnchor(
      { uri: filePath, type: 'definition', symbolName: 'target', lineHint: 1 },
      TOOL_NAME
    );
    expect(result.ok).toBe(true);
  });

  it('returns ok:false when file resolution fails', async () => {
    const result = await resolveSymbolAnchor(
      {
        uri: join(tempDir, 'missing.ts'),
        type: 'references',
        symbolName: 'target',
        lineHint: 1,
      },
      TOOL_NAME
    );
    expect(result.ok).toBe(false);
  });

  it('returns ok:false with error for documentSymbols type', async () => {
    const result = await resolveSymbolAnchor(
      {
        uri: filePath,
        type: 'documentSymbols',
      },
      TOOL_NAME
    );
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.error).toContain('documentSymbols is file-level');
    }
  });

  it('returns symbol_not_found when symbol is absent', async () => {
    const result = await resolveSymbolAnchor(
      {
        uri: filePath,
        type: 'definition',
        symbolName: 'nonExistentSymbol',
        lineHint: 1,
      },
      TOOL_NAME
    );
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.errorType).toBe('symbol_not_found');
    }
  });

  it('includes orderHint in resolved symbol when provided', async () => {
    const result = await resolveSymbolAnchor(
      {
        uri: filePath,
        type: 'definition',
        symbolName: 'target',
        lineHint: 1,
        orderHint: 1,
      },
      TOOL_NAME
    );
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.value.resolvedSymbol.orderHint).toBe(1);
    }
  });
});
