import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  createMockMcpServer,
  type MockMcpServer,
} from '../fixtures/mcp-fixtures.js';
import { expectHasResultsData, getSingleResult } from '../flows/assertions.js';
import {
  LocalFindFilesDataSchema,
  LocalFindFilesOutputSchema as UpstreamLocalFindFilesOutputSchema,
  LocalGetFileContentDataSchema,
  LocalGetFileContentOutputSchema as UpstreamLocalGetFileContentOutputSchema,
  LocalSearchCodeDataSchema,
  LocalSearchCodeOutputSchema as UpstreamLocalSearchCodeOutputSchema,
  LocalViewStructureDataSchema,
  LocalViewStructureOutputSchema as UpstreamLocalViewStructureOutputSchema,
} from '@octocodeai/octocode-core';
import { withResponseEnvelope } from '../../src/scheme/responseEnvelope.js';

const LocalFindFilesOutputSchema = withResponseEnvelope(
  UpstreamLocalFindFilesOutputSchema
);
const LocalGetFileContentOutputSchema = withResponseEnvelope(
  UpstreamLocalGetFileContentOutputSchema
);
const LocalSearchCodeOutputSchema = withResponseEnvelope(
  UpstreamLocalSearchCodeOutputSchema
);
const LocalViewStructureOutputSchema = withResponseEnvelope(
  UpstreamLocalViewStructureOutputSchema
);
import { registerLocalRipgrepTool } from '../../src/tools/local_ripgrep/register.js';
import { registerLocalViewStructureTool } from '../../src/tools/local_view_structure/register.js';
import { registerLocalFindFilesTool } from '../../src/tools/local_find_files/register.js';
import { registerLocalFetchContentTool } from '../../src/tools/local_fetch_content/register.js';
import { TOOL_NAMES } from '../../src/tools/toolMetadata/proxies.js';

const mockSearchContentRipgrep = vi.hoisted(() => vi.fn());
const mockViewStructure = vi.hoisted(() => vi.fn());
const mockFindFiles = vi.hoisted(() => vi.fn());
const mockFetchContent = vi.hoisted(() => vi.fn());

vi.mock('../../src/tools/local_ripgrep/searchContentRipgrep.js', () => ({
  searchContentRipgrep: (...args: unknown[]) =>
    mockSearchContentRipgrep(...args),
}));

vi.mock('../../src/tools/local_view_structure/local_view_structure.js', () => ({
  viewStructure: (...args: unknown[]) => mockViewStructure(...args),
}));

vi.mock('../../src/tools/local_find_files/findFiles.js', () => ({
  findFiles: (...args: unknown[]) => mockFindFiles(...args),
}));

vi.mock('../../src/tools/local_fetch_content/fetchContent.js', () => ({
  fetchContent: (...args: unknown[]) => mockFetchContent(...args),
}));

describe('local tool MCP pagination responses', () => {
  let mockServer: MockMcpServer;

  beforeEach(() => {
    vi.clearAllMocks();
    mockServer = createMockMcpServer();
  });

  it('localSearchCode returns schema-valid results with match data', async () => {
    registerLocalRipgrepTool(mockServer.server);

    const matchValue = 'match content';
    mockSearchContentRipgrep.mockResolvedValue({
      files: [
        {
          path: '/workspace/src/search.ts',
          matchCount: 1,
          matches: [
            {
              value: matchValue,
              line: 12,
              column: 0,
            },
          ],
        },
      ],
      searchEngine: 'rg',
      hints: ['search hint'],
    });

    const result = await mockServer.callTool(TOOL_NAMES.LOCAL_RIPGREP, {
      queries: [
        {
          id: 'local_search',
          researchGoal: 'Find local matches',
          reasoning: 'Verify actual MCP output for ripgrep',
          pattern: 'match',
          path: '/workspace',
        },
      ],
    });

    const data = expectHasResultsData(
      LocalSearchCodeOutputSchema,
      LocalSearchCodeDataSchema,
      result
    );

    expect(data.files?.[0]?.matches?.[0]?.value).toBe(matchValue);
  });

  it('localViewStructure returns schema-valid structured content', async () => {
    registerLocalViewStructureTool(mockServer.server);

    mockViewStructure.mockResolvedValue({
      entries: Array.from({ length: 5 }, (_, index) => ({
        name: `entry-${index}`,
        type: 'file',
        depth: 0,
      })),
      summary: '5 entries',
      hints: ['view hint'],
    });

    const result = await mockServer.callTool(TOOL_NAMES.LOCAL_VIEW_STRUCTURE, {
      queries: [
        {
          id: 'view_tree',
          researchGoal: 'Inspect local structure',
          reasoning: 'Verify actual MCP response for local structure',
          path: '/workspace',
        },
      ],
    });

    const data = expectHasResultsData(
      LocalViewStructureOutputSchema,
      LocalViewStructureDataSchema,
      result
    );

    expect(data.entries?.length).toBe(5);
  });

  it('localFindFiles returns schema-valid results', async () => {
    registerLocalFindFilesTool(mockServer.server);

    mockFindFiles.mockResolvedValue({
      files: [
        {
          path: '/workspace/src/a.ts',
          type: 'file',
        },
        {
          path: '/workspace/src/b.ts',
          type: 'file',
        },
      ],
      pagination: {
        currentPage: 1,
        totalPages: 2,
        hasMore: true,
        totalItems: 4,
      },
      hints: ['find hint'],
    });

    const result = await mockServer.callTool(TOOL_NAMES.LOCAL_FIND_FILES, {
      queries: [
        {
          id: 'find_files',
          researchGoal: 'Find local files',
          reasoning: 'Verify MCP response for localFindFiles',
          path: '/workspace',
        },
      ],
    });

    const data = expectHasResultsData(
      LocalFindFilesOutputSchema,
      LocalFindFilesDataSchema,
      result
    );

    expect(data.files?.length).toBe(2);
  });

  it('localGetFileContent preserves content pagination on actual MCP responses', async () => {
    registerLocalFetchContentTool(mockServer.server);

    mockFetchContent.mockResolvedValue({
      content: 'export const value = 1;',
      isPartial: true,
      totalLines: 20,
      startLine: 1,
      endLine: 5,
      pagination: {
        currentPage: 1,
        totalPages: 3,
        hasMore: true,
        charOffset: 0,
        charLength: 24,
        totalChars: 72,
      },
      hints: ['fetch hint'],
    });

    const result = await mockServer.callTool(TOOL_NAMES.LOCAL_FETCH_CONTENT, {
      queries: [
        {
          id: 'fetch_local',
          researchGoal: 'Read a local file',
          reasoning: 'Verify MCP response preserves content pagination',
          path: '/workspace/src/file.ts',
        },
      ],
    });

    const data = expectHasResultsData(
      LocalGetFileContentOutputSchema,
      LocalGetFileContentDataSchema,
      result
    );

    expect(data.content).toBe('export const value = 1;');
    expect(data.pagination?.hasMore).toBe(true);
  });

  it('multi-query localViewStructure returns schema-valid results for both queries', async () => {
    registerLocalViewStructureTool(mockServer.server);

    mockViewStructure.mockImplementation(async (query: { id: string }) => ({
      entries: Array.from({ length: 3 }, (_, index) => ({
        name: `${query.id}-entry-${index}`,
        type: 'file',
        depth: 0,
      })),
      summary: `${query.id} summary`,
      hints: ['view hint'],
    }));

    const result = await mockServer.callTool(TOOL_NAMES.LOCAL_VIEW_STRUCTURE, {
      queries: [
        {
          id: 'tree_a',
          researchGoal: 'Inspect first tree',
          reasoning: 'Verify bulk response for first query',
          path: '/workspace/a',
        },
        {
          id: 'tree_b',
          researchGoal: 'Inspect second tree',
          reasoning: 'Verify bulk response for second query',
          path: '/workspace/b',
        },
      ],
    });

    const parsed = LocalViewStructureOutputSchema.parse(
      result.structuredContent
    );

    expect(parsed.results.length).toBe(2);

    const firstResult = getSingleResult(LocalViewStructureOutputSchema, {
      ...result,
      structuredContent: {
        results: parsed.results.slice(0, 1),
      },
    });
    expect(firstResult.id).toBeDefined();
  });
});
