import { describe, it, expect, vi, beforeAll, afterEach } from 'vitest';
import { incrementToolCharSavings } from 'octocode-shared';
import { executeBulkOperation } from '../../src/utils/response/bulk.js';
import { attachRawResponseChars } from '../../src/utils/response/charSavings.js';
import type { QueryStatus } from '../../src/types/toolResults.js';
import { TOOL_NAMES } from '../../src/tools/toolMetadata/proxies.js';
import { initializeToolMetadata } from '../../src/tools/toolMetadata/state.js';
import type { ToolName } from '../../src/tools/toolMetadata/types.js';
import { getTextContent } from './testHelpers.js';

beforeAll(async () => {
  await initializeToolMetadata();
});

describe('executeBulkOperation', () => {
  describe('Single query scenarios', () => {
    it('returns all query results without truncation', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockResolvedValue({
        repositories: [
          { name: 'alpha-repository-with-long-name' },
          { name: 'beta-repository-with-long-name' },
          { name: 'gamma-repository-with-long-name' },
        ],
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
      });

      const structured = result.structuredContent as {
        results: Array<{
          data: {
            repositories?: Array<{ name: string }>;
          };
        }>;
      };

      expect(structured.results[0]?.data.repositories).toHaveLength(3);
    });

    it('marks peer evidence incomplete when query output pagination has more data', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockResolvedValue({
        incomingCalls: [],
        outputPagination: { hasMore: true },
        evidence: {
          kind: 'calls',
          answerReady: true,
          complete: true,
          confidence: 'high',
        },
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.LSP_CALL_HIERARCHY,
        peerEvidence: true,
      });

      const structured = result.structuredContent as {
        evidence?: {
          kind?: string;
          complete?: boolean;
          confidence?: string;
          reason?: string;
        };
      };

      expect(structured.evidence).toMatchObject({
        kind: 'calls',
        complete: false,
        confidence: 'high',
      });
      expect(structured.evidence?.reason).toContain(
        'One or more query-level output pages have more data.'
      );
    });

    it('marks peer evidence complete when all queries succeed', async () => {
      const queries = Array.from({ length: 5 }, (_, index) => ({
        id: `q${index + 1}`,
      }));
      const processor = vi.fn().mockImplementation(query =>
        Promise.resolve({
          packages: [
            {
              name: query.id,
              description: 'x'.repeat(500),
            },
          ],
          evidence: {
            kind: 'package',
            answerReady: true,
            complete: true,
            confidence: 'high',
          },
        })
      );

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.PACKAGE_SEARCH,
        peerEvidence: true,
      });

      const structured = result.structuredContent as {
        evidence?: {
          kind?: string;
          complete?: boolean;
          confidence?: string;
        };
      };

      expect(structured.evidence).toMatchObject({
        kind: 'package',
        complete: true,
        confidence: 'high',
      });
    });

    it('should process single query with hasResults status', async () => {
      const queries = [{ id: 'q1', name: 'test1' }];
      const processor = vi.fn().mockResolvedValue({
        files: [{ path: 'test.ts', content: 'data' }],
        hints: ['Test hint for hasResults'],
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
        keysPriority: ['files'],
      });

      expect(result.isError).toBe(false);
      expect(processor).toHaveBeenCalledTimes(1);
      expect(processor).toHaveBeenCalledWith(queries[0], 0);

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('results:');
      expect(responseText).toContain('id: "q1"');
      expect(responseText).not.toContain('instructions:');
      expect(responseText).not.toContain('status: "hasResults"');
      expect(responseText).toContain('path: "test.ts"');
      expect(responseText).toContain('Test hint for hasResults');
    });

    it('should process single query with empty status', async () => {
      const queries = [{ id: 'q1', search: 'nonexistent' }];
      const processor = vi.fn().mockResolvedValue({
        status: 'empty' as const,
        files: [],
        hints: ['Test hint for empty'],
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('status: "empty"');
      expect(responseText).toContain('Test hint for empty');
    });

    it('should process single query with error status', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockResolvedValue({
        status: 'error' as const,
        error: 'Rate limit exceeded',
        hints: ['Test hint for error'],
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('status: "error"');
      expect(responseText).toContain('error: "Rate limit exceeded"');
      expect(responseText).toContain('Test hint for error');
    });

    it('surfaces error messages in the text payload when every query fails (regression)', async () => {
      const queries = [
        { id: 'q1', pattern: 'x', path: '/tmp' },
        { id: 'q2', pattern: 'y', path: '/tmp' },
      ];
      const processor = vi.fn().mockResolvedValue({
        status: 'error' as const,
        error: 'filesOnly and filesWithoutMatch are mutually exclusive',
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.LOCAL_RIPGREP,
      });

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('status: "error"');
      expect(responseText).toContain('mutually exclusive');
    });

    it('should handle processor throwing error', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockRejectedValue(new Error('API error'));

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_FETCH_CONTENT,
      });

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('status: "error"');
      expect(responseText).toContain('error: "API error"');
    });
  });

  describe('Multiple queries - same status', () => {
    it('records responseChars for all results', async () => {
      const queries = [{ id: 'q1' }, { id: 'q2' }, { id: 'q3' }];
      const processor = vi
        .fn()
        .mockImplementation(async (query: { id: string }) =>
          attachRawResponseChars(
            {
              repositories: [
                { name: `${query.id}-repository-with-a-very-long-name` },
              ],
            },
            1_000
          )
        );

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
      });

      const responseText = getTextContent(result.content);
      const structured = result.structuredContent as {
        results: Array<{ id: string }>;
      };
      const [toolName, rawChars, responseChars] =
        vi.mocked(incrementToolCharSavings).mock.calls.at(-1) ?? [];

      expect(toolName).toBe(TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES);
      expect(rawChars).toBe(3_000);
      expect(responseChars).toBe(responseText.length);
      expect(structured.results.length).toBe(queries.length);
    });

    it('returns all results without truncation', async () => {
      const queries = [{ id: 'q1' }, { id: 'q2' }, { id: 'q3' }];
      const processor = vi
        .fn()
        .mockImplementation(async (query: { id: string }) => ({
          repositories: [{ name: `${query.id}-repository-with-long-name` }],
        }));

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
      });

      const structured = result.structuredContent as {
        results: Array<{ id: string }>;
      };

      expect(structured.results.length).toBe(3);
    });

    it('should process multiple queries all with hasResults status', async () => {
      const queries = [
        { id: 'q1', name: 'react' },
        { id: 'q2', name: 'vue' },
        { id: 'q3', name: 'angular' },
      ];
      const processor = vi
        .fn()
        .mockImplementation(async (query: { id: string; name: string }) => ({
          repositories: [{ name: `${query.name}-repo` }],
        }));

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
      });

      expect(result.isError).toBe(false);
      expect(processor).toHaveBeenCalledTimes(3);

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('id: "q3"');
      expect(responseText).not.toContain('empty');
      expect(responseText).not.toContain('failed');
      expect(responseText).toContain('name: "react-repo"');
      expect(responseText).toContain('name: "vue-repo"');
      expect(responseText).toContain('name: "angular-repo"');
    });

    it('should process multiple queries all with empty status', async () => {
      const queries = [
        { id: 'q1', search: 'xyz123' },
        { id: 'q2', search: 'abc456' },
      ];
      const processor = vi.fn().mockResolvedValue({
        status: 'empty' as const,
        files: [],
        hints: ['Test hint for empty'],
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('id: "q2"');
      expect(responseText).not.toContain('failed');
      expect(responseText).toContain('Test hint for empty');
    });

    it('should process multiple queries all with error status', async () => {
      const queries = [{ id: 'q1' }, { id: 'q2' }, { id: 'q3' }];
      const processor = vi.fn().mockResolvedValue({
        status: 'error' as const,
        error: 'Authentication failed',
        hints: ['Test hint for error'],
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_FETCH_CONTENT,
      });

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('id: "q3"');
      expect(responseText).not.toContain('empty');
      expect(responseText).toContain('Test hint for error');
    });

    it('should process multiple queries all throwing errors', async () => {
      const queries = [{ id: 'q1' }, { id: 'q2' }];
      const processor = vi.fn().mockRejectedValue(new Error('Network timeout'));

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      });

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('id: "q2"');
      expect(responseText).toContain('error: "Network timeout"');
    });
  });

  describe('Multiple queries - mixed statuses (2 types)', () => {
    it('should record aggregate char savings for parallel mixed responses', async () => {
      vi.mocked(incrementToolCharSavings).mockClear();
      const queries = [
        { id: 'q1', delayMs: 20, type: 'success' },
        { id: 'q2', delayMs: 5, type: 'throw' },
        { id: 'q3', delayMs: 0, type: 'success' },
      ];
      const largePayload = 'x'.repeat(2000);

      const processor = vi
        .fn()
        .mockImplementation(
          async (query: {
            id: string;
            delayMs: number;
            type: 'success' | 'throw';
          }) => {
            await new Promise(resolve => setTimeout(resolve, query.delayMs));

            if (query.type === 'throw') {
              throw new Error(`processor failed for ${query.id}`);
            }

            return attachRawResponseChars(
              {
                payload: `${query.id}:${largePayload}`,
              },
              5000
            );
          }
        );

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_FETCH_CONTENT,
        concurrency: 3,
      });

      const responseText = getTextContent(result.content);
      expect(incrementToolCharSavings).toHaveBeenCalledTimes(1);
      expect(incrementToolCharSavings).toHaveBeenCalledWith(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        expect.any(Number),
        responseText.length
      );
      const [, rawChars] = vi.mocked(incrementToolCharSavings).mock.calls[0]!;
      expect(rawChars).toBeGreaterThanOrEqual(10000);
      expect(responseText).not.toContain('octocode.rawResponseChars');
      expect(result.structuredContent).not.toHaveProperty(
        'octocode.rawResponseChars'
      );
    });

    it('should preserve input query order when thrown errors are mixed with successes', async () => {
      const queries = [
        { id: 'q1', delayMs: 25, type: 'success' },
        { id: 'q2', delayMs: 5, type: 'throw' },
        { id: 'q3', delayMs: 0, type: 'success' },
      ];

      const processor = vi
        .fn()
        .mockImplementation(
          async (query: {
            id: string;
            delayMs: number;
            type: 'success' | 'throw';
          }) => {
            await new Promise(resolve => setTimeout(resolve, query.delayMs));

            if (query.type === 'throw') {
              throw new Error(`processor failed for ${query.id}`);
            }

            return {
              payload: query.id,
            };
          }
        );

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_FETCH_CONTENT,
        concurrency: 3,
      });

      expect(result.isError).toBe(false);

      const structured = result.structuredContent as {
        results: Array<{
          id: string;
          status: QueryStatus;
          data: Record<string, unknown>;
        }>;
      };

      expect(structured.results.map(entry => entry.id)).toEqual([
        'q1',
        'q2',
        'q3',
      ]);
      expect(structured.results[1]).toMatchObject({
        id: 'q2',
        status: 'error',
        data: {
          error: 'processor failed for q2',
        },
      });

      const responseText = getTextContent(result.content);
      expect(responseText.indexOf('id: "q1"')).toBeLessThan(
        responseText.indexOf('id: "q2"')
      );
      expect(responseText.indexOf('id: "q2"')).toBeLessThan(
        responseText.indexOf('id: "q3"')
      );
    });

    it('should handle hasResults + empty mix', async () => {
      const queries = [
        { id: 'q1', name: 'found' },
        { id: 'q2', name: 'notfound' },
        { id: 'q3', name: 'found2' },
      ];
      const processor = vi
        .fn()
        .mockImplementation(async (query: { id: string; name: string }) => {
          const isFound = !query.name.startsWith('notfound');
          return {
            status: isFound ? ('hasResults' as const) : ('empty' as const),
            pull_requests: isFound ? [{ number: 1 }] : [],
            hints: ['Test hint'],
          };
        });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('id: "q3"');
      expect(responseText).not.toContain('failed');
      expect(responseText).toContain('Test hint');
    });

    it('should handle hasResults + error mix', async () => {
      const queries = [
        { id: 'q1', type: 'success' },
        { id: 'q2', type: 'error' },
        { id: 'q3', type: 'success' },
      ];
      const processor = vi
        .fn()
        .mockImplementation(async (query: { id: string; type: string }) => {
          if (query.type === 'error') {
            return {
              status: 'error' as const,
              error: 'Failed to fetch',
              hints: ['Test hint for error'],
            };
          }
          return {
            data: { result: 'success' },
            hints: ['Test hint for success'],
          };
        });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_FETCH_CONTENT,
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('id: "q3"');
      expect(responseText).not.toContain(': 0 empty');
      expect(responseText).toContain('Test hint for success');
      expect(responseText).toContain('Test hint for error');
    });

    it('should handle empty + error mix', async () => {
      const queries = [
        { id: 'q1', type: 'empty' },
        { id: 'q2', type: 'error' },
        { id: 'q3', type: 'empty' },
        { id: 'q4', type: 'error' },
      ];
      const processor = vi
        .fn()
        .mockImplementation(async (query: { id: string; type: string }) => {
          if (query.type === 'error') {
            throw new Error('Query failed');
          }
          return {
            status: 'empty' as const,
            files: [],
            hints: ['Test hint for empty'],
          };
        });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('id: "q4"');
      expect(responseText).toContain('Test hint for empty');
    });
  });

  describe('Multiple queries - all 3 status types', () => {
    it('should handle hasResults + empty + error mix', async () => {
      const queries = [
        { id: 'q1', type: 'hasResults' },
        { id: 'q2', type: 'empty' },
        { id: 'q3', type: 'error' },
        { id: 'q4', type: 'throw' },
      ];
      const processor = vi
        .fn()
        .mockImplementation(async (query: { id: string; type: string }) => {
          if (query.type === 'throw') {
            throw new Error('Processing failed');
          }
          if (query.type === 'error') {
            return {
              status: 'error' as const,
              error: 'Custom error',
              hints: ['Test hint for error'],
            };
          }
          return {
            status: query.type as 'hasResults' | 'empty',
            data: query.type === 'hasResults' ? { result: 'data' } : {},
            hints: ['Test hint'],
          };
        });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('id: "q4"');
      expect(responseText).toContain('Test hint');
    });

    it('should handle balanced mix of all statuses', async () => {
      const queries = [
        { id: 'q1', type: 'hasResults' },
        { id: 'q2', type: 'hasResults' },
        { id: 'q3', type: 'empty' },
        { id: 'q4', type: 'empty' },
        { id: 'q5', type: 'error' },
        { id: 'q6', type: 'error' },
      ];
      const processor = vi
        .fn()
        .mockImplementation(async (query: { id: string; type: string }) => {
          if (query.type === 'error') {
            return { status: 'error' as const, error: 'Error occurred' };
          }
          return {
            status: query.type as QueryStatus,
            repositories: query.type === 'hasResults' ? [{ name: 'repo' }] : [],
          };
        });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('id: "q6"');
    });
  });

  describe('Request metadata omission', () => {
    it('should not echo query research metadata in success responses', async () => {
      const queries = [
        {
          id: 'q1',
          mainResearchGoal: 'Understand API patterns',
          researchGoal: 'Find implementations',
          reasoning: 'Looking for patterns',
        },
      ];
      const processor = vi.fn().mockResolvedValue({
        files: [{ path: 'test.py' }],
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const responseText = getTextContent(result.content);
      expect(responseText).not.toContain('mainResearchGoal:');
      expect(responseText).not.toContain('researchGoal:');
      expect(responseText).not.toContain('reasoning:');
    });

    it('should not echo processor research metadata in success responses', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockResolvedValue({
        mainResearchGoal: 'Result main goal',
        researchGoal: 'Result goal',
        reasoning: 'Result reasoning',
        actualData: 'This should appear',
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
      });

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('actualData: "This should appear"');
      expect(responseText).not.toContain('Result main goal');
      expect(responseText).not.toContain('Result goal');
      expect(responseText).not.toContain('Result reasoning');
    });

    it('should preserve explicit processor fields instead of stripping them generically', async () => {
      const queries = [
        {
          id: 'q1',
          owner: 'facebook',
          repo: 'react',
          path: 'README.md',
          branch: 'main',
        },
      ];
      const processor = vi.fn().mockResolvedValue({
        owner: 'facebook',
        repo: 'react',
        path: 'README.md',
        branch: 'main',
        content: '# React',
        actualBranch: 'main',
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_FETCH_CONTENT,
      });

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('content: "# React"');
      expect(responseText).toContain('owner: "facebook"');
      expect(responseText).toContain('repo: "react"');
      expect(responseText).toContain('path: "README.md"');
      expect(responseText).toContain('branch: "main"');
    });

    it('should preserve structured error metadata from processor results', async () => {
      const queries = [
        {
          id: 'q1',
          path: './src/index.ts',
        },
      ];
      const processor = vi.fn().mockResolvedValue({
        status: 'error' as const,
        error: 'File access failed',
        errorCode: 'fileAccessFailed',
        resolvedPath: '/repo/src/index.ts',
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.LOCAL_FETCH_CONTENT,
      });

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('errorCode: "fileAccessFailed"');
      expect(responseText).toContain('resolvedPath: "/repo/src/index.ts"');
    });

    it('should not echo query research metadata in thrown error responses', async () => {
      const queries = [
        {
          id: 'q1',
          mainResearchGoal: 'Test main goal',
          researchGoal: 'Test goal',
          reasoning: 'Test reasoning',
        },
      ];
      const processor = vi.fn().mockRejectedValue(new Error('Failed'));

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('status: "error"');
      expect(responseText).toContain('error: "Failed"');
      expect(responseText).not.toContain('mainResearchGoal:');
      expect(responseText).not.toContain('researchGoal:');
      expect(responseText).not.toContain('reasoning:');
    });

    it('should not echo processor research metadata in error responses', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockResolvedValue({
        status: 'error' as const,
        error: 'Custom error',
        mainResearchGoal: 'Result main goal',
        researchGoal: 'Result goal',
        reasoning: 'Result reasoning',
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
      });

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('error: "Custom error"');
      expect(responseText).not.toContain('Result main goal');
      expect(responseText).not.toContain('Result goal');
      expect(responseText).not.toContain('Result reasoning');
    });
  });

  describe('Custom hints handling', () => {
    it('should include custom hints for hasResults status', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockResolvedValue({
        hints: ['Custom hint 1', 'Custom hint 2'],
        data: { test: true },
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('Custom hint 1');
      expect(responseText).toContain('Custom hint 2');
    });

    it('should include custom hints for empty status', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockResolvedValue({
        status: 'empty' as const,
        hints: ['Try broadening search', 'Check spelling'],
        files: [],
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('Try broadening search');
      expect(responseText).toContain('Check spelling');
    });

    it('should include custom hints for error status', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockResolvedValue({
        status: 'error' as const,
        error: 'Rate limited',
        hints: ['Wait before retrying', 'Use authentication'],
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('Wait before retrying');
      expect(responseText).toContain('Use authentication');
    });

    it('should deduplicate hints across multiple queries with same hints', async () => {
      const queries = [{ id: 'q1' }, { id: 'q2' }, { id: 'q3' }];
      const processor = vi.fn().mockResolvedValue({
        hints: ['Same hint for all'],
        data: { test: true },
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const responseText = getTextContent(result.content);
      const hintMatches = (responseText.match(/Same hint for all/g) || [])
        .length;
      expect(hintMatches).toBe(3);
    });

    it('should collect and deduplicate hints from mixed statuses', async () => {
      const queries = [
        { id: 'q1', type: 'hasResults' },
        { id: 'q2', type: 'hasResults' },
        { id: 'q3', type: 'empty' },
        { id: 'q4', type: 'empty' },
      ];
      const processor = vi
        .fn()
        .mockImplementation(async (query: { id: string; type: string }) => {
          if (query.type === 'hasResults') {
            return {
              hints: ['Success hint'],
              data: { result: true },
            };
          }
          return {
            status: 'empty' as const,
            hints: ['Empty hint'],
            files: [],
          };
        });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('Success hint');
      expect(responseText).toContain('Empty hint');
      const successHintMatches = (responseText.match(/Success hint/g) || [])
        .length;
      const emptyHintMatches = (responseText.match(/Empty hint/g) || []).length;
      expect(successHintMatches).toBe(2);
      expect(emptyHintMatches).toBe(2);
    });
  });

  describe('Tool-specific data fields', () => {
    it('should preserve all tool-specific fields in response', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockResolvedValue({
        pull_requests: [{ number: 123, title: 'Test PR' }],
        total_count: 1,
        incomplete_results: false,
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
      });

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('pull_requests:');
      expect(responseText).toContain('number: 123');
      expect(responseText).toContain('title: "Test PR"');
      expect(responseText).toContain('total_count: 1');
      expect(responseText).toContain('incomplete_results: false');
    });

    it('should exclude metadata fields from data section', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockResolvedValue({
        researchGoal: 'Test goal',
        reasoning: 'Test reasoning',
        researchSuggestions: ['Test'],
        error: 'Should not appear in data',
        hints: ['Test hint'],
        query: { test: 'query' },
        actualData: 'This should appear',
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const responseText = getTextContent(result.content);
      expect(responseText).not.toContain('researchGoal:');
      expect(responseText).not.toContain('reasoning:');
      expect(responseText).toContain('actualData: "This should appear"');
      expect(responseText).toContain('data:');
    });

    it('should handle complex nested data structures', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockResolvedValue({
        files: [
          {
            path: 'src/index.ts',
            matches: [
              { line: 10, content: 'match1' },
              { line: 20, content: 'match2' },
            ],
          },
        ],
        metadata: {
          total: 2,
          page: 1,
        },
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('path: "src/index.ts"');
      expect(responseText).toContain('line: 10');
      expect(responseText).toContain('content: "match1"');
      expect(responseText).toContain('line: 20');
      expect(responseText).toContain('content: "match2"');
      expect(responseText).toContain('total: 2');
      expect(responseText).toContain('page: 1');
    });
  });

  describe('Config options', () => {
    it('should respect keysPriority for field ordering', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockResolvedValue({
        owner: 'testowner',
        repo: 'testrepo',
        files: [{ path: 'src/index.ts' }],
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
        keysPriority: ['owner', 'repo', 'files'],
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('owner: "testowner"');
      expect(responseText).toContain('repo: "testrepo"');
      expect(responseText).toContain('files:');
    });

    it('should work with different tool names', async () => {
      const toolNames: ToolName[] = [
        TOOL_NAMES.GITHUB_SEARCH_CODE,
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      ];

      for (const toolName of toolNames) {
        const queries = [{ id: 'q1' }];
        const processor = vi.fn().mockResolvedValue({
          data: { test: true },
        });

        const result = await executeBulkOperation(queries, processor, {
          toolName,
        });

        expect(result.isError).toBe(false);
        const responseText = getTextContent(result.content);
        expect(responseText).toContain('id: "q1"');
      }
    });

    it('should work without keysPriority', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockResolvedValue({
        files: [{ path: 'test.ts' }],
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('files:');
      expect(responseText).toContain('path: "test.ts"');
    });
  });

  describe('Empty queries array', () => {
    it('should handle empty array gracefully', async () => {
      const queries: Array<{ id: string }> = [];
      const processor = vi.fn();

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      expect(result.isError).toBe(false);
      expect(processor).not.toHaveBeenCalled();

      const responseText = getTextContent(result.content);
      expect(responseText.trim()).toBe('results: []');
    });
  });

  describe('Query indexing', () => {
    it('should pass correct index to processor for each query', async () => {
      const queries = [{ id: 'q1' }, { id: 'q2' }, { id: 'q3' }];
      const processor = vi.fn().mockResolvedValue({
        data: { test: true },
      });

      await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      expect(processor).toHaveBeenCalledTimes(3);
      expect(processor).toHaveBeenNthCalledWith(1, queries[0], 0);
      expect(processor).toHaveBeenNthCalledWith(2, queries[1], 1);
      expect(processor).toHaveBeenNthCalledWith(3, queries[2], 2);
    });
  });

  describe('Error message preservation', () => {
    it('should preserve error messages from processor returning error status', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockResolvedValue({
        status: 'error' as const,
        error: 'Specific error: Repository not found',
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_FETCH_CONTENT,
      });

      const responseText = getTextContent(result.content);
      expect(responseText).toContain(
        'error: "Specific error: Repository not found"'
      );
    });

    it('should preserve error messages from thrown errors', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi
        .fn()
        .mockRejectedValue(new Error('Network error: Connection refused'));

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const responseText = getTextContent(result.content);
      expect(responseText).toContain(
        'error: "Network error: Connection refused"'
      );
    });

    it('should handle different error messages for different queries', async () => {
      const queries = [{ id: 'q1' }, { id: 'q2' }, { id: 'q3' }];
      const processor = vi
        .fn()
        .mockImplementation(async (query: { id: string }) => {
          throw new Error(`Error for ${query.id}`);
        });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      });

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('error: "Error for q1"');
      expect(responseText).toContain('error: "Error for q2"');
      expect(responseText).toContain('error: "Error for q3"');
    });
  });

  describe('Non-standard query field types', () => {
    it('should handle queries with non-string researchGoal gracefully', async () => {
      const queries = [
        {
          id: 'q1',

          researchGoal: 123 as any,
        },
      ];
      const processor = vi.fn().mockResolvedValue({
        data: { test: true },
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).not.toContain('status: "hasResults"');
      expect(responseText).not.toContain('researchGoal: 123');
    });

    it('should handle queries with non-string reasoning gracefully', async () => {
      const queries = [
        {
          id: 'q1',

          reasoning: { nested: 'object' } as any,
        },
      ];
      const processor = vi.fn().mockResolvedValue({
        data: { test: true },
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).not.toContain('status: "hasResults"');
      expect(responseText).not.toContain('reasoning:');
    });

    it('should handle queries with non-array researchSuggestions gracefully', async () => {
      const queries = [
        {
          id: 'q1',

          researchSuggestions: 'not an array' as any,
        },
      ];
      const processor = vi.fn().mockResolvedValue({
        data: { test: true },
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).not.toContain('status: "hasResults"');
    });
  });

  describe('Response field ordering', () => {
    it('should output numeric id before status in response', async () => {
      const queries = [
        {
          id: 'q1',
          mainResearchGoal: 'Understand authentication flow',
        },
      ];
      const processor = vi.fn().mockResolvedValue({
        status: 'empty' as const,
        data: { test: true },
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const responseText = getTextContent(result.content);
      const idIndex = responseText.indexOf('id: "q1"');
      const statusIndex = responseText.indexOf('status:');

      expect(idIndex).toBeGreaterThan(-1);
      expect(statusIndex).toBeGreaterThan(-1);
      expect(idIndex).toBeLessThan(statusIndex);
    });

    it('should return the caller-provided query id in the response', async () => {
      const queries = [
        {
          id: 'react_hooks_search',
          researchGoal: 'Find hooks',
          reasoning: 'Need stable mapping',
        },
      ];
      const processor = vi.fn().mockResolvedValue({
        data: { test: true },
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "react_hooks_search"');
    });

    it('should generate qN ids when the caller omits query ids', async () => {
      const queries = [
        { researchGoal: 'Find hooks', reasoning: 'First query' },
        { researchGoal: 'Find refs', reasoning: 'Second query' },
      ];
      const processor = vi.fn().mockResolvedValue({
        data: { test: true },
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('id: "q2"');
    });

    it('should output status before data in response', async () => {
      const queries = [
        {
          id: 'q1',
          researchGoal: 'Find implementations',
        },
      ];
      const processor = vi.fn().mockResolvedValue({
        status: 'empty' as const,
        data: { test: true },
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const responseText = getTextContent(result.content);
      const statusIndex = responseText.indexOf('status:');
      const dataIndex = responseText.indexOf('data:');

      expect(statusIndex).toBeGreaterThan(-1);
      expect(dataIndex).toBeGreaterThan(-1);
      expect(statusIndex).toBeLessThan(dataIndex);
    });

    it('should not include request metadata fields in responses', async () => {
      const queries = [
        {
          id: 'q1',
          mainResearchGoal: 'Main goal',
          researchGoal: 'Specific goal',
          reasoning: 'The reasoning',
        },
      ];
      const processor = vi.fn().mockResolvedValue({
        status: 'empty' as const,
        files: [{ path: 'test.ts' }],
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const responseText = getTextContent(result.content);
      const idIndex = responseText.indexOf('id:');
      const statusIndex = responseText.indexOf('status:');
      const dataIndex = responseText.indexOf('data:');

      expect(idIndex).toBeGreaterThan(-1);
      expect(statusIndex).toBeGreaterThan(-1);
      expect(dataIndex).toBeGreaterThan(-1);
      expect(idIndex).toBeLessThan(statusIndex);
      expect(statusIndex).toBeLessThan(dataIndex);
      expect(responseText).not.toContain('mainResearchGoal:');
      expect(responseText).not.toContain('researchGoal:');
      expect(responseText).not.toContain('reasoning:');
    });

    it('should maintain id -> status -> data ordering in error responses', async () => {
      const queries = [
        {
          id: 'q1',
          mainResearchGoal: 'Main goal',
          researchGoal: 'Specific goal',
          reasoning: 'The reasoning',
        },
      ];
      const processor = vi.fn().mockResolvedValue({
        status: 'error' as const,
        error: 'Something went wrong',
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const responseText = getTextContent(result.content);
      const idIndex = responseText.indexOf('id:');
      const statusIndex = responseText.indexOf('status:');
      const dataIndex = responseText.indexOf('data:');

      expect(idIndex).toBeLessThan(statusIndex);
      expect(statusIndex).toBeLessThan(dataIndex);
      expect(responseText).not.toContain('mainResearchGoal:');
      expect(responseText).not.toContain('researchGoal:');
      expect(responseText).not.toContain('reasoning:');
    });

    it('should maintain id -> status -> data ordering with thrown errors', async () => {
      const queries = [
        {
          id: 'q1',
          mainResearchGoal: 'Main goal',
          researchGoal: 'Specific goal',
          reasoning: 'The reasoning',
        },
      ];
      const processor = vi.fn().mockRejectedValue(new Error('Network error'));

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const responseText = getTextContent(result.content);
      const idIndex = responseText.indexOf('id:');
      const statusIndex = responseText.indexOf('status:');
      const dataIndex = responseText.indexOf('data:');

      expect(idIndex).toBeLessThan(statusIndex);
      expect(statusIndex).toBeLessThan(dataIndex);
      expect(responseText).not.toContain('mainResearchGoal:');
      expect(responseText).not.toContain('researchGoal:');
      expect(responseText).not.toContain('reasoning:');
    });
  });

  describe('Parallel processing', () => {
    it('should process queries in parallel', async () => {
      const queries = [{ id: 'q1' }, { id: 'q2' }, { id: 'q3' }];
      const processingOrder: number[] = [];
      const processor = vi
        .fn()
        .mockImplementation(async (_query: any, index: number) => {
          processingOrder.push(index);
          await new Promise(resolve => setTimeout(resolve, 10));
          return {
            data: { processed: true },
          };
        });

      await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      expect(processor).toHaveBeenCalledTimes(3);
      expect(processingOrder).toHaveLength(3);
    });
  });

  describe('Edge cases for error hints aggregation', () => {
    it('should aggregate error hints from processor returning error status with hints array', async () => {
      const queries = [
        { id: 'q1', mainResearchGoal: 'Goal 1' },
        { id: 'q2', mainResearchGoal: 'Goal 2' },
      ];
      const processor = vi.fn().mockImplementation(async () => ({
        status: 'error' as const,
        error: 'API rate limit exceeded',
        hints: ['Wait 60 seconds', 'Use authentication token'],
      }));

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_FETCH_CONTENT,
      });

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('Wait 60 seconds');
      expect(responseText).toContain('Use authentication token');
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('id: "q2"');
    });

    it('should collect unique error hints from multiple error results', async () => {
      const queries = [{ id: 'q1' }, { id: 'q2' }, { id: 'q3' }];
      const processor = vi
        .fn()
        .mockResolvedValueOnce({
          status: 'error' as const,
          error: 'Error 1',
          hints: ['Hint A', 'Hint B'],
        })
        .mockResolvedValueOnce({
          status: 'error' as const,
          error: 'Error 2',
          hints: ['Hint B', 'Hint C'],
        })
        .mockResolvedValueOnce({
          status: 'error' as const,
          error: 'Error 3',
          hints: ['Hint A', 'Hint D'],
        });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('id: "q3"');
      const hintAMatches = (responseText.match(/Hint A/g) || []).length;
      const hintBMatches = (responseText.match(/Hint B/g) || []).length;
      expect(hintAMatches).toBe(2);
      expect(hintBMatches).toBe(2);
    });

    it('should handle error status without hints array', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockResolvedValue({
        status: 'error' as const,
        error: 'Simple error without hints',
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('Simple error without hints');
    });

    it('should handle mixed hasResults, empty, and error with hints from all', async () => {
      const queries = [
        { id: 'q1', type: 'hasResults' },
        { id: 'q2', type: 'empty' },
        { id: 'q3', type: 'error' },
      ];
      const processor = vi
        .fn()
        .mockImplementation(async (query: { id: string; type: string }) => {
          if (query.type === 'hasResults') {
            return {
              data: { result: true },
              hints: ['Success hint from processor'],
            };
          }
          if (query.type === 'empty') {
            return {
              status: 'empty' as const,
              files: [],
              hints: ['Empty hint from processor'],
            };
          }
          return {
            status: 'error' as const,
            error: 'Error from processor',
            hints: ['Error hint from processor'],
          };
        });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('id: "q3"');
      expect(responseText).toContain('Success hint from processor');
      expect(responseText).toContain('Empty hint from processor');
      expect(responseText).toContain('Error hint from processor');
    });
  });

  describe('Error handling with invalid query indices', () => {
    it('should handle errors with queryIndex out of bounds gracefully', async () => {
      const queries = [{ id: 'q1' }];

      const processor = vi.fn().mockImplementation(async () => {
        throw new Error('Processor threw an error');
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('error:');
    });

    it('should handle all queries throwing errors simultaneously', async () => {
      const queries = [
        { id: 'q1', mainResearchGoal: 'Goal 1' },
        { id: 'q2', mainResearchGoal: 'Goal 2' },
        { id: 'q3', mainResearchGoal: 'Goal 3' },
      ];

      const processor = vi
        .fn()
        .mockImplementation(async (query: { id: string }) => {
          throw new Error(`Error processing ${query.id}`);
        });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_FETCH_CONTENT,
      });

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('id: "q3"');
      expect(responseText).toContain('error: "Error processing q1"');
      expect(responseText).toContain('error: "Error processing q2"');
      expect(responseText).toContain('error: "Error processing q3"');
      expect(responseText).not.toContain('mainResearchGoal:');
    });
  });

  describe('Output size limiting', () => {
    it('returns the full response when output is large (pagination handled externally)', async () => {
      const queries = [{ id: 'q1' }];
      const largeContent = 'x'.repeat(500);
      const processor = vi.fn().mockResolvedValue({
        items: Array.from({ length: 50 }, (_, i) => ({
          id: i,
          name: `item-${i}`,
          description: largeContent,
          metadata: { field1: largeContent, field2: largeContent },
        })),
        hints: ['Test hint'],
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        keysPriority: ['items'],
      });

      expect(result.isError).toBe(false);
    });

    it('should not paginate small responses', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockResolvedValue({
        data: { test: true },
        hints: ['Test hint'],
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const responseText = getTextContent(result.content);

      expect(responseText).not.toContain('Auto-paginated');
    });

    it('returns large responses without modification (pagination is a separate layer)', async () => {
      const queries = [{ id: 'q1' }];
      const largeContent = 'x'.repeat(1000);
      const processor = vi.fn().mockResolvedValue({
        items: Array.from({ length: 30 }, (_, i) => ({
          id: i,
          content: largeContent,
        })),
        hints: ['Test hint'],
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        keysPriority: ['items'],
      });

      expect(result.isError).toBe(false);
    });
  });

  describe('Status condition branches', () => {
    it('should correctly handle error status branch for hints aggregation', async () => {
      const queries = [{ id: 'q1' }, { id: 'q2' }];

      const processor = vi
        .fn()
        .mockResolvedValueOnce({
          data: { success: true },
          hints: ['Success hint'],
        })
        .mockResolvedValueOnce({
          status: 'error' as const,
          error: 'Something went wrong',
          hints: ['Error recovery hint'],
        });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('id: "q2"');
      expect(responseText).toContain('Success hint');
      expect(responseText).toContain('Error recovery hint');
    });

    it('should handle error status without any hints (fallback to generic)', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockResolvedValue({
        status: 'error' as const,
        error: 'Generic error',
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('Generic error');
    });

    it('should handle error status with empty hints array', async () => {
      const queries = [{ id: 'q1' }];
      const processor = vi.fn().mockResolvedValue({
        status: 'error' as const,
        error: 'Error with empty hints',
        hints: [],
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      });

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "q1"');
      expect(responseText).toContain('Error with empty hints');
    });
  });
});

describe('OCTOCODE_BULK_QUERY_TIMEOUT_MS', () => {
  const originalEnv = process.env.OCTOCODE_BULK_QUERY_TIMEOUT_MS;

  afterEach(() => {
    if (originalEnv !== undefined) {
      process.env.OCTOCODE_BULK_QUERY_TIMEOUT_MS = originalEnv;
    } else {
      delete process.env.OCTOCODE_BULK_QUERY_TIMEOUT_MS;
    }
    vi.restoreAllMocks();
  });

  it('should default to 60000ms when env var is not set', async () => {
    delete process.env.OCTOCODE_BULK_QUERY_TIMEOUT_MS;
    vi.resetModules();
    const { executeBulkOperation: freshBulk } =
      await import('../../src/utils/response/bulk.js');
    expect(freshBulk).toBeDefined();
  });

  it('should parse custom timeout from env var', async () => {
    process.env.OCTOCODE_BULK_QUERY_TIMEOUT_MS = '120000';
    vi.resetModules();
    const mod = await import('../../src/utils/response/bulk.js');
    expect(mod.executeBulkOperation).toBeDefined();
  });

  it('should fall back to 60000ms for invalid env var', async () => {
    process.env.OCTOCODE_BULK_QUERY_TIMEOUT_MS = 'not-a-number';
    vi.resetModules();
    const mod = await import('../../src/utils/response/bulk.js');
    expect(mod.executeBulkOperation).toBeDefined();
  });
});

describe('computeQueryTimeout (concurrency-aware)', () => {
  it('should be exported for testing', async () => {
    const { computeQueryTimeout } =
      await import('../../src/utils/response/bulk.js');
    expect(typeof computeQueryTimeout).toBe('function');
  });

  it('should return full budget for single query', async () => {
    const { computeQueryTimeout } =
      await import('../../src/utils/response/bulk.js');
    const result = computeQueryTimeout(1, 3);
    expect(result).toBeGreaterThanOrEqual(60000);
  });

  it('should give full budget when concurrency >= queryCount (parallel)', async () => {
    const { computeQueryTimeout } =
      await import('../../src/utils/response/bulk.js');
    const result = computeQueryTimeout(2, 3);
    expect(result).toBeGreaterThanOrEqual(60000);
  });

  it('should divide budget by batches when concurrency < queryCount', async () => {
    const { computeQueryTimeout } =
      await import('../../src/utils/response/bulk.js');
    const result = computeQueryTimeout(5, 3);
    expect(result).toBeLessThanOrEqual(30000);
    expect(result).toBeGreaterThanOrEqual(5000);
  });

  it('should respect minQueryTimeoutMs when higher than computed', async () => {
    const { computeQueryTimeout } =
      await import('../../src/utils/response/bulk.js');
    const result = computeQueryTimeout(5, 3, 45000);
    expect(result).toBe(45000);
  });

  it('should NOT lower timeout when minQueryTimeoutMs is below computed', async () => {
    const { computeQueryTimeout } =
      await import('../../src/utils/response/bulk.js');
    const result = computeQueryTimeout(2, 3, 30000);
    expect(result).toBeGreaterThanOrEqual(60000);
  });
});
