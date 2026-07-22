import Ajv2020 from 'ajv/dist/2020.js';
import { describe, expect, it } from 'vitest';
import { z } from 'zod';

import { GitHubCodeSearchOutputLocalSchema } from '../../../octocode-tools-core/src/tools/github_search_code/scheme.js';
import { GitHubFetchContentOutputLocalSchema } from '../../../octocode-tools-core/src/tools/github_fetch_content/scheme.js';
import { GitHubViewRepoStructureOutputLocalSchema } from '../../../octocode-tools-core/src/tools/github_view_repo_structure/scheme.js';
import { GitHubSearchRepositoriesOutputLocalSchema } from '../../../octocode-tools-core/src/tools/github_search_repos/scheme.js';
import { GitHubSearchPullRequestsOutputLocalSchema } from '../../../octocode-tools-core/src/tools/github_search_pull_requests/scheme.js';
import { NpmSearchOutputLocalSchema } from '../../../octocode-tools-core/src/tools/package_search/scheme.js';
import { GitHubCloneRepoOutputLocalSchema } from '../../../octocode-tools-core/src/tools/github_clone_repo/scheme.js';
import { LocalSearchCodeOutputSchema } from '../../../octocode-tools-core/src/tools/local_ripgrep/scheme.js';
import { LocalViewStructureOutputSchema } from '../../../octocode-tools-core/src/tools/local_view_structure/scheme.js';
import { LocalFindFilesOutputSchema } from '../../../octocode-tools-core/src/tools/local_find_files/scheme.js';
import { LocalGetFileContentOutputSchema } from '../../../octocode-tools-core/src/tools/local_fetch_content/scheme.js';
import { LspGetSemanticsOutputSchema } from '../../../octocode-tools-core/src/tools/lsp/semantic_content/scheme.js';

const responsePagination = {
  currentPage: 1,
  totalPages: 2,
  hasMore: true,
  charOffset: 0,
  charLength: 2000,
  totalChars: 3000,
  nextCharOffset: 2000,
};

const itemPagination = {
  currentPage: 1,
  totalPages: 2,
  hasMore: true,
  nextPage: 2,
  filesPerPage: 20,
  entriesPerPage: 20,
  matchesPerPage: 20,
  totalFiles: 25,
  totalEntries: 25,
  totalMatches: 25,
};

const continuation = {
  tool: 'localGetFileContent',
  query: { path: '/tmp/example.ts' },
  why: 'continue',
  confidence: 'exact',
};

const runtimeStructuredContentByTool = [
  {
    name: 'ghSearchCode',
    schema: GitHubCodeSearchOutputLocalSchema,
    value: {
      responsePagination,
      results: [
        {
          id: 'q1',
          data: {
            files: ['microsoft/vscode:README.md'],
            pagination: {
              currentPage: 1,
              totalPages: 1,
              hasMore: false,
              totalMatches: 1,
              uniqueFileCount: 1,
            },
          },
        },
      ],
      next: { getLines: continuation },
    },
  },
  {
    name: 'ghGetFileContent',
    schema: GitHubFetchContentOutputLocalSchema,
    value: {
      responsePagination,
      results: [
        {
          id: 'microsoft/vscode',
          owner: 'microsoft',
          repo: 'vscode',
          files: [
            {
              path: 'README.md',
              content: '# Visual Studio Code',
              contentView: 'none',
              totalLines: 3,
              sourceChars: 20,
              sourceBytes: 20,
              resolvedBranch: 'main',
              modified: '2026-01-01T00:00:00.000Z',
              startLine: 1,
              endLine: 3,
              pagination: {
                currentPage: 1,
                totalPages: 1,
                hasMore: false,
                charOffset: 0,
                charLength: 20,
                totalChars: 20,
              },
              next: { cloneForSemantics: { ...continuation, tool: 'ghCloneRepo' } },
            },
          ],
        },
      ],
    },
  },
  {
    name: 'ghViewRepoStructure',
    schema: GitHubViewRepoStructureOutputLocalSchema,
    value: {
      responsePagination,
      results: [
        {
          id: 'q1',
          data: {
            structure: ['README.md'],
            pagination: itemPagination,
          },
        },
      ],
    },
  },
  {
    name: 'ghSearchRepos',
    schema: GitHubSearchRepositoriesOutputLocalSchema,
    value: {
      responsePagination,
      results: [
        {
          id: 'q1',
          data: {
            repositories: ['microsoft/vscode'],
            pagination: {
              currentPage: 1,
              totalPages: 1,
              hasMore: false,
              totalMatches: 1,
              totalMatchesKind: 'exact',
              totalMatchesCapped: false,
            },
          },
        },
      ],
    },
  },
  {
    name: 'ghHistoryResearch',
    schema: GitHubSearchPullRequestsOutputLocalSchema,
    value: {
      responsePagination,
      results: [
        {
          id: 'q1',
          data: {
            pull_requests: ['#1 Example'],
            next: { readPr: continuation },
          },
        },
      ],
    },
  },
  {
    name: 'npmSearch',
    schema: NpmSearchOutputLocalSchema,
    value: {
      responsePagination,
      results: [
        {
          id: 'q1',
          data: {
            packages: [{ name: 'typescript', version: '5.0.0' }],
            repositories: {
              typescript: {
                repository: 'https://github.com/microsoft/TypeScript',
                owner: 'microsoft',
                repo: 'TypeScript',
                next: { searchCode: continuation },
              },
            },
            pagination: {
              currentPage: 1,
              totalPages: 1,
              hasMore: false,
              totalItems: 1,
            },
          },
        },
      ],
    },
  },
  {
    name: 'ghCloneRepo',
    schema: GitHubCloneRepoOutputLocalSchema,
    value: {
      responsePagination,
      results: [
        {
          id: 'q1',
          data: {
            localPath: '/tmp/octocode-cache/vscode',
            cached: true,
          },
        },
      ],
    },
  },
  {
    name: 'localSearchCode',
    schema: LocalSearchCodeOutputSchema,
    value: {
      responsePagination,
      results: [
        {
          id: 'q1',
          data: {
            files: [
              {
                path: 'benchmark/_engine.mjs',
                matches: [{ line: 1, column: 1, value: 'engine' }],
                pagination: itemPagination,
              },
            ],
            searchEngine: 'rg',
            stats: { totalOccurrences: 1, matchedLines: 1 },
            pagination: itemPagination,
          },
        },
      ],
    },
  },
  {
    name: 'localViewStructure',
    schema: LocalViewStructureOutputSchema,
    value: {
      responsePagination,
      results: [
        {
          id: 'q1',
          data: {
            path: 'octocode-benchmark',
            files: ['package.json'],
            folders: ['benchmark'],
            pagination: itemPagination,
          },
        },
      ],
    },
  },
  {
    name: 'localFindFiles',
    schema: LocalFindFilesOutputSchema,
    value: {
      responsePagination,
      results: [
        {
          id: 'q1',
          data: {
            files: ['package.json'],
            pagination: itemPagination,
          },
        },
      ],
    },
  },
  {
    name: 'localGetFileContent',
    schema: LocalGetFileContentOutputSchema,
    value: {
      responsePagination,
      results: [
        {
          id: 'q1',
          data: {
            path: 'package.json',
            content: '{}',
            pagination: responsePagination,
          },
        },
      ],
    },
  },
  {
    name: 'lspGetSemantics',
    schema: LspGetSemanticsOutputSchema,
    value: {
      responsePagination,
      results: [
        {
          id: 'q1',
          data: {
            type: 'documentSymbols',
            uri: '/tmp/example.mjs',
            payload: { kind: 'documentSymbols', symbols: [], totalSymbols: 0 },
            pagination: itemPagination,
            lsp: { serverAvailable: true, provider: 'documentSymbolProvider' },
          },
        },
      ],
    },
  },
] as const;

function validateWithJsonSchema(schema: z.ZodType, value: unknown): string[] {
  const jsonSchema = z.toJSONSchema(schema, { io: 'output' });
  const ajv = new Ajv2020({ allErrors: true, strict: false });
  const validate = ajv.compile(jsonSchema);
  if (validate(value)) return [];
  return (validate.errors ?? []).map(error => `${error.instancePath} ${error.message}`);
}

describe('MCP output schemas match runtime structuredContent envelopes', () => {
  it.each(runtimeStructuredContentByTool)(
    '$name accepts the shape emitted through structuredContent',
    ({ schema, value }) => {
      expect(validateWithJsonSchema(schema, value)).toEqual([]);
    }
  );
});
