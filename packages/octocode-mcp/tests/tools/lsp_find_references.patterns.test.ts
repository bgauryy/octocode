/**
 * Branch coverage tests for lsp_find_references/lspReferencesPatterns.ts
 * Targets the massive coverage gap (36% branch → higher)
 *
 * Key branches covered:
 * - spawnCollectOutput: validation failure, buffer limit, exit codes, error
 * - enhancePatternReference: contextLines > 0, catch block
 * - findReferencesWithPatternMatching: filtering, pagination, hints
 * - buildGrepFilterArgsArray: exclude with / vs without /
 * - buildGrepSearchArgs: with exclude-only patterns
 * - isLikelyDefinition edge cases
 * - searchReferencesInWorkspace/searchReferencesWithGrep via findReferencesWithPatternMatching
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import * as path from 'path';

// Mock child_process spawn for spawnCollectOutput
const mockSpawnOn = vi.fn();
const mockStdoutOn = vi.fn();
const mockKill = vi.fn();
const mockSpawnReturn = {
  stdout: { on: mockStdoutOn },
  on: mockSpawnOn,
  kill: mockKill,
};

vi.mock('child_process', () => ({
  spawn: vi.fn(() => mockSpawnReturn),
}));

vi.mock('fs/promises', () => ({
  readFile: vi.fn(),
  stat: vi.fn(),
  access: vi.fn(),
}));

vi.mock('../../src/lsp/validation.js', () => ({
  safeReadFile: vi.fn(),
}));

vi.mock('octocode-security-utils/commandValidator', () => ({
  validateCommand: vi.fn().mockReturnValue({ isValid: true }),
}));

vi.mock('../../src/hints/index.js', () => ({
  getHints: vi.fn().mockReturnValue([]),
}));

import { access } from 'fs/promises';
import { safeReadFile } from '../../src/lsp/validation.js';
import { validateCommand } from 'octocode-security-utils/commandValidator';

import {
  buildGrepFilterArgsArray,
  buildGrepSearchArgs,
  buildRipgrepSearchArgs,
  findWorkspaceRoot,
  isLikelyDefinition,
  findReferencesWithPatternMatching,
} from '../../src/tools/lsp_find_references/lspReferencesPatterns.js';

describe('lspReferencesPatterns - Branch Coverage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(validateCommand).mockReturnValue({ isValid: true });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('buildGrepFilterArgsArray', () => {
    it('should use --exclude-dir for patterns containing /', () => {
      const args = buildGrepFilterArgsArray(undefined, [
        '**/node_modules/**',
        '**/dist/**',
      ]);
      expect(args).toContain('--exclude-dir=node_modules');
      expect(args).toContain('--exclude-dir=dist');
    });

    it('should use --exclude for patterns without /', () => {
      const args = buildGrepFilterArgsArray(undefined, ['*.snap', '*.min.js']);
      expect(args).toContain('--exclude=*.snap');
      expect(args).toContain('--exclude=*.min.js');
    });

    it('should handle mix of include and exclude patterns', () => {
      const args = buildGrepFilterArgsArray(
        ['**/*.ts', '**/*.tsx'],
        ['**/node_modules/**', '*.snap']
      );
      expect(args).toContain('--include=*.ts');
      expect(args).toContain('--include=*.tsx');
      expect(args).toContain('--exclude-dir=node_modules');
      expect(args).toContain('--exclude=*.snap');
    });

    it('should return empty for no patterns', () => {
      expect(buildGrepFilterArgsArray()).toEqual([]);
      expect(buildGrepFilterArgsArray([], [])).toEqual([]);
    });
  });

  describe('buildGrepSearchArgs - exclude only', () => {
    it('should include default extensions when only excludePattern is provided', () => {
      const args = buildGrepSearchArgs('/workspace', 'myFunc', undefined, [
        '**/node_modules/**',
      ]);
      expect(args.some(a => a.startsWith('--include=*.'))).toBe(true);
      expect(args.some(a => a.startsWith('--exclude-dir='))).toBe(true);
    });

    it('should not add default extensions when includePattern is provided', () => {
      const args = buildGrepSearchArgs(
        '/workspace',
        'myFunc',
        ['**/*.py'],
        ['**/node_modules/**']
      );
      expect(args).toContain('--include=*.py');
      const defaultExtArgs = args.filter(
        a => a.startsWith('--include=*.') && a !== '--include=*.py'
      );
      expect(defaultExtArgs.length).toBe(0);
    });
  });

  describe('findWorkspaceRoot', () => {
    it('should find workspace root when marker exists', async () => {
      vi.mocked(access).mockResolvedValue(undefined);

      const result = await findWorkspaceRoot('/some/project/src/file.ts');
      expect(typeof result).toBe('string');
    });

    it('should traverse up and return dirname when no marker found', async () => {
      vi.mocked(access).mockRejectedValue(new Error('ENOENT'));

      const result = await findWorkspaceRoot('/a/b/c/d/e/f/g/h/i/j/k/file.ts');
      expect(result).toBe(path.dirname('/a/b/c/d/e/f/g/h/i/j/k/file.ts'));
    });

    it('should stop when parentDir equals currentDir (root)', async () => {
      vi.mocked(access).mockRejectedValue(new Error('ENOENT'));

      const result = await findWorkspaceRoot('/file.ts');
      expect(result).toBeDefined();
    });
  });

  describe('findReferencesWithPatternMatching', () => {
    const setupSpawnSuccess = (stdout: string) => {
      mockStdoutOn.mockImplementation(
        (event: string, cb: (data: Buffer) => void) => {
          if (event === 'data') {
            cb(Buffer.from(stdout));
          }
        }
      );
      mockSpawnOn.mockImplementation(
        (event: string, cb: (code: number) => void) => {
          if (event === 'close') {
            setTimeout(() => cb(0), 0);
          }
        }
      );
    };

    it('should return empty when no references found', async () => {
      setupSpawnSuccess('');

      const result = await findReferencesWithPatternMatching(
        '/workspace/src/file.ts',
        '/workspace',
        {
          id: 'pattern_query_1',
          uri: '/workspace/src/file.ts',
          symbolName: 'nonExistentSymbol',
          lineHint: 1,
          researchGoal: 'test',
          reasoning: 'test',
          page: 1,
          contextLines: 0,
          orderHint: 0,
          includeDeclaration: true,
          referencesPerPage: 20,
        }
      );

      expect(result.status).toBe('empty');
    });

    it('should return results when ripgrep finds matches', async () => {
      const rgOutput = [
        JSON.stringify({
          type: 'match',
          data: {
            path: { text: '/workspace/src/file.ts' },
            line_number: 5,
            lines: { text: 'export function myFunc() {}\n' },
          },
        }),
        JSON.stringify({
          type: 'match',
          data: {
            path: { text: '/workspace/src/other.ts' },
            line_number: 10,
            lines: { text: 'const result = myFunc();\n' },
          },
        }),
      ].join('\n');

      setupSpawnSuccess(rgOutput);

      const result = await findReferencesWithPatternMatching(
        '/workspace/src/file.ts',
        '/workspace',
        {
          id: 'pattern_query_2',
          uri: '/workspace/src/file.ts',
          symbolName: 'myFunc',
          lineHint: 5,
          includeDeclaration: true,
          referencesPerPage: 20,
          page: 1,
          contextLines: 0,
          orderHint: 0,
          researchGoal: 'test',
          reasoning: 'test',
        }
      );

      expect(result.status).toBe('hasResults');
      expect(result.locations!.length).toBe(2);
    });

    it('should return empty with pagination when page exceeds total pages', async () => {
      const rgOutput = [
        JSON.stringify({
          type: 'match',
          data: {
            path: { text: '/workspace/src/file.ts' },
            line_number: 5,
            lines: { text: 'const testFunc = 1;\n' },
          },
        }),
        JSON.stringify({
          type: 'match',
          data: {
            path: { text: '/workspace/src/file.ts' },
            line_number: 9,
            lines: { text: 'console.log(testFunc);\n' },
          },
        }),
      ].join('\n');

      setupSpawnSuccess(rgOutput);

      const result = await findReferencesWithPatternMatching(
        '/workspace/src/file.ts',
        '/workspace',
        {
          id: 'pattern_query_out_of_range',
          uri: '/workspace/src/file.ts',
          symbolName: 'testFunc',
          lineHint: 5,
          includeDeclaration: true,
          referencesPerPage: 1,
          page: 10,
          contextLines: 0,
          orderHint: 0,
          researchGoal: 'test',
          reasoning: 'test',
        }
      );

      expect(result.status).toBe('empty');
      expect(result.pagination).toEqual({
        currentPage: 10,
        totalPages: 2,
        totalResults: 2,
        hasMore: false,
        resultsPerPage: 1,
      });
      expect(
        result.hints!.some(h => h.includes('outside available range'))
      ).toBe(true);
    });

    it('should preserve hasMultipleFiles when pagination reduces the page to one file', async () => {
      const rgOutput = [
        JSON.stringify({
          type: 'match',
          data: {
            path: { text: '/workspace/src/consumer.ts' },
            line_number: 4,
            lines: { text: 'const score = computeScore(input);\n' },
          },
        }),
        JSON.stringify({
          type: 'match',
          data: {
            path: { text: '/workspace/src/secondary.ts' },
            line_number: 7,
            lines: {
              text: 'return computeScore(left) - computeScore(right);\n',
            },
          },
        }),
        JSON.stringify({
          type: 'match',
          data: {
            path: { text: '/workspace/src/score.ts' },
            line_number: 10,
            lines: {
              text: 'export function computeScore(input: ScoreInput): number {\n',
            },
          },
        }),
      ].join('\n');

      setupSpawnSuccess(rgOutput);

      const result = await findReferencesWithPatternMatching(
        '/workspace/src/score.ts',
        '/workspace',
        {
          id: 'pattern_query_paginated_multi_file',
          uri: '/workspace/src/score.ts',
          symbolName: 'computeScore',
          lineHint: 10,
          includeDeclaration: false,
          referencesPerPage: 1,
          page: 1,
          contextLines: 0,
          orderHint: 0,
          researchGoal: 'test',
          reasoning: 'test',
        }
      );

      expect(result.status).toBe('hasResults');
      expect(result.locations).toHaveLength(1);
      expect(result.hasMultipleFiles).toBe(true);
    });

    it('should filter by includeDeclaration=false', async () => {
      const rgOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/workspace/src/file.ts' },
          line_number: 5,
          lines: { text: 'export function myFunc() {}\n' },
        },
      });

      setupSpawnSuccess(rgOutput);

      const result = await findReferencesWithPatternMatching(
        '/workspace/src/file.ts',
        '/workspace',
        {
          id: 'pattern_query_3',
          uri: '/workspace/src/file.ts',
          symbolName: 'myFunc',
          lineHint: 5,
          includeDeclaration: false,
          contextLines: 0,
          orderHint: 0,
          page: 1,
          referencesPerPage: 20,
          researchGoal: 'test',
          reasoning: 'test',
        }
      );

      // The definition match should be filtered out
      expect(result.status).toBe('empty');
    });

    it('should add filter hint when patterns reduce results', async () => {
      const rgOutput = [
        JSON.stringify({
          type: 'match',
          data: {
            path: { text: '/workspace/src/a.ts' },
            line_number: 1,
            lines: { text: 'const x = myFunc();\n' },
          },
        }),
        JSON.stringify({
          type: 'match',
          data: {
            path: { text: '/workspace/node_modules/pkg/b.ts' },
            line_number: 1,
            lines: { text: 'const y = myFunc();\n' },
          },
        }),
      ].join('\n');

      setupSpawnSuccess(rgOutput);

      const result = await findReferencesWithPatternMatching(
        '/workspace/src/file.ts',
        '/workspace',
        {
          id: 'pattern_query_4',
          uri: '/workspace/src/file.ts',
          symbolName: 'myFunc',
          lineHint: 1,
          includeDeclaration: true,
          excludePattern: ['**/node_modules/**'],
          contextLines: 0,
          orderHint: 0,
          page: 1,
          referencesPerPage: 20,
          researchGoal: 'test',
          reasoning: 'test',
        }
      );

      expect(result.status).toBe('hasResults');
      expect(result.hints!.some(h => h.includes('Filtered:'))).toBe(true);
    });

    it('should add "none matched patterns" hint when all filtered out', async () => {
      const rgOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/workspace/node_modules/pkg/index.ts' },
          line_number: 1,
          lines: { text: 'const z = myFunc();\n' },
        },
      });

      setupSpawnSuccess(rgOutput);

      const result = await findReferencesWithPatternMatching(
        '/workspace/src/file.ts',
        '/workspace',
        {
          id: 'pattern_query_5',
          uri: '/workspace/src/file.ts',
          symbolName: 'myFunc',
          lineHint: 1,
          includeDeclaration: true,
          excludePattern: ['**/node_modules/**'],
          contextLines: 0,
          orderHint: 0,
          page: 1,
          referencesPerPage: 20,
          researchGoal: 'test',
          reasoning: 'test',
        }
      );

      expect(result.status).toBe('empty');
      expect(
        result.hints!.some(h => h.includes('none matched the file patterns'))
      ).toBe(true);
    });

    it('should paginate results and add hasMore hint', async () => {
      const matches = Array.from({ length: 25 }, (_, i) =>
        JSON.stringify({
          type: 'match',
          data: {
            path: { text: `/workspace/src/file${i}.ts` },
            line_number: 1,
            lines: { text: `const v${i} = myFunc();\n` },
          },
        })
      ).join('\n');

      setupSpawnSuccess(matches);

      const result = await findReferencesWithPatternMatching(
        '/workspace/src/source.ts',
        '/workspace',
        {
          id: 'pattern_query_6',
          uri: '/workspace/src/source.ts',
          symbolName: 'myFunc',
          lineHint: 1,
          includeDeclaration: true,
          referencesPerPage: 10,
          page: 1,
          contextLines: 0,
          orderHint: 0,
          researchGoal: 'test',
          reasoning: 'test',
        }
      );

      expect(result.status).toBe('hasResults');
      expect(result.locations!.length).toBe(10);
      expect(result.hints!.some(h => h.includes('page'))).toBe(true);
    });

    it('should enhance references with context when contextLines > 0', async () => {
      const rgOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/workspace/src/a.ts' },
          line_number: 3,
          lines: { text: 'const x = myFunc();\n' },
        },
      });

      setupSpawnSuccess(rgOutput);

      vi.mocked(safeReadFile).mockResolvedValue(
        'line1\nline2\nconst x = myFunc();\nline4\nline5'
      );

      const result = await findReferencesWithPatternMatching(
        '/workspace/src/file.ts',
        '/workspace',
        {
          id: 'pattern_query_7',
          uri: '/workspace/src/file.ts',
          symbolName: 'myFunc',
          lineHint: 3,
          includeDeclaration: true,
          contextLines: 2,
          orderHint: 0,
          page: 1,
          referencesPerPage: 20,
          researchGoal: 'test',
          reasoning: 'test',
        }
      );

      expect(result.status).toBe('hasResults');
      const content = result.locations![0]!.content;
      expect(content).toContain('line1');
    });

    it('should keep single-line content when safeReadFile fails', async () => {
      const rgOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/workspace/src/a.ts' },
          line_number: 3,
          lines: { text: 'const x = myFunc();\n' },
        },
      });

      setupSpawnSuccess(rgOutput);

      vi.mocked(safeReadFile).mockResolvedValue(null);

      const result = await findReferencesWithPatternMatching(
        '/workspace/src/file.ts',
        '/workspace',
        {
          id: 'pattern_query_8',
          uri: '/workspace/src/file.ts',
          symbolName: 'myFunc',
          lineHint: 3,
          includeDeclaration: true,
          contextLines: 2,
          orderHint: 0,
          page: 1,
          referencesPerPage: 20,
          researchGoal: 'test',
          reasoning: 'test',
        }
      );

      expect(result.status).toBe('hasResults');
      expect(result.locations![0]!.content).toBe('const x = myFunc();');
    });

    it('should fallback to grep when ripgrep fails with non-1 exit code and skip malformed grep lines', async () => {
      let spawnCallCount = 0;
      mockStdoutOn.mockImplementation(
        (event: string, cb: (data: Buffer) => void) => {
          if (event === 'data' && spawnCallCount > 0) {
            // Grep output: valid line + malformed lines (no second colon, no colon)
            cb(
              Buffer.from(
                [
                  '/workspace/src/valid.ts:5:const x = myFunc();',
                  'malformed-no-colon',
                  'path:onlyonecolon',
                ].join('\n')
              )
            );
          }
        }
      );
      mockSpawnOn.mockImplementation(
        (event: string, cb: (code: number) => void) => {
          if (event === 'close') {
            spawnCallCount++;
            setTimeout(() => cb(spawnCallCount === 1 ? 2 : 0), 0);
          }
        }
      );

      const result = await findReferencesWithPatternMatching(
        '/workspace/src/file.ts',
        '/workspace',
        {
          id: 'pattern_query_grep',
          uri: '/workspace/src/file.ts',
          symbolName: 'myFunc',
          lineHint: 5,
          includeDeclaration: true,
          contextLines: 0,
          orderHint: 0,
          page: 1,
          referencesPerPage: 20,
          researchGoal: 'test',
          reasoning: 'test',
        }
      );

      expect(result.status).toBe('hasResults');
      expect(result.locations!.length).toBe(1);
      expect(result.locations![0]!.uri).toContain('valid');
    });

    it('should return empty when both ripgrep and grep fail (grep catch block)', async () => {
      let spawnCallCount = 0;
      mockSpawnOn.mockImplementation(
        (event: string, cb: (code: number) => void) => {
          if (event === 'close') {
            spawnCallCount++;
            setTimeout(() => cb(2), 0);
          }
        }
      );
      mockStdoutOn.mockImplementation(() => {});

      const result = await findReferencesWithPatternMatching(
        '/workspace/src/file.ts',
        '/workspace',
        {
          id: 'pattern_query_both_fail',
          uri: '/workspace/src/file.ts',
          symbolName: 'myFunc',
          lineHint: 5,
          includeDeclaration: true,
          contextLines: 0,
          orderHint: 0,
          page: 1,
          referencesPerPage: 20,
          researchGoal: 'test',
          reasoning: 'test',
        }
      );

      expect(result.status).toBe('empty');
      expect(result.locations ?? []).toEqual([]);
    });

    it('should use includePattern and excludePattern in search (buildRipgrepSearchArgs)', async () => {
      const rgOutput = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/workspace/src/included.ts' },
          line_number: 1,
          lines: { text: 'myFunc();\n' },
        },
      });
      setupSpawnSuccess(rgOutput);

      const result = await findReferencesWithPatternMatching(
        '/workspace/src/file.ts',
        '/workspace',
        {
          id: 'pattern_query_glob',
          uri: '/workspace/src/file.ts',
          symbolName: 'myFunc',
          lineHint: 1,
          includePattern: ['**/*.ts'],
          excludePattern: ['**/node_modules/**'],
          includeDeclaration: true,
          contextLines: 0,
          orderHint: 0,
          page: 1,
          referencesPerPage: 20,
          researchGoal: 'test',
          reasoning: 'test',
        }
      );

      expect(result.status).toBe('hasResults');
    });

    it('should sort refs: definition first, then by uri (sort comparisons)', async () => {
      const rgOutput = [
        JSON.stringify({
          type: 'match',
          data: {
            path: { text: '/workspace/src/b.ts' },
            line_number: 1,
            lines: { text: 'myFunc();\n' },
          },
        }),
        JSON.stringify({
          type: 'match',
          data: {
            path: { text: '/workspace/src/file.ts' },
            line_number: 5,
            lines: { text: 'export function myFunc() {}\n' },
          },
        }),
        JSON.stringify({
          type: 'match',
          data: {
            path: { text: '/workspace/src/a.ts' },
            line_number: 1,
            lines: { text: 'myFunc();\n' },
          },
        }),
      ].join('\n');
      setupSpawnSuccess(rgOutput);

      const result = await findReferencesWithPatternMatching(
        '/workspace/src/file.ts',
        '/workspace',
        {
          id: 'pattern_query_sort',
          uri: '/workspace/src/file.ts',
          symbolName: 'myFunc',
          lineHint: 5,
          includeDeclaration: true,
          contextLines: 0,
          orderHint: 0,
          page: 1,
          referencesPerPage: 20,
          researchGoal: 'test',
          reasoning: 'test',
        }
      );

      expect(result.status).toBe('hasResults');
      expect(result.locations!.length).toBe(3);
      expect(result.locations![0]!.isDefinition).toBe(true);
      expect(result.locations![1]!.uri).toBeDefined();
      expect(result.locations![2]!.uri).toBeDefined();
    });
  });

  describe('Cross-language extension coverage (regression)', () => {
    // Regression: ripgrep/grep fallbacks only matched ts/tsx/js/jsx/py/go/rs/java/c/cpp/h
    // so Kotlin/Swift/Dart/Ruby/PHP/C#/Scala/Lua users got zero references.
    it('buildRipgrepSearchArgs registers a code type covering Kotlin/Swift/Dart/Ruby/PHP/C#/Scala/Lua', () => {
      const args = buildRipgrepSearchArgs('/ws', 'foo');
      const typeAddIndex = args.indexOf('--type-add');
      expect(typeAddIndex).toBeGreaterThanOrEqual(0);
      const typeDef = args[typeAddIndex + 1]!;
      for (const ext of [
        'kt',
        'kts',
        'swift',
        'dart',
        'rb',
        'php',
        'cs',
        'scala',
        'lua',
        'mjs',
        'cjs',
        'pyi',
        'cc',
        'cxx',
        'hpp',
      ]) {
        expect(typeDef).toContain(ext);
      }
    });

    it('buildGrepSearchArgs --include matches the same extension set when no patterns provided', () => {
      const args = buildGrepSearchArgs('/ws', 'foo');
      const includes = args.filter(a => a.startsWith('--include='));
      const exts = includes.map(a => a.replace('--include=*.', ''));
      for (const ext of [
        'kt',
        'kts',
        'swift',
        'dart',
        'rb',
        'php',
        'cs',
        'scala',
        'lua',
      ]) {
        expect(exts).toContain(ext);
      }
    });
  });

  describe('isLikelyDefinition - additional patterns', () => {
    it('should detect pub Rust definitions', () => {
      expect(isLikelyDefinition('pub fn create() {', 'create')).toBe(true);
      expect(isLikelyDefinition('pub struct Config {', 'Config')).toBe(true);
      expect(isLikelyDefinition('pub enum Status {', 'Status')).toBe(true);
      expect(isLikelyDefinition('pub trait Handler {', 'Handler')).toBe(true);
      expect(isLikelyDefinition('pub type Result = Vec<u8>;', 'Result')).toBe(
        true
      );
      expect(isLikelyDefinition('pub const MAX: u32 = 100;', 'MAX')).toBe(true);
      expect(
        isLikelyDefinition('pub static INSTANCE: u32 = 1;', 'INSTANCE')
      ).toBe(true);
    });

    it('should detect Go method with receiver', () => {
      expect(isLikelyDefinition('func (s *Server) Handle() {', 'Handle')).toBe(
        true
      );
    });
  });
});
