/**
 * Branch coverage for fallback chains in `tsvColumns.ts`.
 *
 * Each projection uses `??` chains so missing/partial upstream shapes
 * still produce parseable TSV. These tests exercise those branches
 * directly so coverage reflects what the registry actually has to
 * tolerate at runtime (LSP servers vary, GitHub omits optional fields,
 * etc).
 */

import { describe, it, expect } from 'vitest';
import { getTsvProjection } from '../../../src/utils/response/tsvColumns.js';
import { STATIC_TOOL_NAMES } from '../../../src/tools/toolNames.js';

function project(tool: string, data: unknown) {
  const p = getTsvProjection(tool)!;
  return p.toRows(data);
}

describe('tsvColumns fallback chains', () => {
  it('searchCode emits one TSV row per match, defaulting missing cells', () => {
    const rows = project(STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE, {
      results: [{ matches: [{}, { path: 'x.ts' }] }],
    });
    expect(rows).toEqual([
      { id: '', owner: '', repo: '', path: '', value: '' },
      { id: '', owner: '', repo: '', path: 'x.ts', value: '' },
    ]);
  });

  it('fetchContent tolerates missing per-field metadata', () => {
    const rows = project(STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT, {
      results: [{ owner: 'o', repo: 'r', files: [{}] }],
    });
    expect(rows[0]).toMatchObject({
      totalLines: '',
      lastModifiedBy: '',
    });
  });

  it('searchRepositories tolerates non-array topics', () => {
    const rows = project(STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES, {
      repositories: [
        { owner: 'o', repo: 'r' /* topics intentionally absent */ },
      ],
    });
    expect(rows[0]).toMatchObject({ repo: 'r', topics: '' });
  });

  it('searchPullRequests tolerates partial PR rows', () => {
    const rows = project(STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, {
      pull_requests: [{ number: 1 }],
    });
    expect(rows[0]).toMatchObject({
      number: 1,
      additions: '',
    });
  });

  it('viewRepoStructure tolerates dirs without files/folders arrays', () => {
    const rows = project(STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE, {
      structure: { 'empty/dir': {} },
    });
    // No entries: zero rows.
    expect(rows).toHaveLength(0);
  });

  it('packageSearch tolerates partial package rows', () => {
    const rows = project(STATIC_TOOL_NAMES.PACKAGE_SEARCH, {
      packages: [{ name: 'x' }],
    });
    expect(rows[0]).toMatchObject({
      name: 'x',
      version: '',
      owner: '',
      weeklyDownloads: '',
    });
  });

  it('localSearchCode tolerates files without matches by emitting file rows', () => {
    const rows = project(STATIC_TOOL_NAMES.LOCAL_RIPGREP, {
      files: [{ path: 'a.ts' }, { path: 'b.ts', matches: [] }],
    });
    expect(rows).toEqual([
      { path: 'a.ts', matchCount: '', line: '', column: '', value: '' },
      { path: 'b.ts', matchCount: '', line: '', column: '', value: '' },
    ]);
  });

  it('localFindFiles tolerates files without size / modified / perms', () => {
    const rows = project(STATIC_TOOL_NAMES.LOCAL_FIND_FILES, {
      files: [{ path: 'a.ts' }],
    });
    expect(rows[0]).toEqual({
      path: 'a.ts',
      type: '',
      size: '',
      permissions: '',
      modified: '',
      accessed: '',
      created: '',
    });
  });

  it('localViewStructure tolerates entries without modified / size', () => {
    const rows = project(STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE, {
      entries: [{ name: 'x', type: 'f' }],
    });
    expect(rows[0]).toEqual({
      name: 'x',
      path: '',
      type: 'f',
      size: '',
      modified: '',
      depth: '',
    });
  });

  it('localGetFileContent emits zero rows when neither content nor totalLines present', () => {
    const rows = project(STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT, { path: 'x' });
    expect(rows).toHaveLength(0);
  });

  it('localGetFileContent emits a row when totalLines is present (even without content)', () => {
    const rows = project(STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT, {
      path: 'x',
      totalLines: 10,
    });
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ totalLines: 10 });
  });

  it('lspGotoDefinition falls back through locations when definitions absent', () => {
    const rows = project(STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION, {
      locations: [
        { path: 'src/a.ts', line: 10, column: 4, context: 'class A' },
      ],
    });
    expect(rows[0]).toMatchObject({
      line: 10,
      column: 4,
    });
  });

  it('lspFindReferences falls back through `locations` and `column` field', () => {
    const rows = project(STATIC_TOOL_NAMES.LSP_FIND_REFERENCES, {
      locations: [{ uri: 'b.ts', line: 5, column: 2, snippet: 'x' }],
    });
    expect(rows[0]).toMatchObject({
      line: 5,
      column: 2,
    });
  });

  it('lspGotoDefinition tolerates ranges without start.character (uses start.column)', () => {
    const rows = project(STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION, {
      definitions: [
        { uri: 'a.ts', range: { start: { line: 1, column: 7 } }, snippet: 's' },
      ],
    });
    expect(rows[0]).toMatchObject({ line: 1, column: 7 });
  });

  it('lspCallHierarchy falls back through `to` when `from` absent', () => {
    const rows = project(STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY, {
      calls: [
        {
          direction: 'outgoing',
          to: {
            name: 'callee',
            uri: 'src/h.ts',
            range: { start: { line: 12, character: 4 } },
          },
        },
      ],
    });
    expect(rows[0]).toMatchObject({
      name: 'callee',
      line: 12,
      column: 4,
    });
  });

  it('lspCallHierarchy tolerates missing node entirely', () => {
    const rows = project(STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY, {
      calls: [{ direction: 'incoming' }],
    });
    expect(rows[0]).toEqual({
      direction: 'incoming',
      name: '',
      kind: '',
      uri: '',
      line: '',
      column: '',
      fromRanges: '',
    });
  });

  it('does not expose non-output-only tool input parameter names as TSV columns', () => {
    const forbiddenByTool: Record<string, string[]> = {
      [STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE]: [
        'filename',
        'extension',
        'match',
        'page',
        'limit',
      ],
      [STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT]: [
        'branch',
        'matchString',
      ],
      [STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES]: [
        'created',
        'updated',
        'sort',
      ],
      [STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS]: [
        'assignee',
        'base',
        'head',
        'label',
        'sort',
        'order',
      ],
      [STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE]: [
        'branch',
      ],
      [STATIC_TOOL_NAMES.LOCAL_RIPGREP]: ['pattern', 'sort'],
      [STATIC_TOOL_NAMES.LOCAL_FIND_FILES]: [
        'name',
        'sortBy',
      ],
      [STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE]: [
        'pattern',
        'extension',
      ],
      [STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT]: [
        'matchString',
      ],
      [STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION]: [
        'symbolName',
        'lineHint',
        'orderHint',
      ],
      [STATIC_TOOL_NAMES.LSP_FIND_REFERENCES]: [
        'symbolName',
        'lineHint',
        'orderHint',
      ],
      [STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY]: [
        'symbolName',
        'lineHint',
        'orderHint',
      ],
    };

    for (const [tool, forbidden] of Object.entries(forbiddenByTool)) {
      const columns = getTsvProjection(tool)?.columns ?? [];
      expect(columns.filter(column => forbidden.includes(column))).toEqual([]);
    }
  });
});
