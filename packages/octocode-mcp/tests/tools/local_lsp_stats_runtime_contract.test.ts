import { beforeEach, describe, expect, it, vi } from 'vitest';
import { incrementToolCharSavings } from '@octocodeai/octocode-tools-core/session';
import { TOOL_NAMES } from '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js';
import { attachRawResponseChars } from '../../../octocode-tools-core/src/utils/response/charSavings.js';
import { createMockMcpServer } from '../fixtures/mcp-fixtures.js';
import { LSP_GET_SEMANTICS_TOOL_NAME } from '../../../octocode-tools-core/src/tools/lsp/shared/semanticTypes.js';

const mockSearchContentRipgrep = vi.hoisted(() => vi.fn());
const mockViewStructure = vi.hoisted(() => vi.fn());
const mockFindFiles = vi.hoisted(() => vi.fn());
const mockFetchContent = vi.hoisted(() => vi.fn());

vi.mock(
  '../../../octocode-tools-core/src/tools/local_ripgrep/searchContentRipgrep.js',
  () => ({
    searchContentRipgrep: (...args: unknown[]) =>
      mockSearchContentRipgrep(...args),
  })
);

vi.mock(
  '../../../octocode-tools-core/src/tools/local_view_structure/local_view_structure.js',
  () => ({
    viewStructure: (...args: unknown[]) => mockViewStructure(...args),
  })
);

vi.mock(
  '../../../octocode-tools-core/src/tools/local_find_files/findFiles.js',
  () => ({
    findFiles: (...args: unknown[]) => mockFindFiles(...args),
  })
);

vi.mock(
  '../../../octocode-tools-core/src/tools/local_fetch_content/fetchContent.js',
  () => ({
    fetchContent: (...args: unknown[]) => mockFetchContent(...args),
  })
);

vi.mock('@octocodeai/octocode-engine/lsp/manager', () => ({
  acquirePooledClient: vi.fn(),
  isLanguageServerAvailable: vi.fn().mockResolvedValue(false),
}));

vi.mock('@octocodeai/octocode-engine/lsp/workspaceRoot', () => ({
  resolveWorkspaceRootForFile: vi.fn().mockResolvedValue(process.cwd()),
}));

import { registerLocalRipgrepTool } from '../../src/tools/local_ripgrep/register.js';
import { registerLocalViewStructureTool } from '../../src/tools/local_view_structure/register.js';
import { registerLocalFindFilesTool } from '../../src/tools/local_find_files/register.js';
import { registerLocalFetchContentTool } from '../../src/tools/local_fetch_content/register.js';
import { registerLspGetSemanticsTool } from '../../src/tools/lsp/semantic_content/register.js';

const RAW_BY_TOOL: Record<string, number> = {
  [TOOL_NAMES.LOCAL_RIPGREP]: 11_111,
  [TOOL_NAMES.LOCAL_VIEW_STRUCTURE]: 22_222,
  [TOOL_NAMES.LOCAL_FIND_FILES]: 33_333,
  [TOOL_NAMES.LOCAL_FETCH_CONTENT]: 44_444,
  [LSP_GET_SEMANTICS_TOOL_NAME]: 55_555,
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
  });

  it('records charsSavedByTool for every local + LSP tool when invoked', async () => {
    const mockServer = createMockMcpServer();

    registerLocalRipgrepTool(mockServer.server);
    registerLocalViewStructureTool(mockServer.server);
    registerLocalFindFilesTool(mockServer.server);
    registerLocalFetchContentTool(mockServer.server);
    registerLspGetSemanticsTool(mockServer.server);

    await mockServer.callTool(TOOL_NAMES.LOCAL_RIPGREP, {
      queries: [
        {
          id: 'rg',
          researchGoal: 'exercise localSearchCode stats',
          reasoning: 'prove runtime char savings emission',
          keywords: 'foo',
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

    await mockServer.callTool(LSP_GET_SEMANTICS_TOOL_NAME, {
      queries: [
        {
          id: 'semantic',
          researchGoal: 'exercise lspGetSemantics stats',
          reasoning: 'prove runtime char savings emission',
          uri: `${process.cwd()}/package.json`,
          type: 'documentSymbols',
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
      LSP_GET_SEMANTICS_TOOL_NAME,
    ];

    expect(recordedToolNames).toEqual(expectedToolNames);

    for (const toolName of expectedToolNames) {
      const call = statsCalls.find(
        ([recordedName]) => recordedName === toolName
      );
      if (toolName.startsWith('lsp')) {
        expect(
          call?.[1],
          `${toolName} should record raw chars`
        ).toBeGreaterThan(0);
      } else {
        expect(
          call?.[1],
          `${toolName} should forward upstream raw chars verbatim`
        ).toBe(RAW_BY_TOOL[toolName]);
      }
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
