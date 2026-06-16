import { describe, it, expect, vi, afterEach } from 'vitest';

import {
  compactResolvedSymbol,
  compactLocation,
} from '../../octocode-tools-core/src/tools/lsp/shared/semanticTypes.js';

describe('semanticTypes — compactResolvedSymbol branch coverage', () => {
  it('omits orderHint when undefined (falsy spread branch at line 77)', () => {
    const result = compactResolvedSymbol({
      name: 'myFn',
      uri: 'file:///src/a.ts',
      range: {
        start: { line: 0, character: 0 },
        end: { line: 0, character: 4 },
      },
      foundAtLine: 1,
      orderHint: undefined,
      position: { line: 0, character: 0 },
    });
    expect(result).toEqual({
      name: 'myFn',
      uri: 'file:///src/a.ts',
      foundAtLine: 1,
    });
    expect('orderHint' in result).toBe(false);
  });

  it('includes orderHint when defined (truthy spread branch)', () => {
    const result = compactResolvedSymbol({
      name: 'myFn',
      uri: 'file:///src/a.ts',
      range: {
        start: { line: 0, character: 0 },
        end: { line: 0, character: 4 },
      },
      foundAtLine: 1,
      orderHint: 2,
      position: { line: 0, character: 0 },
    });
    expect(result.orderHint).toBe(2);
  });
});

describe('semanticTypes — compactLocation branch coverage', () => {
  it('omits content when undefined (line 99 false-branch)', () => {
    const result = compactLocation({ uri: 'file:///src/b.ts' });
    expect('content' in result).toBe(false);
  });

  it('includes content when defined (line 99 true-branch)', () => {
    const result = compactLocation({
      uri: 'file:///src/b.ts',
      content: 'hello',
    });
    expect(result.content).toBe('hello');
  });

  it('omits displayRange when falsy (line 99 false-branch for displayRange)', () => {
    const result = compactLocation({
      uri: 'file:///src/b.ts',
      displayRange: undefined,
    });
    expect('displayRange' in result).toBe(false);
  });

  it('includes displayRange when provided (line 99 true-branch for displayRange)', () => {
    const result = compactLocation({
      uri: 'file:///src/b.ts',
      displayRange: { startLine: 1, endLine: 5 },
    });
    expect(result.displayRange).toEqual({ startLine: 1, endLine: 5 });
  });

  it('omits isDefinition when falsy (line 100 false-branch)', () => {
    const result = compactLocation({
      uri: 'file:///src/b.ts',
      isDefinition: false,
    });
    expect('isDefinition' in result).toBe(false);
  });

  it('includes isDefinition: true when provided (line 100 true-branch)', () => {
    const result = compactLocation({
      uri: 'file:///src/b.ts',
      isDefinition: true,
    });
    expect(result.isDefinition).toBe(true);
  });
});

vi.mock(
  '../../octocode-tools-core/src/hints/dynamic.js',
  async importOriginal => {
    return importOriginal();
  }
);

describe('hints/dynamic — getDynamicHints with explicit context (line 46)', () => {
  it('passes provided context through to the hint generator', async () => {
    const { getDynamicHints } =
      await import('../../octocode-tools-core/src/hints/dynamic.js');

    const hints = getDynamicHints('localSearchCode', 'hasResults', {
      hasResults: true,
      resultCount: 3,
    });
    expect(Array.isArray(hints)).toBe(true);
  });

  it('returns [] for an unknown toolName regardless of context', async () => {
    const { getDynamicHints } =
      await import('../../octocode-tools-core/src/hints/dynamic.js');
    const hints = getDynamicHints('nonExistentTool_xyz', 'hasResults', {
      hasResults: true,
    });
    expect(hints).toEqual([]);
  });
});

describe('toolMetadata/gateway — getDescription unknown tool (line 19)', () => {
  it('returns empty string for a tool not in DESCRIPTIONS', async () => {
    vi.resetModules();
    const { DEFAULT_TOOL_METADATA_GATEWAY } =
      await import('../../octocode-tools-core/src/tools/toolMetadata/gateway.js');
    const desc = DEFAULT_TOOL_METADATA_GATEWAY.getDescription(
      '__completely_unknown__'
    );
    expect(desc).toBe('');
  });

  it('hasTool returns false for unknown tool', async () => {
    vi.resetModules();
    const { DEFAULT_TOOL_METADATA_GATEWAY } =
      await import('../../octocode-tools-core/src/tools/toolMetadata/gateway.js');
    expect(
      DEFAULT_TOOL_METADATA_GATEWAY.hasTool('__completely_unknown__')
    ).toBe(false);
  });
});
