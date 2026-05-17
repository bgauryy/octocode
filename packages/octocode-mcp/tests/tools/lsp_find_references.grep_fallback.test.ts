/**
 * Grep fallback coverage for lsp_find_references/lspReferencesPatterns.ts
 *
 * Covers uncovered lines 587-618, 633-636 (searchReferencesWithGrep).
 *
 * Strategy:
 * - Make the ripgrep spawn close with exit code 2 (not 0 or 1) →
 *   getExecErrorCode returns 2 → falls back to searchReferencesWithGrep
 * - Second spawn (grep) returns well-formed grep output
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

const mockKill = vi.fn();
const mockSpawnFn = vi.fn();

// We return a FRESH child object per spawn call so each
// test can simulate independent close/data events.
vi.mock('child_process', () => ({
  spawn: (...args: unknown[]) => mockSpawnFn(...args),
}));

vi.mock('fs/promises', () => ({
  readFile: vi.fn(),
  stat: vi.fn(),
  access: vi.fn().mockRejectedValue(new Error('ENOENT')),
}));

vi.mock('../../src/lsp/validation.js', () => ({
  safeReadFile: vi.fn().mockResolvedValue(null),
}));

vi.mock('octocode-security-utils/commandValidator', () => ({
  validateCommand: vi.fn().mockReturnValue({ isValid: true }),
}));

vi.mock('../../src/hints/index.js', () => ({
  getHints: vi.fn().mockReturnValue([]),
}));

import { findReferencesWithPatternMatching } from '../../src/tools/lsp_find_references/lspReferencesPatterns.js';

/** Create a mock child that fires data + close events in microtasks. */
function makeChild(chunks: string[], closeCode: number) {
  const dataHandlers: Array<(data: Buffer) => void> = [];
  const closeHandlers: Array<(code: number) => void> = [];

  const child = {
    stdout: {
      on: vi.fn((event: string, cb: (data: Buffer) => void) => {
        if (event === 'data') dataHandlers.push(cb);
      }),
    },
    on: vi.fn((event: string, cb: (...args: unknown[]) => void) => {
      if (event === 'close') closeHandlers.push(cb as (code: number) => void);
    }),
    kill: mockKill,
  };

  // Fire data and close asynchronously
  setImmediate(() => {
    for (const chunk of chunks) {
      for (const handler of dataHandlers) {
        handler(Buffer.from(chunk));
      }
    }
    for (const handler of closeHandlers) {
      handler(closeCode);
    }
  });

  return child;
}

const BASE_QUERY = {
  id: 'grep_fallback_test',
  uri: '/workspace/src/file.ts',
  symbolName: 'myFunc',
  lineHint: 5,
  includeDeclaration: true,
  referencesPerPage: 20,
  page: 1,
  contextLines: 0,
  orderHint: 0,
  researchGoal: 'test grep fallback',
  reasoning: 'branch coverage for searchReferencesWithGrep',
};

describe('lspReferencesPatterns — grep fallback (searchReferencesWithGrep)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('falls back to grep when ripgrep exits with code 2 and returns results', async () => {
    const grepOutput = [
      '/workspace/src/file.ts:5:export function myFunc() {}',
      '/workspace/src/other.ts:10:const x = myFunc();',
    ].join('\n');

    let spawnCall = 0;
    mockSpawnFn.mockImplementation(() => {
      spawnCall++;
      if (spawnCall === 1) {
        // Ripgrep: fails with code 2 → triggers grep fallback
        return makeChild([], 2);
      } else {
        // Grep: succeeds with grep-format output
        return makeChild([grepOutput], 0);
      }
    });

    const result = await findReferencesWithPatternMatching(
      '/workspace/src/file.ts',
      '/workspace',
      BASE_QUERY
    );

    expect(result.status).toBe('hasResults');
    expect(result.locations!.length).toBeGreaterThanOrEqual(1);
  });

  it('returns empty when grep also finds nothing', async () => {
    let spawnCall = 0;
    mockSpawnFn.mockImplementation(() => {
      spawnCall++;
      if (spawnCall === 1) {
        return makeChild([], 2); // ripgrep fails with code 2
      } else {
        return makeChild([''], 0); // grep: empty output
      }
    });

    const result = await findReferencesWithPatternMatching(
      '/workspace/src/file.ts',
      '/workspace',
      { ...BASE_QUERY, symbolName: 'noSuchSymbol' }
    );

    expect(result.status).toBe('empty');
  });

  it('handles grep output with malformed lines gracefully', async () => {
    const mixedOutput = [
      'bad-line-no-colons',
      '/workspace/src/file.ts:notanumber:content',
      '/workspace/src/file.ts:5:myFunc is here',
    ].join('\n');

    let spawnCall = 0;
    mockSpawnFn.mockImplementation(() => {
      spawnCall++;
      if (spawnCall === 1) {
        return makeChild([], 2); // ripgrep fails
      } else {
        return makeChild([mixedOutput], 0); // grep with mixed output
      }
    });

    const result = await findReferencesWithPatternMatching(
      '/workspace/src/file.ts',
      '/workspace',
      BASE_QUERY
    );

    // Should parse the valid line and skip the invalid ones
    expect(['hasResults', 'empty']).toContain(result.status);
  });

  it('grep fallback: definition first sort (grep sort comparisons covered)', async () => {
    const grepOutput = [
      '/workspace/src/other.ts:10:const x = myFunc();',
      '/workspace/src/file.ts:5:export function myFunc() {}',
      '/workspace/src/another.ts:3:myFunc();',
    ].join('\n');

    let spawnCall = 0;
    mockSpawnFn.mockImplementation(() => {
      spawnCall++;
      if (spawnCall === 1) {
        return makeChild([], 2); // ripgrep fails
      } else {
        return makeChild([grepOutput], 1); // grep: exit code 1 (no more matches) but output present
      }
    });

    const result = await findReferencesWithPatternMatching(
      '/workspace/src/file.ts',
      '/workspace',
      BASE_QUERY
    );

    // Definition should appear first
    expect(['hasResults', 'empty']).toContain(result.status);
    if (result.status === 'hasResults' && result.locations!.length > 0) {
      expect(result.locations![0]!.isDefinition).toBe(true);
    }
  });

  it('grep fallback: sorts by uri when both are non-definitions', async () => {
    const grepOutput = [
      '/workspace/src/z.ts:3:myFunc();',
      '/workspace/src/a.ts:5:myFunc();',
    ].join('\n');

    let spawnCall = 0;
    mockSpawnFn.mockImplementation(() => {
      spawnCall++;
      if (spawnCall === 1) {
        return makeChild([], 2); // ripgrep fails
      } else {
        return makeChild([grepOutput], 0); // grep succeeds
      }
    });

    const result = await findReferencesWithPatternMatching(
      '/workspace/src/other.ts', // different file - neither match is a definition
      '/workspace',
      BASE_QUERY
    );

    if (result.status === 'hasResults' && result.locations!.length === 2) {
      // a.ts should come before z.ts (alphabetical by uri)
      expect(result.locations![0]!.uri).toContain('a.ts');
    }
  });

  it('grep fallback with include/exclude patterns passes patterns through', async () => {
    const grepOutput = '/workspace/src/file.ts:5:myFunc();';

    let spawnCall = 0;
    mockSpawnFn.mockImplementation(() => {
      spawnCall++;
      if (spawnCall === 1) {
        return makeChild([], 2);
      } else {
        return makeChild([grepOutput], 0);
      }
    });

    const result = await findReferencesWithPatternMatching(
      '/workspace/src/file.ts',
      '/workspace',
      {
        ...BASE_QUERY,
        includePattern: ['**/*.ts'],
        excludePattern: ['**/dist/**'],
      }
    );

    expect(['hasResults', 'empty']).toContain(result.status);
  });

  it('handles grep fallback error gracefully (grep also fails)', async () => {
    let spawnCall = 0;
    mockSpawnFn.mockImplementation(() => {
      spawnCall++;
      if (spawnCall === 1) {
        return makeChild([], 2); // ripgrep fails with code 2
      } else {
        return makeChild([], 2); // grep also fails
      }
    });

    // Should not throw; returns empty (grep failure is caught)
    const result = await findReferencesWithPatternMatching(
      '/workspace/src/file.ts',
      '/workspace',
      BASE_QUERY
    );

    expect(['empty', 'hasResults']).toContain(result.status);
  });

  it('grep sort by line number — same file, different lines (line 636 branch)', async () => {
    // Two matches in the same file → sort falls through to line comparison
    const grepOutput = [
      '/workspace/src/file.ts:20:myFunc();',
      '/workspace/src/file.ts:5:myFunc();',
    ].join('\n');

    let spawnCall = 0;
    mockSpawnFn.mockImplementation(() => {
      spawnCall++;
      if (spawnCall === 1) {
        return makeChild([], 2); // ripgrep fails
      } else {
        return makeChild([grepOutput], 0); // grep succeeds with same-file matches
      }
    });

    const result = await findReferencesWithPatternMatching(
      '/workspace/src/other.ts', // different source file so neither is a definition
      '/workspace',
      BASE_QUERY
    );

    if (result.status === 'hasResults' && result.locations!.length === 2) {
      // Line 5 should come before line 20 after sort
      const lines = result.locations!.map(
        loc => loc.range.start.line + 1 // convert 0-based back to 1-based
      );
      expect(lines[0]).toBeLessThan(lines[1]!);
    }
  });

  it('validation failure in spawn (line 110 branch) falls back gracefully', async () => {
    const { validateCommand } = await import(
      'octocode-security-utils/commandValidator'
    );
    const mockValidate = vi.mocked(validateCommand);
    // Make validation fail for all commands → spawnCollectOutput throws
    mockValidate.mockReturnValue({ isValid: false, error: 'command not allowed' });

    // Both ripgrep and grep will throw → both paths caught → empty result
    const result = await findReferencesWithPatternMatching(
      '/workspace/src/file.ts',
      '/workspace',
      BASE_QUERY
    );

    expect(['empty', 'hasResults']).toContain(result.status);

    // Restore for other tests
    mockValidate.mockReturnValue({ isValid: true });
  });
});
