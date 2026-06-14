import { describe, it, expect, afterEach } from 'vitest';
import {
  checkLargeOutputSafety,
  validateToolPath,
} from '../../../../octocode-tools-core/src/utils/file/toolHelpers.js';
import { LSP_GET_SEMANTIC_CONTENT_TOOL_NAME } from '../../../../octocode-tools-core/src/tools/lsp/shared/semanticTypes.js';

describe('toolHelpers', () => {
  describe('validateToolPath', () => {
    const originalCwd = process.cwd();

    afterEach(() => {
      process.chdir(originalCwd);
    });

    describe('successful validation', () => {
      it('should return valid result with sanitizedPath for valid paths', () => {
        const query = {
          path: process.cwd(),
          id: 'test',
          researchGoal: 'test',
          reasoning: 'test reasoning',
        };

        const result = validateToolPath(query, 'LOCAL_FIND_FILES');

        expect(result.isValid).toBe(true);
        expect(result.sanitizedPath).toBeDefined();
        expect(result.errorResult).toBeUndefined();
      });

      it('should accept paths within workspace using relative notation', () => {
        const query = {
          path: '.',
          id: 'test',
          researchGoal: 'test',
          reasoning: 'test reasoning',
        };

        const result = validateToolPath(query, 'LOCAL_VIEW_STRUCTURE');

        expect(result.isValid).toBe(true);
        expect(result.sanitizedPath).toBeDefined();
      });
    });

    describe('file:// URI protocol stripping', () => {
      it('should strip file:// protocol and validate the underlying path', () => {
        const query = {
          path: `file://${process.cwd()}`,
          id: 'test',
          researchGoal: 'test',
          reasoning: 'test reasoning',
        };

        const result = validateToolPath(
          query,
          LSP_GET_SEMANTIC_CONTENT_TOOL_NAME
        );

        expect(result.isValid).toBe(true);
        expect(result.sanitizedPath).toBeDefined();
      });

      it('should strip file:/// (triple slash) and validate correctly', () => {
        const query = {
          path: `file:///${process.cwd().slice(1)}`,
          id: 'test',
          researchGoal: 'test',
          reasoning: 'test reasoning',
        };

        const result = validateToolPath(
          query,
          LSP_GET_SEMANTIC_CONTENT_TOOL_NAME
        );

        expect(result.isValid).toBe(true);
        expect(result.sanitizedPath).toBeDefined();
      });

      it('should leave non-file:// paths unchanged', () => {
        const query = {
          path: process.cwd(),
          id: 'test',
          researchGoal: 'test',
          reasoning: 'test reasoning',
        };

        const result = validateToolPath(
          query,
          LSP_GET_SEMANTIC_CONTENT_TOOL_NAME
        );

        expect(result.isValid).toBe(true);
        expect(result.sanitizedPath).toBeDefined();
      });
    });

    describe('error context and debugging info', () => {
      it('should include CWD in error result for invalid paths', () => {
        const query = {
          path: '/some/invalid/outside/path',
          id: 'test',
          researchGoal: 'test',
          reasoning: 'test reasoning',
        };

        const result = validateToolPath(query, 'LOCAL_FIND_FILES');

        expect(result.isValid).toBe(false);
        expect(result.errorResult).toBeDefined();
        expect(result.errorResult?.cwd).toBe(process.cwd());
      });

      it('should include resolvedPath in error result', () => {
        const query = {
          path: '/some/invalid/outside/path',
          id: 'test',
          researchGoal: 'test',
          reasoning: 'test reasoning',
        };

        const result = validateToolPath(query, 'LOCAL_FIND_FILES');

        expect(result.isValid).toBe(false);
        expect(result.errorResult?.resolvedPath).toBe(
          '/some/invalid/outside/path'
        );
      });

      it('should show resolved path when relative path differs from input', () => {
        const query = {
          path: '/var/log/system.log',
          id: 'test',
          researchGoal: 'test',
          reasoning: 'test reasoning',
        };

        const result = validateToolPath(query, 'LOCAL_VIEW_STRUCTURE');

        expect(result.isValid).toBe(false);
        expect(result.errorResult?.hints).toBeDefined();

        const hints = result.errorResult?.hints as string[];
        expect(hints.some(h => h.includes('CWD:'))).toBe(true);
      });

      it('should NOT show "resolved to" hint when input equals resolved path', () => {
        const query = {
          path: '/etc/passwd',
          id: 'test',
          researchGoal: 'test',
          reasoning: 'test reasoning',
        };

        const result = validateToolPath(query, 'LOCAL_FETCH_CONTENT');

        expect(result.isValid).toBe(false);
        const hints = result.errorResult?.hints as string[];
        expect(hints.some(h => h.includes('resolved to'))).toBe(false);
        expect(hints.some(h => h.includes('CWD:'))).toBe(true);
      });

      it('should return error result for invalid path', () => {
        const query = {
          path: '/invalid/path',
          researchGoal: 'Find config files',
          reasoning: 'Need to check configuration',
          mainResearchGoal: 'Understand project setup',
        };

        const result = validateToolPath(query, 'LOCAL_FIND_FILES');

        expect(result.isValid).toBe(false);
        expect(result.errorResult).toBeDefined();
      });
    });

    describe('hint quality for different error types', () => {
      it('should provide fix suggestions for path outside allowed directories', () => {
        const query = {
          path: '/etc/passwd',
          id: 'test',
          researchGoal: 'test',
          reasoning: 'test reasoning',
        };

        const result = validateToolPath(query, 'LOCAL_FETCH_CONTENT');

        expect(result.isValid).toBe(false);
        const hints = result.errorResult?.hints as string[];
        expect(hints.some(h => h.includes('Fix:'))).toBe(true);
        expect(hints.some(h => h.includes(process.cwd()))).toBe(true);
      });

      it('should provide permission denied hints when error contains Permission denied', () => {
        const query = {
          path: '/root/secret',
          id: 'test',
          researchGoal: 'test',
          reasoning: 'test reasoning',
        };

        const result = validateToolPath(query, 'LOCAL_FETCH_CONTENT');

        expect(result.isValid).toBe(false);
        const hints = result.errorResult?.hints as string[];
        expect(hints.some(h => h.includes('Fix:'))).toBe(true);
      });

      it('should provide not found hints when path does not exist', () => {
        const query = {
          path: `${process.cwd()}/nonexistent_path_xyz_123`,
          id: 'test',
          researchGoal: 'test',
          reasoning: 'test reasoning',
        };

        const result = validateToolPath(query, 'LOCAL_FIND_FILES');

        if (!result.isValid) {
          const hints = result.errorResult?.hints as string[];
          expect(hints).toBeDefined();
          expect(hints.some(h => h.includes('CWD:'))).toBe(true);
        }
      });

      it('should include helpful hints about using absolute paths', () => {
        const query = {
          path: '/var/tmp/test',
          id: 'test',
          researchGoal: 'test',
          reasoning: 'test reasoning',
        };

        const result = validateToolPath(query, 'LOCAL_FIND_FILES');

        expect(result.isValid).toBe(false);
        const hints = result.errorResult?.hints as string[];

        expect(hints.some(h => h.includes('Fix:'))).toBe(true);
        expect(hints.some(h => h.toLowerCase().includes('absolute'))).toBe(
          true
        );
      });

      it('should include example fix syntax', () => {
        const query = {
          path: '/outside/path',
          id: 'test',
          researchGoal: 'test',
          reasoning: 'test reasoning',
        };

        const result = validateToolPath(query, 'LOCAL_RIPGREP');

        expect(result.isValid).toBe(false);
        const hints = result.errorResult?.hints as string[];

        expect(hints.some(h => h.includes('path="'))).toBe(true);
        expect(hints.some(h => h.includes('Fix:'))).toBe(true);
      });
    });

    describe('error code and status', () => {
      it('should include errorCode in error result', () => {
        const query = {
          path: '/invalid/path',
          id: 'test',
          researchGoal: 'test',
          reasoning: 'test reasoning',
        };

        const result = validateToolPath(query, 'LOCAL_FIND_FILES');

        expect(result.isValid).toBe(false);
        expect(result.errorResult?.errorCode).toBe('pathValidationFailed');
      });

      it('should set status to error', () => {
        const query = {
          path: '/invalid/path',
          id: 'test',
          researchGoal: 'test',
          reasoning: 'test reasoning',
        };

        const result = validateToolPath(query, 'LOCAL_FIND_FILES');

        expect(result.isValid).toBe(false);
        expect(result.errorResult?.status).toBe('error');
      });
    });
  });

  describe('checkLargeOutputSafety', () => {
    it('should not block when hasCharLength is true', () => {
      const result = checkLargeOutputSafety(1000, true);

      expect(result.shouldBlock).toBe(false);
      expect(result.errorCode).toBeUndefined();
    });

    it('should not block when itemCount is below threshold', () => {
      const result = checkLargeOutputSafety(50, false, { threshold: 100 });

      expect(result.shouldBlock).toBe(false);
      expect(result.errorCode).toBeUndefined();
    });

    it('should block when itemCount exceeds threshold', () => {
      const result = checkLargeOutputSafety(150, false, { threshold: 100 });

      expect(result.shouldBlock).toBe(true);
      expect(result.errorCode).toBeDefined();
      expect(result.hints).toBeDefined();
      expect(result.hints?.some(h => h.includes('150'))).toBe(true);
      expect(result.hints?.some(h => h.includes('exceeds'))).toBe(true);
    });

    it('should use default threshold of 100', () => {
      const result = checkLargeOutputSafety(101, false);

      expect(result.shouldBlock).toBe(true);
    });

    it('should include custom itemType in hints', () => {
      const result = checkLargeOutputSafety(150, false, {
        threshold: 100,
        itemType: 'file',
      });

      expect(result.hints?.some(h => h.includes('files'))).toBe(true);
    });

    it('should use singular form for count of 1 (edge case)', () => {
      const result = checkLargeOutputSafety(1, false, {
        threshold: 0,
        itemType: 'item',
      });

      expect(result.shouldBlock).toBe(true);
      expect(result.hints?.some(h => h.includes('1 item -'))).toBe(true);
    });

    it('should show detailed hint when detailed is true', () => {
      const result = checkLargeOutputSafety(150, false, {
        threshold: 100,
        detailed: true,
      });

      expect(
        result.hints?.some(h => h.includes('Detailed results increase size'))
      ).toBe(true);
    });

    it('should show generic hint when detailed is false', () => {
      const result = checkLargeOutputSafety(150, false, {
        threshold: 100,
        detailed: false,
      });

      expect(
        result.hints?.some(h =>
          h.includes('Consider using charLength to paginate')
        )
      ).toBe(true);
    });

    it('should not block at exactly threshold', () => {
      const result = checkLargeOutputSafety(100, false, { threshold: 100 });

      expect(result.shouldBlock).toBe(false);
    });

    it('should block at threshold + 1', () => {
      const result = checkLargeOutputSafety(101, false, { threshold: 100 });

      expect(result.shouldBlock).toBe(true);
    });
  });
});
