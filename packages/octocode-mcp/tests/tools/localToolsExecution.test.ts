/**
 * Tests for local tools execution and registration modules
 * Covers execution.ts and register.ts files for local_fetch_content, local_find_files,
 * local_ripgrep, and local_view_structure
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import type { RipgrepQuery } from '@octocodeai/octocode-core';

// Mock the bulk operation module
vi.mock('../../src/utils/response/bulk.js', () => ({
  executeBulkOperation: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'mocked result' }],
  }),
}));

// Mock individual tool functions
vi.mock('../../src/tools/local_fetch_content/fetchContent.js', () => ({
  fetchContent: vi.fn().mockResolvedValue({ status: 'success' }),
}));

vi.mock('../../src/tools/local_find_files/findFiles.js', () => ({
  findFiles: vi.fn().mockResolvedValue({ status: 'success' }),
}));

vi.mock('../../src/tools/local_ripgrep/searchContentRipgrep.js', () => ({
  searchContentRipgrep: vi.fn().mockResolvedValue({ status: 'success' }),
}));

vi.mock('../../src/tools/local_view_structure/local_view_structure.js', () => ({
  viewStructure: vi.fn().mockResolvedValue({ status: 'success' }),
}));

// Mock schema modules so safeParse succeeds and visibly applies schema defaults.
// Execution handlers must pass validation.data to implementations, not the raw
// query, otherwise MCP clients that omit defaulted fields get inconsistent
// behavior.
const withParsedDefaults = <T extends object>(
  query: T
): T & {
  __schemaDefaultsApplied: true;
} => ({
  ...query,
  __schemaDefaultsApplied: true,
});
const mockSafeParse = (query: object) => ({
  success: true,
  data: withParsedDefaults(query),
});
vi.mock('@octocodeai/octocode-core', async importOriginal => {
  const { z } = await import('zod/v4');
  const out = () => z.object({}).passthrough();
  return {
    ...(await importOriginal<object>()),
    FetchContentQuerySchema: { safeParse: mockSafeParse },
    LOCAL_RIPGREP_DESCRIPTION: 'localSearchCode',
    LOCAL_FIND_FILES_DESCRIPTION: 'localFindFiles',
    LOCAL_VIEW_STRUCTURE_DESCRIPTION: 'localViewStructure',
    LOCAL_FETCH_CONTENT_DESCRIPTION: 'localGetFileContent',
    LocalSearchCodeOutputSchema: out(),
    LocalFindFilesOutputSchema: out(),
    LocalViewStructureOutputSchema: out(),
    LocalGetFileContentOutputSchema: out(),
  };
});

// localSchemaOverlay re-publishes the ripgrep/find/view schemas with relaxed
// caps. Stub the overlay so tests can verify orchestration without exercising Zod.
vi.mock('../../src/scheme/localSchemaOverlay.js', () => ({
  RipgrepQuerySchema: { safeParse: mockSafeParse },
  FindFilesQuerySchema: { safeParse: mockSafeParse },
  ViewStructureQuerySchema: { safeParse: mockSafeParse },
  FetchContentQuerySchema: { safeParse: mockSafeParse },
  BulkRipgrepQuerySchema: {},
  BulkFindFilesSchema: {},
  BulkViewStructureSchema: {},
  BulkFetchContentQuerySchema: {},
  VERBOSITY_VALUES: ['basic', 'compact', 'concise'] as const,
  verbosityField: {},
  isConcise: (_v: unknown) => false,
  conciseDrillBackHint: (_s: string) => [] as string[],
}));

// Verbosity helper module — stub helpers so handlers stay on the default
// (basic) path when tests don't pass a verbosity value.
vi.mock('../../src/scheme/verbosity.js', () => ({
  isConcise: (v: unknown) => v === 'concise',
  isCompact: (v: unknown) => v === 'compact',
  isBasic: (v: unknown) => v === undefined || v === 'basic',
  normalizeVerbosity: (v: unknown) => v ?? 'basic',
  conciseDrillBackHint: (_s: string) => [] as string[],
  compactTrimHints: (hints: string[]) => hints,
  makeAdvisoryPredicate:
    (_keywords: string[]) =>
    (_hint: string): boolean =>
      false,
  assertConcisePayload: () => undefined,
}));

describe('Local Tools Execution', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('executeFetchContent', () => {
    it('should call executeBulkOperation with queries', async () => {
      const { executeFetchContent } =
        await import('../../src/tools/local_fetch_content/execution.js');
      const { executeBulkOperation } =
        await import('../../src/utils/response/bulk.js');

      const queries = [
        {
          id: 'test',
          researchGoal: 'Test',
          reasoning: 'Schema validation',
          path: '/test/file.ts',
        },
      ];
      await executeFetchContent({ queries: queries as any });

      expect(executeBulkOperation).toHaveBeenCalledWith(
        queries,
        expect.any(Function),
        expect.objectContaining({ toolName: 'localGetFileContent' })
      );
    });

    it('should pass fetchContent function as callback', async () => {
      const { executeFetchContent } =
        await import('../../src/tools/local_fetch_content/execution.js');
      const { executeBulkOperation } =
        await import('../../src/utils/response/bulk.js');
      const { fetchContent } =
        await import('../../src/tools/local_fetch_content/fetchContent.js');

      const queries = [
        {
          id: 'test',
          researchGoal: 'Test',
          reasoning: 'Schema validation',
          path: '/test/file.ts',
        },
      ];
      await executeFetchContent({ queries: queries as any });

      // Get the callback function passed to executeBulkOperation
      const mockCall = vi.mocked(executeBulkOperation).mock.calls[0];
      expect(mockCall).toBeDefined();
      const callback = mockCall![1];

      // Execute the callback to cover line 16
      const query = {
        researchGoal: 'Test',
        reasoning: 'Schema validation',
        path: '/test',
      };
      await callback(query, 0);

      expect(fetchContent).toHaveBeenCalledWith(withParsedDefaults(query));
    });

    it('should handle empty queries array', async () => {
      const { executeFetchContent } =
        await import('../../src/tools/local_fetch_content/execution.js');
      const { executeBulkOperation } =
        await import('../../src/utils/response/bulk.js');

      await executeFetchContent({ queries: [] });

      expect(executeBulkOperation).toHaveBeenCalledWith(
        [],
        expect.any(Function),
        expect.objectContaining({ toolName: 'localGetFileContent' })
      );
    });

    it('should handle undefined queries with fallback to empty array', async () => {
      const { executeFetchContent } =
        await import('../../src/tools/local_fetch_content/execution.js');
      const { executeBulkOperation } =
        await import('../../src/utils/response/bulk.js');

      await executeFetchContent({ queries: undefined } as unknown as Parameters<
        typeof executeFetchContent
      >[0]);

      expect(executeBulkOperation).toHaveBeenCalledWith(
        [],
        expect.any(Function),
        expect.objectContaining({ toolName: 'localGetFileContent' })
      );
    });

    it('should not cache callback responses', async () => {
      const { executeFetchContent } =
        await import('../../src/tools/local_fetch_content/execution.js');
      const { executeBulkOperation } =
        await import('../../src/utils/response/bulk.js');
      const { fetchContent } =
        await import('../../src/tools/local_fetch_content/fetchContent.js');
      vi.mocked(fetchContent).mockResolvedValue({
        content: 'abc',
      } as any);

      const query = {
        researchGoal: 'Test',
        reasoning: 'Schema validation',
        path: '/test/file.ts',
      };
      await executeFetchContent({ queries: [query] as any });
      const callback = vi.mocked(executeBulkOperation).mock.calls[0]![1];
      await callback(query as any, 0);
      await callback(query as any, 0);

      expect(fetchContent).toHaveBeenCalledTimes(2);
    });
  });

  describe('executeFindFiles', () => {
    it('should call executeBulkOperation with queries', async () => {
      const { executeFindFiles } =
        await import('../../src/tools/local_find_files/execution.js');
      const { executeBulkOperation } =
        await import('../../src/utils/response/bulk.js');

      const queries = [
        {
          id: 'test',
          researchGoal: 'Test',
          reasoning: 'Schema validation',
          path: '/test',
        },
      ];
      await executeFindFiles({ queries: queries as any });

      expect(executeBulkOperation).toHaveBeenCalledWith(
        queries,
        expect.any(Function),
        expect.objectContaining({ toolName: 'localFindFiles' })
      );
    });

    it('should pass findFiles function as callback', async () => {
      const { executeFindFiles } =
        await import('../../src/tools/local_find_files/execution.js');
      const { executeBulkOperation } =
        await import('../../src/utils/response/bulk.js');
      const { findFiles } =
        await import('../../src/tools/local_find_files/findFiles.js');

      const queries = [
        {
          id: 'test',
          researchGoal: 'Test',
          reasoning: 'Schema validation',
          path: '/test',
        },
      ];
      await executeFindFiles({ queries: queries as any });

      const mockCall = vi.mocked(executeBulkOperation).mock.calls[0];
      expect(mockCall).toBeDefined();
      const callback = mockCall![1];

      const query = {
        researchGoal: 'Test',
        reasoning: 'Schema validation',
        path: '/test',
      };
      await callback(query, 0);

      expect(findFiles).toHaveBeenCalledWith(withParsedDefaults(query));
    });

    it('should handle undefined queries with fallback to empty array', async () => {
      const { executeFindFiles } =
        await import('../../src/tools/local_find_files/execution.js');
      const { executeBulkOperation } =
        await import('../../src/utils/response/bulk.js');

      await executeFindFiles({ queries: undefined } as unknown as Parameters<
        typeof executeFindFiles
      >[0]);

      expect(executeBulkOperation).toHaveBeenCalledWith(
        [],
        expect.any(Function),
        expect.objectContaining({ toolName: 'localFindFiles' })
      );
    });

    it('should catch thrown errors via executeWithToolBoundary', async () => {
      const { executeFindFiles } =
        await import('../../src/tools/local_find_files/execution.js');
      const { executeBulkOperation } =
        await import('../../src/utils/response/bulk.js');
      const { findFiles } =
        await import('../../src/tools/local_find_files/findFiles.js');

      vi.mocked(findFiles).mockRejectedValueOnce(
        new Error('Unexpected failure')
      );

      const query = {
        id: 'test',
        researchGoal: 'Test',
        reasoning: 'boundary test',
        path: '/test',
      };
      await executeFindFiles({ queries: [query] as any });

      const callback = vi.mocked(executeBulkOperation).mock.calls[0]![1];
      const result = await callback(query, 0);
      expect(result).toBeDefined();
      expect(result).toHaveProperty('status', 'error');
    });

    it('marks capped find results as incomplete evidence', async () => {
      const { executeFindFiles } =
        await import('../../src/tools/local_find_files/execution.js');
      const { executeBulkOperation } =
        await import('../../src/utils/response/bulk.js');
      const { findFiles } =
        await import('../../src/tools/local_find_files/findFiles.js');

      vi.mocked(findFiles).mockResolvedValueOnce({
        files: [{ path: '/test/a.ts' }],
        hints: ['Results capped at 5 of 14. Narrow filters or increase limit.'],
      } as any);

      const query = {
        id: 'test',
        researchGoal: 'Test',
        reasoning: 'capped evidence',
        path: '/test',
      };
      await executeFindFiles({ queries: [query] as any });

      const callback = vi.mocked(executeBulkOperation).mock.calls[0]![1];
      const result = await callback(query, 0);
      expect(result.evidence).toMatchObject({
        complete: false,
        confidence: 'medium',
      });
      expect(result.evidence?.reason).toContain('capped');
    });
  });

  describe('executeRipgrepSearch', () => {
    it('should call executeBulkOperation with queries', async () => {
      const { executeRipgrepSearch } =
        await import('../../src/tools/local_ripgrep/execution.js');
      const { executeBulkOperation } =
        await import('../../src/utils/response/bulk.js');

      const queries = [
        {
          researchGoal: 'Test',
          reasoning: 'Schema validation',
          pattern: 'test',
          path: '/test',
        },
      ] as RipgrepQuery[];
      await executeRipgrepSearch({ queries });

      expect(executeBulkOperation).toHaveBeenCalledWith(
        queries,
        expect.any(Function),
        expect.objectContaining({ toolName: 'localSearchCode' })
      );
    });

    it('should pass searchContentRipgrep function as callback', async () => {
      const { executeRipgrepSearch } =
        await import('../../src/tools/local_ripgrep/execution.js');
      const { executeBulkOperation } =
        await import('../../src/utils/response/bulk.js');
      const { searchContentRipgrep } =
        await import('../../src/tools/local_ripgrep/searchContentRipgrep.js');

      const queries = [
        {
          researchGoal: 'Test',
          reasoning: 'Schema validation',
          pattern: 'test',
          path: '/test',
        },
      ] as RipgrepQuery[];
      await executeRipgrepSearch({ queries });

      const mockCall = vi.mocked(executeBulkOperation).mock.calls[0];
      expect(mockCall).toBeDefined();
      const callback = mockCall![1];

      const query = {
        researchGoal: 'Test',
        reasoning: 'Schema validation',
        pattern: 'test',
        path: '/test',
      } as RipgrepQuery;
      await callback(query, 0);

      expect(searchContentRipgrep).toHaveBeenCalledWith(
        withParsedDefaults(query)
      );
    });

    it('should handle undefined queries with fallback to empty array', async () => {
      const { executeRipgrepSearch } =
        await import('../../src/tools/local_ripgrep/execution.js');
      const { executeBulkOperation } =
        await import('../../src/utils/response/bulk.js');

      await executeRipgrepSearch({
        queries: undefined,
      } as unknown as Parameters<typeof executeRipgrepSearch>[0]);

      expect(executeBulkOperation).toHaveBeenCalledWith(
        [],
        expect.any(Function),
        expect.objectContaining({ toolName: 'localSearchCode' })
      );
    });

    it('marks limited ripgrep results as incomplete evidence', async () => {
      const { executeRipgrepSearch } =
        await import('../../src/tools/local_ripgrep/execution.js');
      const { executeBulkOperation } =
        await import('../../src/utils/response/bulk.js');
      const { searchContentRipgrep } =
        await import('../../src/tools/local_ripgrep/searchContentRipgrep.js');

      vi.mocked(searchContentRipgrep).mockResolvedValueOnce({
        files: [{ path: '/test/a.ts', matches: [] }],
        hints: ['Results limited to 10 files (found 17 matching)'],
      } as any);

      const query = {
        researchGoal: 'Test',
        reasoning: 'limited evidence',
        pattern: 'test',
        path: '/test',
      } as RipgrepQuery;
      await executeRipgrepSearch({ queries: [query] });

      const callback = vi.mocked(executeBulkOperation).mock.calls[0]![1];
      const result = await callback(query, 0);
      expect(result.evidence).toMatchObject({
        complete: false,
        confidence: 'medium',
      });
      expect(result.evidence?.reason).toContain('limited');
    });
  });

  describe('executeViewStructure', () => {
    it('should call executeBulkOperation with queries', async () => {
      const { executeViewStructure } =
        await import('../../src/tools/local_view_structure/execution.js');
      const { executeBulkOperation } =
        await import('../../src/utils/response/bulk.js');

      const queries = [
        {
          id: 'test',
          researchGoal: 'Test',
          reasoning: 'Schema validation',
          path: '/test',
        },
      ];
      await executeViewStructure({ queries: queries as any });

      expect(executeBulkOperation).toHaveBeenCalledWith(
        queries,
        expect.any(Function),
        expect.objectContaining({ toolName: 'localViewStructure' })
      );
    });

    it('should pass viewStructure function as callback', async () => {
      const { executeViewStructure } =
        await import('../../src/tools/local_view_structure/execution.js');
      const { executeBulkOperation } =
        await import('../../src/utils/response/bulk.js');
      const { viewStructure } =
        await import('../../src/tools/local_view_structure/local_view_structure.js');

      const queries = [
        {
          id: 'test',
          researchGoal: 'Test',
          reasoning: 'Schema validation',
          path: '/test',
        },
      ];
      await executeViewStructure({ queries: queries as any });

      const mockCall = vi.mocked(executeBulkOperation).mock.calls[0];
      expect(mockCall).toBeDefined();
      const callback = mockCall![1];

      const query = {
        researchGoal: 'Test',
        reasoning: 'Schema validation',
        path: '/test',
      };
      await callback(query, 0);

      expect(viewStructure).toHaveBeenCalledWith(withParsedDefaults(query));
    });

    it('should handle undefined queries with fallback to empty array', async () => {
      const { executeViewStructure } =
        await import('../../src/tools/local_view_structure/execution.js');
      const { executeBulkOperation } =
        await import('../../src/utils/response/bulk.js');

      await executeViewStructure({
        queries: undefined,
      } as unknown as Parameters<typeof executeViewStructure>[0]);

      expect(executeBulkOperation).toHaveBeenCalledWith(
        [],
        expect.any(Function),
        expect.objectContaining({ toolName: 'localViewStructure' })
      );
    });

    it('should catch thrown errors via executeWithToolBoundary', async () => {
      const { executeViewStructure } =
        await import('../../src/tools/local_view_structure/execution.js');
      const { executeBulkOperation } =
        await import('../../src/utils/response/bulk.js');
      const { viewStructure } =
        await import('../../src/tools/local_view_structure/local_view_structure.js');

      vi.mocked(viewStructure).mockRejectedValueOnce(
        new Error('Unexpected failure')
      );

      const query = {
        id: 'test',
        researchGoal: 'Test',
        reasoning: 'boundary test',
        path: '/test',
      };
      await executeViewStructure({ queries: [query] as any });

      const callback = vi.mocked(executeBulkOperation).mock.calls[0]![1];
      const result = await callback(query, 0);
      expect(result).toBeDefined();
      expect(result).toHaveProperty('status', 'error');
    });
  });
});

describe('Local Tools Registration', () => {
  const createMockServer = () => {
    return {
      registerTool: vi.fn().mockReturnValue(undefined),
    } as unknown as McpServer;
  };

  describe('registerLocalFetchContentTool', () => {
    it('should register the tool with correct name and schema', async () => {
      const { registerLocalFetchContentTool } =
        await import('../../src/tools/local_fetch_content/register.js');

      const mockServer = createMockServer();
      registerLocalFetchContentTool(mockServer);

      expect(mockServer.registerTool).toHaveBeenCalledWith(
        'localGetFileContent',
        expect.objectContaining({
          description: expect.any(String),
          inputSchema: expect.any(Object),
          annotations: expect.objectContaining({
            title: 'Local Fetch Content',
            readOnlyHint: true,
            destructiveHint: false,
            idempotentHint: true,
            openWorldHint: false,
          }),
        }),
        expect.any(Function)
      );
    });
  });

  describe('registerLocalFindFilesTool', () => {
    it('should register the tool with correct name and schema', async () => {
      const { registerLocalFindFilesTool } =
        await import('../../src/tools/local_find_files/register.js');

      const mockServer = createMockServer();
      registerLocalFindFilesTool(mockServer);

      expect(mockServer.registerTool).toHaveBeenCalledWith(
        'localFindFiles',
        expect.objectContaining({
          description: expect.any(String),
          inputSchema: expect.any(Object),
          annotations: expect.objectContaining({
            title: 'Local Find Files',
            readOnlyHint: true,
            destructiveHint: false,
            idempotentHint: true,
            openWorldHint: false,
          }),
        }),
        expect.any(Function)
      );
    });
  });

  describe('registerLocalRipgrepTool', () => {
    it('should register the tool with correct name and schema', async () => {
      const { registerLocalRipgrepTool } =
        await import('../../src/tools/local_ripgrep/register.js');

      const mockServer = createMockServer();
      registerLocalRipgrepTool(mockServer);

      expect(mockServer.registerTool).toHaveBeenCalledWith(
        'localSearchCode',
        expect.objectContaining({
          description: expect.any(String),
          inputSchema: expect.any(Object),
          annotations: expect.objectContaining({
            title: 'Local Ripgrep Search',
            readOnlyHint: true,
            destructiveHint: false,
            idempotentHint: true,
            openWorldHint: false,
          }),
        }),
        expect.any(Function)
      );
    });
  });
});
