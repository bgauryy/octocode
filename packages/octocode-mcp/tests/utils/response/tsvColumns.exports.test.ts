import { describe, expect, it } from 'vitest';
import { STATIC_TOOL_NAMES } from '../../../src/tools/toolNames.js';
import {
  TOOL_TSV_PROJECTIONS,
  exportToolDataToTsv,
  getTsvProjection,
  githubCloneRepoColumns,
  githubCloneRepoProjection,
  githubCloneRepoToTsv,
  githubFetchContentColumns,
  githubFetchContentProjection,
  githubFetchContentToTsv,
  githubSearchCodeColumns,
  githubSearchCodeProjection,
  githubSearchCodeToTsv,
  githubSearchPullRequestsColumns,
  githubSearchPullRequestsProjection,
  githubSearchPullRequestsToTsv,
  githubSearchRepositoriesColumns,
  githubSearchRepositoriesProjection,
  githubSearchRepositoriesToTsv,
  githubViewRepoStructureColumns,
  githubViewRepoStructureProjection,
  githubViewRepoStructureToTsv,
  localFetchContentColumns,
  localFetchContentProjection,
  localFetchContentToTsv,
  localFindFilesColumns,
  localFindFilesProjection,
  localFindFilesToTsv,
  localSearchCodeColumns,
  localSearchCodeProjection,
  localSearchCodeToTsv,
  localViewStructureColumns,
  localViewStructureProjection,
  localViewStructureToTsv,
  lspCallHierarchyColumns,
  lspCallHierarchyProjection,
  lspCallHierarchyToTsv,
  lspFindReferencesColumns,
  lspFindReferencesProjection,
  lspFindReferencesToTsv,
  lspGotoDefinitionColumns,
  lspGotoDefinitionProjection,
  lspGotoDefinitionToTsv,
  packageSearchColumns,
  packageSearchProjection,
  packageSearchToTsv,
  type TsvProjection,
} from '../../../src/utils/response/tsvColumns.js';

type ExportCase = {
  toolName: string;
  columns: readonly string[];
  projection: TsvProjection;
  toTsv: (data: unknown) => { columns: readonly string[]; rows: string };
  fixture: unknown;
  expectedColumns: readonly string[];
  rowProbe: string;
};

const cases: ExportCase[] = [
  {
    toolName: STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
    columns: githubSearchCodeColumns,
    projection: githubSearchCodeProjection,
    toTsv: githubSearchCodeToTsv,
    fixture: {
      results: [
        {
          id: 'o/r',
          owner: 'o',
          repo: 'r',
          matches: [{ path: 'src/a.ts', value: 'export const a = 1' }],
        },
      ],
    },
    expectedColumns: ['owner', 'repo', 'path', 'value'],
    rowProbe: 'o\tr\tsrc/a.ts\texport const a = 1',
  },
  {
    toolName: STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
    columns: githubFetchContentColumns,
    projection: githubFetchContentProjection,
    toTsv: githubFetchContentToTsv,
    fixture: {
      results: [
        {
          id: 'q1',
          owner: 'o',
          repo: 'r',
          files: [
            {
              path: 'README.md',
              content: 'hello',
              totalLines: 1,
              resolvedBranch: 'main',
            },
          ],
        },
      ],
    },
    // `content` omitted by design — TSV is metadata, file body lives only
    // in JSON `data.results[].files[].content`.
    expectedColumns: ['owner', 'repo', 'path', 'totalLines'],
    rowProbe: 'o\tr\tREADME.md\t1',
  },
  {
    toolName: STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
    columns: githubSearchRepositoriesColumns,
    projection: githubSearchRepositoriesProjection,
    toTsv: githubSearchRepositoriesToTsv,
    fixture: {
      repositories: [
        {
          owner: 'o',
          repo: 'r',
          name: 'r',
          language: 'TypeScript',
          stars: 10,
          pushedAt: '2026',
        },
      ],
    },
    expectedColumns: ['owner', 'repo', 'name', 'language', 'stars', 'pushedAt'],
    rowProbe: 'o\tr\tr',
  },
  {
    toolName: STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    columns: githubSearchPullRequestsColumns,
    projection: githubSearchPullRequestsProjection,
    toTsv: githubSearchPullRequestsToTsv,
    fixture: {
      pull_requests: [
        {
          number: 1,
          state: 'open',
          title: 'Fix',
          author: 'dev',
          additions: 2,
          comments: [{ body: 'ok' }],
        },
      ],
    },
    expectedColumns: ['number', 'state', 'title', 'author', 'comments'],
    rowProbe: '1\topen\t',
  },
  {
    toolName: STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
    columns: githubViewRepoStructureColumns,
    projection: githubViewRepoStructureProjection,
    toTsv: githubViewRepoStructureToTsv,
    fixture: { structure: { '.': { files: ['README.md'], folders: ['src'] } } },
    expectedColumns: ['path', 'type'],
    rowProbe: 'README.md\tfile',
  },
  {
    toolName: STATIC_TOOL_NAMES.GITHUB_CLONE_REPO,
    columns: githubCloneRepoColumns,
    projection: githubCloneRepoProjection,
    toTsv: githubCloneRepoToTsv,
    fixture: { localPath: '/tmp/repo', resolvedBranch: 'main', cached: true },
    expectedColumns: ['localPath', 'resolvedBranch', 'cached'],
    rowProbe: '/tmp/repo\tmain\ttrue',
  },
  {
    toolName: STATIC_TOOL_NAMES.PACKAGE_SEARCH,
    columns: packageSearchColumns,
    projection: packageSearchProjection,
    toTsv: packageSearchToTsv,
    fixture: { packages: [{ name: 'pkg', version: '1.0.0', owner: 'o' }] },
    expectedColumns: ['name', 'version', 'owner', 'repo', 'keywords'],
    rowProbe: 'pkg\t1.0.0',
  },
  {
    toolName: STATIC_TOOL_NAMES.LOCAL_RIPGREP,
    columns: localSearchCodeColumns,
    projection: localSearchCodeProjection,
    toTsv: localSearchCodeToTsv,
    fixture: {
      files: [
        {
          path: 'a.ts',
          matchCount: 1,
          matches: [{ line: 5, column: 0, value: 'export' }],
        },
      ],
    },
    expectedColumns: ['path', 'matchCount', 'line', 'column', 'value'],
    rowProbe: 'a.ts\t1\t5\t0\texport',
  },
  {
    toolName: STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
    columns: localFindFilesColumns,
    projection: localFindFilesProjection,
    toTsv: localFindFilesToTsv,
    fixture: {
      files: [{ path: 'a.ts', type: 'f', size: 1, modified: '2026' }],
    },
    expectedColumns: ['path', 'type', 'size', 'permissions', 'modified'],
    rowProbe: 'a.ts\tf\t1',
  },
  {
    toolName: STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
    columns: localViewStructureColumns,
    projection: localViewStructureProjection,
    toTsv: localViewStructureToTsv,
    fixture: {
      entries: [{ name: 'src', path: '/tmp/src', type: 'd', size: '4K' }],
    },
    // `path` omitted by design — after base-relativization it equals `name`.
    expectedColumns: ['name', 'type', 'size', 'depth'],
    rowProbe: 'src\td\t4K',
  },
  {
    toolName: STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
    columns: localFetchContentColumns,
    projection: localFetchContentProjection,
    toTsv: localFetchContentToTsv,
    fixture: {
      path: 'a.ts',
      content: 'x',
      startLine: 1,
      endLine: 1,
      totalLines: 1,
    },
    expectedColumns: ['path', 'totalLines', 'content'],
    rowProbe: 'a.ts\t1\t\t1\t1\t\tx',
  },
  {
    toolName: STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
    columns: lspGotoDefinitionColumns,
    projection: lspGotoDefinitionProjection,
    toTsv: lspGotoDefinitionToTsv,
    fixture: {
      locations: [
        {
          uri: 'a.ts',
          name: 'foo',
          // range.start.line is 0-based LSP; TSV emits +1 (line=2).
          range: { start: { line: 1, character: 2 } },
        },
      ],
    },
    expectedColumns: ['uri', 'name', 'kind', 'line', 'column'],
    rowProbe: 'a.ts\tfoo\t\t2\t2',
  },
  {
    toolName: STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
    columns: lspFindReferencesColumns,
    projection: lspFindReferencesProjection,
    toTsv: lspFindReferencesToTsv,
    fixture: {
      references: [
        {
          uri: 'b.ts',
          name: 'foo',
          // range.start.line is 0-based LSP; TSV emits +1 (line=4).
          range: { start: { line: 3, character: 4 } },
        },
      ],
    },
    expectedColumns: ['uri', 'name', 'line', 'column', 'isDeclaration'],
    rowProbe: 'b.ts\tfoo\t\t4\t4',
  },
  {
    toolName: STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
    columns: lspCallHierarchyColumns,
    projection: lspCallHierarchyProjection,
    toTsv: lspCallHierarchyToTsv,
    fixture: {
      calls: [
        {
          direction: 'incoming',
          from: {
            name: 'caller',
            uri: 'c.ts',
            // range.start.line is 0-based LSP; TSV emits +1 (line=6).
            range: { start: { line: 5, character: 6 } },
          },
        },
      ],
    },
    expectedColumns: ['direction', 'name', 'kind', 'uri', 'line', 'column'],
    rowProbe: 'incoming\tcaller\t\tc.ts\t6\t6',
  },
];

describe('TSV column exports', () => {
  it.each(cases)(
    '$toolName exports columns, projection, direct TSV helper, and registry entry',
    ({
      toolName,
      columns,
      projection,
      toTsv,
      fixture,
      expectedColumns,
      rowProbe,
    }) => {
      expect(columns.length).toBeGreaterThan(0);
      expect(projection.columns).toBe(columns);
      for (const column of expectedColumns) {
        expect(columns).toContain(column);
      }

      expect(getTsvProjection(toolName)).toBe(projection);
      expect(TOOL_TSV_PROJECTIONS[toolName]).toBe(projection);
      expect(exportToolDataToTsv(toolName, fixture)).toEqual(toTsv(fixture));

      const rendered = toTsv(fixture);
      expect(rendered.columns).toBe(columns);
      expect(rendered.rows.split('\n')[0]).toBe(columns.join('\t'));
      expect(rendered.rows).toContain(rowProbe);
    }
  );

  it('covers every registered tool TSV projection', () => {
    const expected = [
      STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
      STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
      STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
      STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
      STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      STATIC_TOOL_NAMES.GITHUB_CLONE_REPO,
      STATIC_TOOL_NAMES.PACKAGE_SEARCH,
      STATIC_TOOL_NAMES.LOCAL_RIPGREP,
      STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
      STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
      STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
      STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
      STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
      STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
    ];

    expect(Object.keys(TOOL_TSV_PROJECTIONS).sort()).toEqual(expected.sort());
  });
});
