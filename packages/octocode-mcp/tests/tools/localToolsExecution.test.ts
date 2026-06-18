import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import type { z } from 'zod';
import type { RipgrepQuerySchema } from '@octocodeai/octocode-core/schemas';
type RipgrepQuery = z.infer<typeof RipgrepQuerySchema>;

vi.mock('../../../octocode-tools-core/src/utils/response/bulk.js', () => ({
  executeBulkOperation: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'mocked result' }],
  }),
}));

vi.mock(
  '../../../octocode-tools-core/src/tools/local_fetch_content/fetchContent.js',
  () => ({
    fetchContent: vi.fn().mockResolvedValue({ status: 'success' }),
  })
);

vi.mock(
  '../../../octocode-tools-core/src/tools/local_find_files/findFiles.js',
  () => ({
    findFiles: vi.fn().mockResolvedValue({ status: 'success' }),
  })
);

vi.mock(
  '../../../octocode-tools-core/src/tools/local_ripgrep/searchContentRipgrep.js',
  () => ({
    searchContentRipgrep: vi.fn().mockResolvedValue({ status: 'success' }),
  })
);

vi.mock(
  '../../../octocode-tools-core/src/tools/local_view_structure/local_view_structure.js',
  () => ({
    viewStructure: vi.fn().mockResolvedValue({ status: 'success' }),
  })
);

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
  const { z } = await import('zod');
  const out = () => z.looseObject({});
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

vi.mock(
  '../../../octocode-tools-core/src/tools/local_ripgrep/scheme.js',
  async importOriginal => {
    const actual =
      await importOriginal<
        typeof import('../../../octocode-tools-core/src/tools/local_ripgrep/scheme.js')
      >();
    return {
      ...actual,
      LocalRipgrepQuerySchema: { safeParse: mockSafeParse },
      LocalRipgrepBulkQuerySchema: {},
    };
  }
);

vi.mock(
  '../../../octocode-tools-core/src/tools/local_find_files/scheme.js',
  async importOriginal => {
    const actual =
      await importOriginal<
        typeof import('../../../octocode-tools-core/src/tools/local_find_files/scheme.js')
      >();
    return {
      ...actual,
      LocalFindFilesQuerySchema: { safeParse: mockSafeParse },
      LocalFindFilesBulkQuerySchema: {},
    };
  }
);

vi.mock(
  '../../../octocode-tools-core/src/tools/local_view_structure/scheme.js',
  async importOriginal => {
    const actual =
      await importOriginal<
        typeof import('../../../octocode-tools-core/src/tools/local_view_structure/scheme.js')
      >();
    return {
      ...actual,
      LocalViewStructureQuerySchema: { safeParse: mockSafeParse },
      LocalViewStructureBulkQuerySchema: {},
    };
  }
);

vi.mock(
  '../../../octocode-tools-core/src/tools/local_fetch_content/scheme.js',
  async importOriginal => {
    const actual =
      await importOriginal<
        typeof import('../../../octocode-tools-core/src/tools/local_fetch_content/scheme.js')
      >();
    return {
      ...actual,
      LocalFetchContentQuerySchema: { safeParse: mockSafeParse },
      LocalFetchContentBulkQuerySchema: {},
    };
  }
);

describe('Local Tools Execution', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('executeFetchContent', () => {
    it('should call executeBulkOperation with queries', async () => {
      const { executeFetchContent } =
        await import('../../../octocode-tools-core/src/tools/local_fetch_content/execution.js');
      const { executeBulkOperation } =
        await import('../../../octocode-tools-core/src/utils/response/bulk.js');

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
        expect.objectContaining({ toolName: 'localGetFileContent' }),
        expect.any(Object)
      );
    });

    it('should pass fetchContent function as callback', async () => {
      const { executeFetchContent } =
        await import('../../../octocode-tools-core/src/tools/local_fetch_content/execution.js');
      const { executeBulkOperation } =
        await import('../../../octocode-tools-core/src/utils/response/bulk.js');
      const { fetchContent } =
        await import('../../../octocode-tools-core/src/tools/local_fetch_content/fetchContent.js');

      const queries = [
        {
          id: 'test',
          researchGoal: 'Test',
          reasoning: 'Schema validation',
          path: '/test/file.ts',
        },
      ];
      await executeFetchContent({ queries: queries as any });

      const mockCall = vi.mocked(executeBulkOperation).mock.calls[0];
      expect(mockCall).toBeDefined();
      const callback = mockCall![1];

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
        await import('../../../octocode-tools-core/src/tools/local_fetch_content/execution.js');
      const { executeBulkOperation } =
        await import('../../../octocode-tools-core/src/utils/response/bulk.js');

      await executeFetchContent({ queries: [] });

      expect(executeBulkOperation).toHaveBeenCalledWith(
        [],
        expect.any(Function),
        expect.objectContaining({ toolName: 'localGetFileContent' }),
        expect.any(Object)
      );
    });

    it('should handle undefined queries with fallback to empty array', async () => {
      const { executeFetchContent } =
        await import('../../../octocode-tools-core/src/tools/local_fetch_content/execution.js');
      const { executeBulkOperation } =
        await import('../../../octocode-tools-core/src/utils/response/bulk.js');

      await executeFetchContent({ queries: undefined } as unknown as Parameters<
        typeof executeFetchContent
      >[0]);

      expect(executeBulkOperation).toHaveBeenCalledWith(
        [],
        expect.any(Function),
        expect.objectContaining({ toolName: 'localGetFileContent' }),
        expect.any(Object)
      );
    });

    it('should not cache callback responses', async () => {
      const { executeFetchContent } =
        await import('../../../octocode-tools-core/src/tools/local_fetch_content/execution.js');
      const { executeBulkOperation } =
        await import('../../../octocode-tools-core/src/utils/response/bulk.js');
      const { fetchContent } =
        await import('../../../octocode-tools-core/src/tools/local_fetch_content/fetchContent.js');
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
        await import('../../../octocode-tools-core/src/tools/local_find_files/execution.js');
      const { executeBulkOperation } =
        await import('../../../octocode-tools-core/src/utils/response/bulk.js');

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
        expect.objectContaining({ toolName: 'localFindFiles' }),
        expect.any(Object)
      );
    });

    it('should pass findFiles function as callback', async () => {
      const { executeFindFiles } =
        await import('../../../octocode-tools-core/src/tools/local_find_files/execution.js');
      const { executeBulkOperation } =
        await import('../../../octocode-tools-core/src/utils/response/bulk.js');
      const { findFiles } =
        await import('../../../octocode-tools-core/src/tools/local_find_files/findFiles.js');

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
        await import('../../../octocode-tools-core/src/tools/local_find_files/execution.js');
      const { executeBulkOperation } =
        await import('../../../octocode-tools-core/src/utils/response/bulk.js');

      await executeFindFiles({ queries: undefined } as unknown as Parameters<
        typeof executeFindFiles
      >[0]);

      expect(executeBulkOperation).toHaveBeenCalledWith(
        [],
        expect.any(Function),
        expect.objectContaining({ toolName: 'localFindFiles' }),
        expect.any(Object)
      );
    });

    it('should catch thrown errors via executeWithToolBoundary', async () => {
      const { executeFindFiles } =
        await import('../../../octocode-tools-core/src/tools/local_find_files/execution.js');
      const { executeBulkOperation } =
        await import('../../../octocode-tools-core/src/utils/response/bulk.js');
      const { findFiles } =
        await import('../../../octocode-tools-core/src/tools/local_find_files/findFiles.js');

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
  });

  describe('executeRipgrepSearch', () => {
    it('should call executeBulkOperation with queries', async () => {
      const { executeRipgrepSearch } =
        await import('../../../octocode-tools-core/src/tools/local_ripgrep/execution.js');
      const { executeBulkOperation } =
        await import('../../../octocode-tools-core/src/utils/response/bulk.js');

      const queries = [
        {
          researchGoal: 'Test',
          reasoning: 'Schema validation',
          keywords: 'test',
          path: '/test',
        },
      ] as RipgrepQuery[];
      await executeRipgrepSearch({ queries });

      expect(executeBulkOperation).toHaveBeenCalledWith(
        queries,
        expect.any(Function),
        expect.objectContaining({ toolName: 'localSearchCode' }),
        expect.any(Object)
      );
    });

    it('should pass searchContentRipgrep function as callback', async () => {
      const { executeRipgrepSearch } =
        await import('../../../octocode-tools-core/src/tools/local_ripgrep/execution.js');
      const { executeBulkOperation } =
        await import('../../../octocode-tools-core/src/utils/response/bulk.js');
      const { searchContentRipgrep } =
        await import('../../../octocode-tools-core/src/tools/local_ripgrep/searchContentRipgrep.js');

      const queries = [
        {
          researchGoal: 'Test',
          reasoning: 'Schema validation',
          keywords: 'test',
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
        keywords: 'test',
        path: '/test',
      } as RipgrepQuery;
      await callback(query, 0);

      expect(searchContentRipgrep).toHaveBeenCalledWith(
        withParsedDefaults(query)
      );
    });

    it('should handle undefined queries with fallback to empty array', async () => {
      const { executeRipgrepSearch } =
        await import('../../../octocode-tools-core/src/tools/local_ripgrep/execution.js');
      const { executeBulkOperation } =
        await import('../../../octocode-tools-core/src/utils/response/bulk.js');

      await executeRipgrepSearch({
        queries: undefined,
      } as unknown as Parameters<typeof executeRipgrepSearch>[0]);

      expect(executeBulkOperation).toHaveBeenCalledWith(
        [],
        expect.any(Function),
        expect.objectContaining({ toolName: 'localSearchCode' }),
        expect.any(Object)
      );
    });
  });

  describe('executeViewStructure', () => {
    it('should call executeBulkOperation with queries', async () => {
      const { executeViewStructure } =
        await import('../../../octocode-tools-core/src/tools/local_view_structure/execution.js');
      const { executeBulkOperation } =
        await import('../../../octocode-tools-core/src/utils/response/bulk.js');

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
        expect.objectContaining({ toolName: 'localViewStructure' }),
        expect.any(Object)
      );
    });

    it('should pass viewStructure function as callback', async () => {
      const { executeViewStructure } =
        await import('../../../octocode-tools-core/src/tools/local_view_structure/execution.js');
      const { executeBulkOperation } =
        await import('../../../octocode-tools-core/src/utils/response/bulk.js');
      const { viewStructure } =
        await import('../../../octocode-tools-core/src/tools/local_view_structure/local_view_structure.js');

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
        await import('../../../octocode-tools-core/src/tools/local_view_structure/execution.js');
      const { executeBulkOperation } =
        await import('../../../octocode-tools-core/src/utils/response/bulk.js');

      await executeViewStructure({
        queries: undefined,
      } as unknown as Parameters<typeof executeViewStructure>[0]);

      expect(executeBulkOperation).toHaveBeenCalledWith(
        [],
        expect.any(Function),
        expect.objectContaining({ toolName: 'localViewStructure' }),
        expect.any(Object)
      );
    });

    it('should catch thrown errors via executeWithToolBoundary', async () => {
      const { executeViewStructure } =
        await import('../../../octocode-tools-core/src/tools/local_view_structure/execution.js');
      const { executeBulkOperation } =
        await import('../../../octocode-tools-core/src/utils/response/bulk.js');
      const { viewStructure } =
        await import('../../../octocode-tools-core/src/tools/local_view_structure/local_view_structure.js');

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
