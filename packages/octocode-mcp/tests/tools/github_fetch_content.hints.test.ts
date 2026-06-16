import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createSuccessResult } from '../../../octocode-tools-core/src/tools/utils.js';

vi.mock('../../../octocode-tools-core/src/session.js', () => ({
  logSessionError: vi.fn(() => Promise.resolve()),
}));

describe('ghGetFileContent Tool Handler - Hints Fix', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('pagination hints passthrough', () => {
    it('should pass through pagination hints from API result', () => {
      const query = {
        mainResearchGoal: 'Read file content',
        researchGoal: 'Testing hints passthrough',
        reasoning: 'Verify hints are passed',
      };

      const apiResult = {
        owner: 'facebook',
        repo: 'react',
        path: 'README.md',
        branch: 'main',
        content: '# React\n...',
        contentLength: 500,
        pagination: {
          currentPage: 1,
          totalPages: 10,
          hasMore: true,
          charOffset: 0,
          charLength: 500,
          totalChars: 5000,
        },
        hints: [
          '📄 More available: This is page 1 of 10',
          '▶ Next page: Use charOffset=500 to continue',
        ],
      };

      const paginationHints = Array.isArray(apiResult.hints)
        ? apiResult.hints
        : [];

      const result = createSuccessResult(
        query,
        apiResult,
        true,
        'GITHUB_FETCH_CONTENT',
        { extraHints: paginationHints }
      );

      expect(result.status).toBeUndefined();
      expect(result.hints).toBeDefined();
      expect(result.hints).toContain('📄 More available: This is page 1 of 10');
      expect(result.hints).toContain(
        '▶ Next page: Use charOffset=500 to continue'
      );
    });

    it('should handle empty hints array gracefully', () => {
      const query = {
        researchGoal: 'Testing empty hints',
        reasoning: 'Verify empty hints handling',
      };

      const apiResult = {
        owner: 'facebook',
        repo: 'react',
        path: 'README.md',
        content: '# React',
        contentLength: 7,
        hints: [],
      };

      const paginationHints = Array.isArray(apiResult.hints)
        ? apiResult.hints
        : [];

      const result = createSuccessResult(
        query,
        apiResult,
        true,
        'GITHUB_FETCH_CONTENT',
        { extraHints: paginationHints }
      );

      expect(result.status).toBeUndefined();
      expect(result.hints === undefined || result.hints?.length === 0).toBe(
        true
      );
    });

    it('should handle missing hints property gracefully', () => {
      const query = {
        researchGoal: 'Testing missing hints',
        reasoning: 'Verify missing hints handling',
      };

      const apiResult = {
        owner: 'facebook',
        repo: 'react',
        path: 'README.md',
        content: '# React',
        contentLength: 7,
      };

      const resultObj = apiResult as Record<string, unknown>;
      const paginationHints = Array.isArray(resultObj.hints)
        ? (resultObj.hints as string[])
        : [];

      const result = createSuccessResult(
        query,
        apiResult,
        true,
        'GITHUB_FETCH_CONTENT',
        { extraHints: paginationHints }
      );

      expect(result.status).toBeUndefined();
      expect(result.hints).toBeUndefined();
    });

    it('should include charOffset hint for paginated content', () => {
      const query = {
        researchGoal: 'Testing charOffset hint',
        reasoning: 'Verify charOffset navigation hint',
      };

      const apiResult = {
        owner: 'facebook',
        repo: 'react',
        path: 'package.json',
        content: '{ "name": "react" ... }',
        contentLength: 1000,
        pagination: {
          currentPage: 2,
          totalPages: 5,
          hasMore: true,
          charOffset: 1000,
          charLength: 1000,
          totalChars: 5000,
        },
        hints: [
          '📄 More available: This is page 2 of 5',
          '▶ Next page: Use charOffset=2000 to continue',
        ],
      };

      const paginationHints = Array.isArray(apiResult.hints)
        ? apiResult.hints
        : [];

      const result = createSuccessResult(
        query,
        apiResult,
        true,
        'GITHUB_FETCH_CONTENT',
        { extraHints: paginationHints }
      );

      expect(result.hints).toContain(
        '▶ Next page: Use charOffset=2000 to continue'
      );
    });

    it('should include final page hint when on last page', () => {
      const query = {
        researchGoal: 'Testing final page hint',
        reasoning: 'Verify final page message',
      };

      const apiResult = {
        owner: 'facebook',
        repo: 'react',
        path: 'README.md',
        content: '...end of file',
        contentLength: 500,
        pagination: {
          currentPage: 10,
          totalPages: 10,
          hasMore: false,
          charOffset: 4500,
          charLength: 500,
          totalChars: 5000,
        },
        hints: ['✓ Final page: Reached end of content'],
      };

      const paginationHints = Array.isArray(apiResult.hints)
        ? apiResult.hints
        : [];

      const result = createSuccessResult(
        query,
        apiResult,
        true,
        'GITHUB_FETCH_CONTENT',
        { extraHints: paginationHints }
      );

      expect(result.hints).toContain('✓ Final page: Reached end of content');
    });
  });

  describe('hints extraction logic', () => {
    it('should extract hints array correctly from result object', () => {
      const resultObj: Record<string, unknown> = {
        content: 'file content',
        hints: ['Hint 1', 'Hint 2', 'Hint 3'],
      };

      const extractedHints = Array.isArray(resultObj.hints)
        ? (resultObj.hints as string[])
        : [];

      expect(extractedHints).toHaveLength(3);
      expect(extractedHints).toEqual(['Hint 1', 'Hint 2', 'Hint 3']);
    });

    it('should return empty array when hints is not an array', () => {
      const resultObj: Record<string, unknown> = {
        content: 'file content',
        hints: 'not an array',
      };

      const extractedHints = Array.isArray(resultObj.hints)
        ? (resultObj.hints as string[])
        : [];

      expect(extractedHints).toHaveLength(0);
    });

    it('should return empty array when hints is null', () => {
      const resultObj: Record<string, unknown> = {
        content: 'file content',
        hints: null,
      };

      const extractedHints = Array.isArray(resultObj.hints)
        ? (resultObj.hints as string[])
        : [];

      expect(extractedHints).toHaveLength(0);
    });
  });
});
