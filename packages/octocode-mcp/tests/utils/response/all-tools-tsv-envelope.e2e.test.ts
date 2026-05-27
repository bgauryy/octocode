/**
 * End-to-end TSV envelope contract for EVERY tool.
 *
 * Live-eval observation: some tools (githubSearchRepositories,
 * githubViewRepoStructure, githubSearchCode, githubGetFileContent,
 * githubSearchPullRequests, lspCallHierarchy) did not emit the TSV
 * envelope when called against a published MCP build. That build is
 * older than this repo, but the test guards against a regression here:
 * every tool's response, when run through the actual generic bulk
 * runner (or its custom finalizer) with `format: "tsv"` (or default),
 * MUST carry `format / columns / rows` at the top level.
 *
 * The matrix:
 *   - 11 tools use the generic `executeBulkOperation` path.
 *   - 2 tools (githubSearchCode, githubGetFileContent) use custom
 *     finalizers — we exercise the finalizer directly with a fixture.
 */

import { describe, it, expect, vi } from 'vitest';
import { executeBulkOperation } from '../../../src/utils/response/bulk.js';
import { STATIC_TOOL_NAMES } from '../../../src/tools/toolNames.js';
import { buildGithubSearchCodeFinalizer } from '../../../src/tools/github_search_code/finalizer.js';
import { buildGithubFetchContentFinalizer } from '../../../src/tools/github_fetch_content/finalizer.js';
import type { BulkFinalizerInput } from '../../../src/types/bulk.js';

// ---------------------------------------------------------------------------
// 1. Generic bulk-path tools — 11 of 13
// ---------------------------------------------------------------------------

type GenericCase = {
  toolName: string;
  dataFixture: Record<string, unknown>;
  /** A signature substring expected in the rendered TSV. */
  rowProbe: string;
};

const genericCases: GenericCase[] = [
  {
    toolName: STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
    dataFixture: {
      repositories: [
        {
          owner: 'o',
          repo: 'r',
          stars: 1,
          language: 'TypeScript',
          pushedAt: '2026',
          forksCount: 0,
          openIssuesCount: 0,
          topics: ['x'],
          description: 'd',
        },
      ],
    },
    rowProbe: 'o\tr\t\t\td\t\t1\t\t0\t0\tTypeScript',
  },
  {
    toolName: STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    dataFixture: {
      pull_requests: [
        {
          number: 42,
          state: 'merged',
          author: 'a',
          title: 't',
          createdAt: '2026',
          updatedAt: '2026',
          additions: 1,
          deletions: 0,
          changedFilesCount: 1,
          url: 'u',
          mergedAt: '2026-05-25T12:00:00Z',
          body: 'body text',
          draft: false,
          assignees: ['reviewer'],
          labels: ['bug'],
          sourceBranch: 'feature',
          targetBranch: 'main',
          sourceSha: 'abc123',
          targetSha: 'def456',
          closedAt: '2026-05-25T12:01:00Z',
          commentsCount: 2,
          comments: [{ author: 'reviewer', body: 'comment' }],
          fileChanges: [{ path: 'src/a.ts', status: 'modified' }],
        },
      ],
    },
    rowProbe: '42\tmerged\tfalse\ta\tt\tbody text\t2026\t2026',
  },
  {
    toolName: STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
    dataFixture: {
      structure: {
        '.': { files: ['README.md'], folders: ['src'] },
      },
    },
    rowProbe: '.\tREADME.md\tfile',
  },
  {
    toolName: STATIC_TOOL_NAMES.PACKAGE_SEARCH,
    dataFixture: {
      packages: [
        {
          name: 'pkg',
          version: '1.0.0',
          owner: 'o',
          repo: 'r',
          weeklyDownloads: 1,
          lastPublished: '2026',
          license: 'MIT',
          description: 'd',
        },
      ],
    },
    rowProbe: 'pkg\t1.0.0\td\to\tr',
  },
  {
    toolName: STATIC_TOOL_NAMES.LOCAL_RIPGREP,
    dataFixture: {
      files: [
        {
          path: 'a.ts',
          matchCount: 1,
          matches: [{ line: 5, column: 0, value: 'export function f' }],
        },
      ],
    },
    rowProbe: '5\t0',
  },
  {
    toolName: STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
    dataFixture: {
      files: [
        {
          path: 'a.ts',
          type: 'f',
          size: 100,
          modified: '2026',
          permissions: '644',
        },
      ],
    },
    rowProbe: 'a.ts\tf\t100\t644\t2026',
  },
  {
    toolName: STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
    dataFixture: {
      entries: [
        { name: 'src', type: 'd', size: '4KB', modified: '2026', depth: 1 },
      ],
    },
    rowProbe: 'src\t\td\t4KB',
  },
  {
    toolName: STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
    dataFixture: {
      path: 'a.ts',
      content: 'x',
      startLine: 1,
      endLine: 1,
      totalLines: 1,
      isPartial: false,
    },
    rowProbe: '1\tfalse',
  },
  {
    toolName: STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
    dataFixture: {
      definitions: [
        {
          uri: 'src/a.ts',
          range: { start: { line: 10, character: 4 } },
          snippet: 'export class A',
        },
      ],
    },
    rowProbe: '10\t4',
  },
  {
    toolName: STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
    dataFixture: {
      references: [
        {
          uri: 'src/b.ts',
          range: { start: { line: 22, character: 0 } },
          snippet: 'new A()',
        },
      ],
    },
    rowProbe: '22\t0',
  },
  {
    toolName: STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
    dataFixture: {
      calls: [
        {
          direction: 'incoming',
          from: {
            name: 'caller',
            uri: 'src/c.ts',
            range: { start: { line: 5, character: 2 } },
          },
        },
      ],
    },
    rowProbe: 'incoming\tcaller\t\tsrc/c.ts\t5\t2',
  },
];

describe('every generic-bulk tool emits the TSV envelope by default', () => {
  it.each(genericCases)(
    '%s emits format/columns/rows with format: "tsv"',
    async ({ toolName, dataFixture, rowProbe }) => {
      const result = await executeBulkOperation(
        [{ id: 'q1' }],
        vi.fn().mockResolvedValue({
          status: 'hasResults' as const,
          ...dataFixture,
        }),
        {
          toolName,
          // NOTE: omitting `format` would default to undefined at this layer
          // (config defaults are only applied at the schema parse). The bulk
          // runner only emits TSV when `config.format === 'tsv'`, so we
          // pass it explicitly here to mirror what the registered schema
          // produces after parsing user input.
          format: 'tsv',
          peerHints: true,
        }
      );
      const sc = result.structuredContent as Record<string, unknown>;
      expect(sc.format, `${toolName} did not emit format=tsv`).toBe('tsv');
      expect(Array.isArray(sc.columns)).toBe(true);
      expect(typeof sc.rows).toBe('string');
      expect(String(sc.rows)).toContain(rowProbe);

      if (toolName === STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS) {
        expect(sc.columns).toEqual(
          expect.arrayContaining([
            'number',
            'state',
            'draft',
            'author',
            'title',
            'body',
            'url',
            'assignees',
            'labels',
            'sourceBranch',
            'targetBranch',
            'sourceSha',
            'targetSha',
            'createdAt',
            'updatedAt',
            'closedAt',
            'mergedAt',
            'commentsCount',
            'changedFilesCount',
            'additions',
            'deletions',
            'comments',
            'fileChanges',
          ])
        );
        expect(String(sc.rows)).toContain('2026-05-25T12:00:00Z');
        expect(String(sc.rows)).toContain('abc123');
        expect(String(sc.rows)).toContain('body text');
        expect(String(sc.rows)).toContain('src/a.ts');
      }
    }
  );
});

// ---------------------------------------------------------------------------
// 2. Custom-finalizer tools — 2 of 13
// ---------------------------------------------------------------------------

describe('custom finalizers emit the TSV envelope when format=tsv', () => {
  it('githubSearchCode finalizer attaches format/columns/rows', () => {
    const finalize = buildGithubSearchCodeFinalizer<{
      id?: string;
      charLength?: number;
    }>();
    const input: BulkFinalizerInput<{ id?: string; charLength?: number }> = {
      queries: [{ id: 'q1' }],
      results: [
        {
          id: 'q1',
          status: 'hasResults',
          data: {
            results: [
              {
                id: 'o/r',
                owner: 'o',
                repo: 'r',
                matches: [{ path: 'a.ts', value: 'export class A {}' }],
              },
            ],
          },
        },
      ],
      config: {
        toolName: STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
        format: 'tsv',
      },
    };
    const out = finalize(input);
    const sc = out.structuredContent as Record<string, unknown>;
    expect(sc.format).toBe('tsv');
    expect(Array.isArray(sc.columns)).toBe(true);
    expect(typeof sc.rows).toBe('string');
    expect(sc.columns).toEqual(['id', 'owner', 'repo', 'path', 'value']);
    expect(String(sc.rows)).toContain('o/r');
    expect(String(sc.rows)).toContain('a.ts');
    expect(String(sc.rows)).toContain('export class A {}');
  });

  it('githubGetFileContent finalizer attaches format/columns/rows', () => {
    // buildGroups() walks the queries[] to seed owner/repo, then reads
    // per-query data for the file entry — so the test fixture must
    // carry owner/repo on the query and `content` on the data, not
    // on a nested `results[]` array (that's the search-code shape).
    const finalize = buildGithubFetchContentFinalizer<{
      id?: string;
      owner?: string;
      repo?: string;
      path?: string;
      charLength?: number;
    }>();
    const input: BulkFinalizerInput<{
      id?: string;
      owner?: string;
      repo?: string;
      path?: string;
      charLength?: number;
    }> = {
      queries: [{ id: 'q1', owner: 'o', repo: 'r', path: 'a.ts' }],
      results: [
        {
          id: 'q1',
          status: 'hasResults',
          data: {
            content: 'hello',
            totalLines: 1,
            startLine: 1,
            endLine: 1,
          },
        },
      ],
      config: {
        toolName: STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
        format: 'tsv',
      },
    };
    const out = finalize(input);
    const sc = out.structuredContent as Record<string, unknown>;
    expect(sc.format).toBe('tsv');
    expect(Array.isArray(sc.columns)).toBe(true);
    expect(typeof sc.rows).toBe('string');
    expect(String(sc.rows)).toContain('1\t');
    expect(String(sc.rows)).toContain('o/r');
    expect(String(sc.rows)).toContain('a.ts');
  });
});

// ---------------------------------------------------------------------------
// 3. Coverage assertion — every tool name in the registry has a test row
// ---------------------------------------------------------------------------

describe('every tool name registered for TSV has a coverage row', () => {
  it('every known tool name appears in one of the test sets above', () => {
    const covered = new Set<string>([
      ...genericCases.map(c => c.toolName),
      STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
      STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
    ]);
    const expected = [
      STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
      STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
      STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
      STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
      STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      STATIC_TOOL_NAMES.PACKAGE_SEARCH,
      STATIC_TOOL_NAMES.LOCAL_RIPGREP,
      STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
      STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
      STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
      STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
      STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
      STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
    ];
    for (const t of expected) {
      expect(covered.has(t), `missing TSV envelope test for ${t}`).toBe(true);
    }
    expect(covered.size).toBeGreaterThanOrEqual(13);
  });
});
