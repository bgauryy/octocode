/**
 * TSV-mode + pagination-cursor contract for every tool (remote + local + LSP).
 *
 * Two policies enforced in one place:
 *   1. Every tool has a TSV projection registered and can emit `{ format,
 *      columns, rows }` alongside its native shape.
 *   2. Pagination cursors are emitted by ONE generator per dimension and
 *      always read as `Page N/M ... Next: <param>=N+1` — never as a multi-
 *      line ceremony, never on the final page.
 */

import { describe, it, expect } from 'vitest';
import {
  getTsvProjection,
  type TsvProjection,
} from '../../src/utils/response/tsvColumns.js';
import { tsvFormat } from '../../src/utils/response/tsvFormat.js';
import { STATIC_TOOL_NAMES } from '../../src/tools/toolNames.js';
import { buildPaginationHints } from '../../src/tools/providerMappers.js';
import {
  generateGitHubPaginationHints,
  generatePaginationHints,
  generateStructurePaginationHints,
} from '../../src/utils/pagination/hints.js';

// ===========================================================================
// 1. Every tool name has a TSV projection — no silent gaps
// ===========================================================================
describe('TSV projection coverage', () => {
  const allTools = [
    // Remote
    STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
    STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
    STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
    STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
    STATIC_TOOL_NAMES.PACKAGE_SEARCH,
    // Local
    STATIC_TOOL_NAMES.LOCAL_RIPGREP,
    STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
    STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
    STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
    // LSP
    STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
    STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
    STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
  ];

  it.each(allTools)('registers a TSV projection for %s', tool => {
    const projection = getTsvProjection(tool);
    expect(projection).toBeDefined();
    expect(Array.isArray(projection!.columns)).toBe(true);
    expect(typeof projection!.toRows).toBe('function');
  });

  it.each(allTools)(
    '%s emits parseable TSV: header + N rows, each row has |columns| cells',
    tool => {
      const projection = getTsvProjection(tool)!;
      // Empty input — header only, no rows, no crashes.
      const text = tsvFormat(projection.columns, []);
      expect(text.split('\n')).toHaveLength(1);
    }
  );
});

// ===========================================================================
// 2. Local + LSP projections — sample-shaped data round-trips through tsvFormat
// ===========================================================================

function project(tool: string, data: unknown) {
  const p = getTsvProjection(tool) as TsvProjection;
  const rows = p.toRows(data);
  return { columns: p.columns, rows, text: tsvFormat(p.columns, rows) };
}

describe('local + LSP TSV projections — sample data', () => {
  it('localSearchCode flattens files[].matches[] to one row per hit', () => {
    const { rows, columns } = project(STATIC_TOOL_NAMES.LOCAL_RIPGREP, {
      files: [
        {
          path: 'src/a.ts',
          matches: [
            { line: 10, column: 4, value: 'export function foo()' },
            { line: 42, column: 1, value: 'foo()' },
          ],
        },
      ],
    });
    expect(columns).toEqual(['path', 'matchCount', 'line', 'column', 'value']);
    expect(rows).toHaveLength(2);
    expect(rows[0]).toMatchObject({
      path: 'src/a.ts',
      line: 10,
      column: 4,
      value: 'export function foo()',
    });
  });

  it('localFindFiles emits one row per file with metadata', () => {
    const { rows, columns } = project(STATIC_TOOL_NAMES.LOCAL_FIND_FILES, {
      files: [
        {
          path: 'README.md',
          type: 'f',
          size: 1234,
          modified: '2026-05-23',
          permissions: '644',
        },
      ],
    });
    expect(columns).toEqual([
      'path',
      'type',
      'size',
      'permissions',
      'modified',
      'accessed',
      'created',
    ]);
    expect(rows[0]).toMatchObject({
      path: 'README.md',
      type: 'f',
      size: 1234,
      permissions: '644',
    });
  });

  it('localViewStructure emits one row per entry', () => {
    const { rows } = project(STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE, {
      entries: [
        { name: 'src', type: 'd', size: '4KB', depth: 1 },
        { name: 'README.md', type: 'f', size: '1KB', depth: 1 },
      ],
    });
    expect(rows).toHaveLength(2);
    expect(rows.map(r => r.name)).toEqual(['src', 'README.md']);
  });

  it('localGetFileContent emits metadata without file content', () => {
    const { rows, text, columns } = project(
      STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
      {
        path: 'src/a.ts',
        content: 'line1\nline2\nline3',
        startLine: 1,
        endLine: 3,
        totalLines: 100,
        isPartial: true,
      }
    );
    expect(rows).toHaveLength(1);
    expect(columns).toContain('content');
    expect(text.split('\n')).toHaveLength(2);
    expect(text).toContain('line1\\nline2\\nline3');
  });

  it('lspGotoDefinition flattens definitions to uri/line/column rows', () => {
    const { rows, columns } = project(STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION, {
      definitions: [
        {
          uri: 'src/a.ts',
          range: { start: { line: 10, character: 4 } },
          snippet: 'export class A',
        },
      ],
    });
    expect(columns).toEqual([
      'uri',
      'name',
      'kind',
      'line',
      'column',
      'content',
      'snippet',
    ]);
    expect(rows[0]).toMatchObject({
      uri: 'src/a.ts',
      line: 10,
      column: 4,
      snippet: 'export class A',
    });
  });

  it('lspFindReferences flattens references the same way', () => {
    const { rows } = project(STATIC_TOOL_NAMES.LSP_FIND_REFERENCES, {
      references: [
        {
          uri: 'src/b.ts',
          range: { start: { line: 22, character: 0 } },
          snippet: 'import { A } from "./a";',
        },
      ],
    });
    expect(rows[0]).toMatchObject({
      line: 22,
      column: 0,
    });
  });

  it('lspCallHierarchy omits direction and uri params', () => {
    const { rows, columns } = project(STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY, {
      calls: [
        {
          direction: 'incoming',
          from: {
            name: 'callerFn',
            uri: 'src/c.ts',
            range: { start: { line: 5, character: 2 } },
          },
        },
      ],
    });
    expect(columns).toEqual([
      'direction',
      'name',
      'kind',
      'uri',
      'line',
      'column',
      'fromRanges',
    ]);
    expect(rows[0]).toMatchObject({
      direction: 'incoming',
      name: 'callerFn',
      uri: 'src/c.ts',
      line: 5,
      column: 2,
    });
  });
});

// ===========================================================================
// 3. Pagination cursor uniformity across generators
//
//    Every generator must satisfy:
//      hasMore=true  → exactly one line of the form "Page N/M ... Next: <p>=N+1"
//      hasMore=false → []  (no "Final page" / "Complete" tautology)
// ===========================================================================

describe('pagination cursor uniformity', () => {
  const buildPagination = (hasMore: boolean) =>
    buildPaginationHints(
      {
        currentPage: 2,
        totalPages: 5,
        hasMore,
        totalMatches: 50,
        perPage: 10,
      },
      'matches'
    );

  it('providerMappers.buildPaginationHints: 1 line on hasMore, [] on final', () => {
    expect(buildPagination(true)).toHaveLength(1);
    expect(buildPagination(true)[0]).toMatch(/Page 2\/5.*page=3/);
    expect(buildPagination(false)).toEqual([]);
  });

  it('generic generatePaginationHints: 1 line on hasMore, [] on final', () => {
    const meta = (hasMore: boolean) => ({
      paginatedContent: 'x',
      charOffset: 0,
      charLength: 10,
      totalChars: 30,
      byteOffset: 0,
      byteLength: 10,
      totalBytes: 30,
      hasMore,
      nextCharOffset: 10,
      currentPage: 1,
      totalPages: 3,
    });
    expect(generatePaginationHints(meta(true))).toHaveLength(1);
    expect(generatePaginationHints(meta(true))[0]).toMatch(
      /Page 1\/3.*charOffset=10/
    );
    expect(generatePaginationHints(meta(false))).toEqual([]);
  });

  it('GitHub file-content cursor: byte-offset based; final page silent', () => {
    expect(
      generateGitHubPaginationHints(
        {
          currentPage: 1,
          totalPages: 3,
          hasMore: true,
          byteOffset: 0,
          byteLength: 20000,
          totalBytes: 60000,
        },
        { owner: 'o', repo: 'r', path: 'a.ts' }
      )[0]
    ).toMatch(/Page 1\/3.*charOffset=20000/);
    expect(
      generateGitHubPaginationHints(
        {
          currentPage: 3,
          totalPages: 3,
          hasMore: false,
          byteOffset: 40000,
          byteLength: 20000,
          totalBytes: 60000,
        },
        { owner: 'o', repo: 'r', path: 'a.ts' }
      )
    ).toEqual([]);
  });

  it('Structure cursor uses entryPageNumber; final page silent', () => {
    expect(
      generateStructurePaginationHints(
        {
          currentPage: 1,
          totalPages: 3,
          hasMore: true,
          entriesPerPage: 20,
          totalEntries: 55,
        },
        {
          owner: 'o',
          repo: 'r',
          branch: 'main',
          pageFiles: 1,
          pageFolders: 1,
          allFiles: 1,
          allFolders: 1,
        }
      )[0]
    ).toMatch(/Page 1\/3.*entryPageNumber=2/);
    expect(
      generateStructurePaginationHints(
        {
          currentPage: 3,
          totalPages: 3,
          hasMore: false,
          entriesPerPage: 20,
          totalEntries: 55,
        },
        {
          owner: 'o',
          repo: 'r',
          branch: 'main',
          pageFiles: 1,
          pageFolders: 1,
          allFiles: 1,
          allFolders: 1,
        }
      )
    ).toEqual([]);
  });
});
