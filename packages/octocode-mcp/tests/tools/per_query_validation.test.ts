/**
 * Tests that per-query Zod validation in execution handlers returns
 * per-query errors instead of failing the entire batch.
 *
 * These tests call execution handlers directly (bypassing MCP SDK schema check)
 * with queries containing null bytes in paths, which pass the outer bulk schema
 * shape but fail the individual query Zod refine.
 */

import { describe, it, expect, vi } from 'vitest';
import { executeFindFiles } from '../../src/tools/local_find_files/execution.js';
import { executeViewStructure } from '../../src/tools/local_view_structure/execution.js';
import { executeRipgrepSearch } from '../../src/tools/local_ripgrep/execution.js';
import { executeFetchContent } from '../../src/tools/local_fetch_content/execution.js';

vi.mock('octocode-security-utils/pathValidator', () => ({
  pathValidator: {
    validate: vi.fn().mockReturnValue({ isValid: true }),
  },
}));

vi.mock('fs', () => {
  const mockModule = {
    lstatSync: vi.fn(),
    promises: {
      readdir: vi.fn().mockResolvedValue([]),
      lstat: vi.fn().mockResolvedValue({
        isDirectory: () => true,
        isFile: () => false,
        isSymbolicLink: () => false,
        size: 0,
        mode: 0o755,
        mtime: new Date(),
      }),
    },
  };
  return { ...mockModule, default: mockModule };
});

vi.mock('fs/promises', () => ({
  readFile: vi.fn().mockResolvedValue(''),
  stat: vi.fn().mockResolvedValue({ size: 0 }),
}));

vi.mock('../../src/utils/exec/safe.js', () => ({
  safeExec: vi
    .fn()
    .mockResolvedValue({ success: true, stdout: '', stderr: '' }),
}));

vi.mock('../../src/utils/exec/commandAvailability.js', () => ({
  checkCommandAvailability: vi
    .fn()
    .mockResolvedValue({ available: true, command: 'test' }),
  getMissingCommandError: vi.fn().mockReturnValue('Command not available'),
}));

function getResultText(result: { content: { text: string }[] }): string {
  return result.content[0]!.text;
}

describe('Per-query validation in execution handlers', () => {
  describe('executeFindFiles', () => {
    it('should return per-query validation error for null-byte path', async () => {
      const result = await executeFindFiles({
        queries: [{ path: '/tmp/test\0evil' } as never],
      });

      const text = getResultText(result as never);
      expect(text).toContain('error');
      expect(text).toContain('null byte');
    });
  });

  describe('executeViewStructure', () => {
    it('should return per-query validation error for null-byte path', async () => {
      const result = await executeViewStructure({
        queries: [{ path: '/tmp/test\0evil' } as never],
      });

      const text = getResultText(result as never);
      expect(text).toContain('error');
      expect(text).toContain('null byte');
    });
  });

  describe('executeRipgrepSearch', () => {
    it('should return per-query validation error for null-byte path', async () => {
      const result = await executeRipgrepSearch({
        queries: [{ path: '/tmp/test\0evil', pattern: 'foo' } as never],
      });

      const text = getResultText(result as never);
      expect(text).toContain('error');
      expect(text).toContain('null byte');
    });
  });

  describe('executeFetchContent (reference pattern)', () => {
    it('should return per-query validation error for null-byte path', async () => {
      const result = await executeFetchContent({
        queries: [{ path: '/tmp/test\0evil' } as never],
      });

      const text = getResultText(result as never);
      expect(text).toContain('error');
      expect(text).toContain('null byte');
    });
  });

  describe('consistent behavior across all tools', () => {
    it('all four tools should catch null-byte paths at per-query level', async () => {
      const invalidQuery = { path: '/valid\0path' };

      const [findResult, viewResult, ripgrepResult, fetchResult] =
        await Promise.all([
          executeFindFiles({ queries: [invalidQuery as never] }),
          executeViewStructure({ queries: [invalidQuery as never] }),
          executeRipgrepSearch({
            queries: [{ ...invalidQuery, pattern: 'x' } as never],
          }),
          executeFetchContent({ queries: [invalidQuery as never] }),
        ]);

      for (const result of [
        findResult,
        viewResult,
        ripgrepResult,
        fetchResult,
      ]) {
        const text = getResultText(result as never);
        expect(text).toContain('error');
        expect(text).toContain('null byte');
      }
    });
  });
});
