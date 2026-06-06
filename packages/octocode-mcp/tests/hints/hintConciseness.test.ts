import { describe, it, expect, vi, beforeAll } from 'vitest';
import { executeBulkOperation } from '../../src/utils/response/bulk.js';
import { createErrorResult } from '../../src/utils/response/error.js';
import { buildSearchResult } from '../../src/tools/local_ripgrep/ripgrepResultBuilder.js';
import { validateToolPath } from '../../src/utils/file/toolHelpers.js';
import { TOOL_NAMES } from '../../src/tools/toolMetadata/proxies.js';
import { initializeToolMetadata } from '../../src/tools/toolMetadata/state.js';
import { getTextContent } from '../utils/testHelpers.js';

beforeAll(async () => {
  await initializeToolMetadata();
});

describe('Hint conciseness', () => {
  describe('empty string filtering in bulk responses', () => {
    it('should strip empty strings from hints in hasResults result', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockResolvedValue({
        data: { foo: 1 },
        hints: ['Valid hint', '', '  ', 'Another valid hint'],
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const text = getTextContent(result.content);
      expect(text).toContain('Valid hint');
      expect(text).toContain('Another valid hint');
      expect(text).not.toMatch(/- ""\n/);
      expect(text).not.toMatch(/- " {2}"\n/);
    });

    it('should strip empty strings from hints in empty result', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockResolvedValue({
        status: 'empty' as const,
        hints: ['', 'Useful empty hint', ''],
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const text = getTextContent(result.content);
      expect(text).toContain('Useful empty hint');
      expect(text).not.toMatch(/- ""\n/);
    });

    it('should strip empty strings from hints in error result', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockResolvedValue({
        status: 'error' as const,
        error: 'Something failed',
        hints: ['', 'Error recovery hint', '', '   '],
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const text = getTextContent(result.content);
      expect(text).toContain('Error recovery hint');
      expect(text).not.toMatch(/- ""\n/);
      expect(text).not.toMatch(/- " {3}"\n/);
    });
  });

  describe('empty string filtering in createErrorResult', () => {
    it('should not include empty strings in error hints', () => {
      const result = createErrorResult(
        'Test error',
        {
          researchGoal: 'test',
          reasoning: 'test',
        },
        {
          customHints: ['', 'Actionable hint', '  ', 'Another hint', ''],
        }
      );

      expect(result.hints).toBeDefined();
      expect(result.hints).toContain('Actionable hint');
      expect(result.hints).toContain('Another hint');
      expect(result.hints).not.toContain('');
      expect(result.hints).not.toContain('  ');
    });

    it('should return no hints field when all hints are empty strings', () => {
      const result = createErrorResult(
        'Test error',
        {
          researchGoal: 'test',
          reasoning: 'test',
        },
        {
          customHints: ['', '  ', ''],
        }
      );

      expect(result.hints).toBeUndefined();
    });
  });

  describe('Integration block in ripgrep results', () => {
    const makeFiles = (count: number) =>
      Array.from({ length: count }, (_, i) => ({
        path: `/test/file${i}.ts`,
        matchCount: 1,
        matches: [
          {
            line: i + 1,
            column: 1,
            value: 'match',
            location: {
              byteOffset: 0,
              byteLength: 5,
              charOffset: 0,
              charLength: 5,
              line: i + 1,
              column: 1,
            },
          },
        ],
      }));

    it('should NOT include Integration hint in discovery mode', async () => {
      const result = await buildSearchResult(
        makeFiles(3),
        {
          path: '/test',
          pattern: 'export',
          mode: 'discovery',
          researchGoal: 'test',
          reasoning: 'test',
        } as any,
        'rg',
        []
      );

      const allHints = (result.hints || []).join('\n');
      expect(allHints).not.toContain('Integration:');
      expect(allHints).not.toContain('byteOffset');
    });

    it('should NOT include Integration hint in paginated mode', async () => {
      const result = await buildSearchResult(
        makeFiles(3),
        {
          path: '/test',
          pattern: 'export',
          mode: 'paginated',
          researchGoal: 'test',
          reasoning: 'test',
        } as any,
        'rg',
        []
      );

      const allHints = (result.hints || []).join('\n');
      expect(allHints).not.toContain('Integration:');
      expect(allHints).not.toContain('byteOffset');
    });

    it('should NOT include Integration hint when mode is undefined', async () => {
      const result = await buildSearchResult(
        makeFiles(3),
        {
          path: '/test',
          pattern: 'export',
          researchGoal: 'test',
          reasoning: 'test',
        } as any,
        'rg',
        []
      );

      const allHints = (result.hints || []).join('\n');
      expect(allHints).not.toContain('Integration:');
      expect(allHints).not.toContain('byteOffset');
    });
  });

  describe('path error hint conciseness', () => {
    it('should produce at most 2 hints for path outside allowed dirs', () => {
      const result = validateToolPath(
        { path: '/etc/passwd', researchGoal: 'test', reasoning: 'test' },
        'LOCAL_FETCH_CONTENT'
      );

      expect(result.isValid).toBe(false);
      const hints = result.errorResult?.hints as string[];
      const pathHints = hints.filter(
        h => h.includes('CWD:') || h.includes('Fix:')
      );
      expect(pathHints.length).toBeLessThanOrEqual(2);
    });

    it('should not contain emojis in path error hints', () => {
      const result = validateToolPath(
        { path: '/etc/passwd', researchGoal: 'test', reasoning: 'test' },
        'LOCAL_FETCH_CONTENT'
      );

      expect(result.isValid).toBe(false);
      const hints = result.errorResult?.hints as string[];
      for (const hint of hints) {
        expect(hint).not.toMatch(/[\u{1F300}-\u{1F9FF}]/u);
      }
    });

    it('should not contain empty separator strings in path error hints', () => {
      const result = validateToolPath(
        { path: '/etc/passwd', researchGoal: 'test', reasoning: 'test' },
        'LOCAL_FETCH_CONTENT'
      );

      expect(result.isValid).toBe(false);
      const hints = result.errorResult?.hints as string[];
      for (const hint of hints) {
        expect(hint.trim().length).toBeGreaterThan(0);
      }
    });

    it('should include CWD and Fix in concise format', () => {
      const result = validateToolPath(
        { path: '/var/tmp/test', researchGoal: 'test', reasoning: 'test' },
        'LOCAL_FIND_FILES'
      );

      expect(result.isValid).toBe(false);
      const hints = result.errorResult?.hints as string[];
      expect(hints.some(h => h.startsWith('CWD:'))).toBe(true);
      expect(hints.some(h => h.startsWith('Fix:'))).toBe(true);
    });

    it('should not contain the old verbose TIP or multi-line format', () => {
      const result = validateToolPath(
        { path: '/outside/path', researchGoal: 'test', reasoning: 'test' },
        'LOCAL_RIPGREP'
      );

      expect(result.isValid).toBe(false);
      const hints = result.errorResult?.hints as string[];
      const allText = hints.join('\n');
      expect(allText).not.toContain('💡 TIP');
      expect(allText).not.toContain('🔧 Fix');
      expect(allText).not.toContain('Instead of:');
      expect(allText).not.toContain('Always prefer absolute paths');
    });
  });

  describe('"Good result size" removal', () => {
    const makeFiles = (count: number) =>
      Array.from({ length: count }, (_, i) => ({
        path: `/test/file${i}.ts`,
        matchCount: 1,
        matches: [
          {
            line: i + 1,
            column: 1,
            value: 'match',
            location: {
              byteOffset: 0,
              byteLength: 5,
              charOffset: 0,
              charLength: 5,
              line: i + 1,
              column: 1,
            },
          },
        ],
      }));

    it('should NOT include "Good result size" for small result sets', async () => {
      const result = await buildSearchResult(
        makeFiles(5),
        {
          path: '/test',
          pattern: 'export',
          mode: 'discovery',
          researchGoal: 'test',
          reasoning: 'test',
        } as any,
        'rg',
        []
      );

      const allHints = (result.hints || []).join('\n');
      expect(allHints).not.toContain('Good result size');
      expect(allHints).not.toContain('manageable for analysis');
    });

    it('should NOT include "Good result size" for medium result sets', async () => {
      const result = await buildSearchResult(
        makeFiles(50),
        {
          path: '/test',
          pattern: 'export',
          mode: 'paginated',
          researchGoal: 'test',
          reasoning: 'test',
        } as any,
        'rg',
        []
      );

      const allHints = (result.hints || []).join('\n');
      expect(allHints).not.toContain('Good result size');
    });
  });
});
