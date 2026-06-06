import { beforeEach, describe, expect, it, vi } from 'vitest';
import { incrementToolCharSavings } from 'octocode-shared';
import { TOOL_NAMES } from '../../src/tools/toolMetadata/proxies.js';
import { attachRawResponseChars } from '../../src/utils/response/charSavings.js';
import { createMockMcpServer } from '../fixtures/mcp-fixtures.js';

const mockSearchContentRipgrep = vi.hoisted(() => vi.fn());
const mockViewStructure = vi.hoisted(() => vi.fn());
const mockFindFiles = vi.hoisted(() => vi.fn());
const mockFetchContent = vi.hoisted(() => vi.fn());
const mockFindReferences = vi.hoisted(() => vi.fn());
const mockProcessCallHierarchy = vi.hoisted(() => vi.fn());

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

vi.mock('../../src/tools/lsp_find_references/lsp_find_references.js', () => ({
  findReferences: (...args: unknown[]) => mockFindReferences(...args),
}));

vi.mock('../../src/tools/lsp_call_hierarchy/callHierarchy.js', () => ({
  processCallHierarchy: (...args: unknown[]) =>
    mockProcessCallHierarchy(...args),
}));

import { registerLocalRipgrepTool } from '../../src/tools/local_ripgrep/register.js';
import { registerLocalViewStructureTool } from '../../src/tools/local_view_structure/register.js';
import { registerLocalFindFilesTool } from '../../src/tools/local_find_files/register.js';
import { registerLocalFetchContentTool } from '../../src/tools/local_fetch_content/register.js';
import { registerLSPFindReferencesTool } from '../../src/tools/lsp_find_references/register.js';
import { registerLSPCallHierarchyTool } from '../../src/tools/lsp_call_hierarchy/register.js';

const RAW_BY_TOOL: Record<string, number> = {
  [TOOL_NAMES.LOCAL_RIPGREP]: 11_111,
  [TOOL_NAMES.LOCAL_VIEW_STRUCTURE]: 22_222,
  [TOOL_NAMES.LOCAL_FIND_FILES]: 33_333,
  [TOOL_NAMES.LOCAL_FETCH_CONTENT]: 44_444,
  [TOOL_NAMES.LSP_FIND_REFERENCES]: 55_555,
  [TOOL_NAMES.LSP_CALL_HIERARCHY]: 66_666,
};

describe('local + LSP tool stats runtime contract', () => {
  beforeEach(() => {
    vi.clearAllMocks();

    mockSearchContentRipgrep.mockResolvedValue(
      attachRawResponseChars(
        {
          files: [
            {
              path: '/workspace/src/a.ts',
              matchCount: 1,
              matches: [{ value: 'hit', line: 1, column: 0 }],
            },
          ],
          searchEngine: 'rg',
          hints: ['search hint'],
        },
        RAW_BY_TOOL[TOOL_NAMES.LOCAL_RIPGREP]
      )
    );

    mockViewStructure.mockResolvedValue(
      attachRawResponseChars(
        {
          path: '/workspace',
          depth: 1,
          entries: [{ name: 'src', type: 'directory' }],
          summary: { totalFiles: 0, totalFolders: 1, truncated: false },
          hints: ['structure hint'],
        },
        RAW_BY_TOOL[TOOL_NAMES.LOCAL_VIEW_STRUCTURE]
      )
    );

    mockFindFiles.mockResolvedValue(
      attachRawResponseChars(
        {
          files: [{ path: '/workspace/a.ts' }],
          pagination: {
            currentPage: 1,
            totalPages: 1,
            filesPerPage: 20,
            totalFiles: 1,
            hasMore: false,
          },
          hints: ['find hint'],
        },
        RAW_BY_TOOL[TOOL_NAMES.LOCAL_FIND_FILES]
      )
    );

    mockFetchContent.mockResolvedValue(
      attachRawResponseChars(
        {
          path: '/workspace/a.ts',
          content: 'export const a = 1;\n',
          totalLines: 1,
          hints: ['fetch hint'],
        },
        RAW_BY_TOOL[TOOL_NAMES.LOCAL_FETCH_CONTENT]
      )
    );

    mockFindReferences.mockResolvedValue(
      attachRawResponseChars(
        {
          symbolName: 'foo',
          references: [
            {
              uri: '/workspace/a.ts',
              range: {
                start: { line: 0, character: 0 },
                end: { line: 0, character: 3 },
              },
              content: 'foo',
            },
          ],
          pagination: {
            currentPage: 1,
            totalPages: 1,
            referencesPerPage: 20,
            totalReferences: 1,
            hasMore: false,
          },
          hints: ['references hint'],
        },
        RAW_BY_TOOL[TOOL_NAMES.LSP_FIND_REFERENCES]
      )
    );

    mockProcessCallHierarchy.mockResolvedValue(
      attachRawResponseChars(
        {
          symbolName: 'foo',
          calls: [],
          hints: ['call hierarchy hint'],
        },
        RAW_BY_TOOL[TOOL_NAMES.LSP_CALL_HIERARCHY]
      )
    );
  });

  it('records charsSavedByTool for every local + LSP tool when invoked', async () => {
    const mockServer = createMockMcpServer();

    registerLocalRipgrepTool(mockServer.server);
    registerLocalViewStructureTool(mockServer.server);
    registerLocalFindFilesTool(mockServer.server);
    registerLocalFetchContentTool(mockServer.server);
    registerLSPFindReferencesTool(mockServer.server);
    registerLSPCallHierarchyTool(mockServer.server);

    await mockServer.callTool(TOOL_NAMES.LOCAL_RIPGREP, {
      queries: [
        {
          id: 'rg',
          researchGoal: 'exercise localSearchCode stats',
          reasoning: 'prove runtime char savings emission',
          pattern: 'foo',
          path: '/workspace',
        },
      ],
    });

    await mockServer.callTool(TOOL_NAMES.LOCAL_VIEW_STRUCTURE, {
      queries: [
        {
          id: 'view',
          researchGoal: 'exercise localViewStructure stats',
          reasoning: 'prove runtime char savings emission',
          path: '/workspace',
        },
      ],
    });

    await mockServer.callTool(TOOL_NAMES.LOCAL_FIND_FILES, {
      queries: [
        {
          id: 'find',
          researchGoal: 'exercise localFindFiles stats',
          reasoning: 'prove runtime char savings emission',
          path: '/workspace',
        },
      ],
    });

    await mockServer.callTool(TOOL_NAMES.LOCAL_FETCH_CONTENT, {
      queries: [
        {
          id: 'fetch',
          researchGoal: 'exercise localGetFileContent stats',
          reasoning: 'prove runtime char savings emission',
          path: '/workspace/a.ts',
        },
      ],
    });

    await mockServer.callTool(TOOL_NAMES.LSP_FIND_REFERENCES, {
      queries: [
        {
          id: 'refs',
          researchGoal: 'exercise lspFindReferences stats',
          reasoning: 'prove runtime char savings emission',
          uri: '/workspace/a.ts',
          symbolName: 'foo',
          lineHint: 1,
        },
      ],
    });

    await mockServer.callTool(TOOL_NAMES.LSP_CALL_HIERARCHY, {
      queries: [
        {
          id: 'calls',
          researchGoal: 'exercise lspCallHierarchy stats',
          reasoning: 'prove runtime char savings emission',
          uri: '/workspace/a.ts',
          symbolName: 'foo',
          lineHint: 1,
          direction: 'incoming',
        },
      ],
    });

    const statsCalls = vi.mocked(incrementToolCharSavings).mock.calls;
    const recordedToolNames = statsCalls.map(([toolName]) => toolName);

    const expectedToolNames = [
      TOOL_NAMES.LOCAL_RIPGREP,
      TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
      TOOL_NAMES.LOCAL_FIND_FILES,
      TOOL_NAMES.LOCAL_FETCH_CONTENT,
      TOOL_NAMES.LSP_FIND_REFERENCES,
      TOOL_NAMES.LSP_CALL_HIERARCHY,
    ];

    expect(recordedToolNames).toEqual(expectedToolNames);

    for (const toolName of expectedToolNames) {
      const call = statsCalls.find(
        ([recordedName]) => recordedName === toolName
      );
      expect(
        call?.[1],
        `${toolName} should forward upstream raw chars verbatim`
      ).toBe(RAW_BY_TOOL[toolName]);
      expect(
        call?.[2],
        `${toolName} should record positive response chars`
      ).toBeGreaterThan(0);
      expect(
        call?.[2],
        `${toolName} response chars should not equal raw chars (proves both are tracked independently)`
      ).not.toBe(RAW_BY_TOOL[toolName]);
    }
  });
});
