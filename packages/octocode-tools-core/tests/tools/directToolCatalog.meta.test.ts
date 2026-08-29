import { describe, expect, it } from 'vitest';

import {
  buildDirectToolCommandPatterns,
  DirectToolInputError,
  getDirectToolDisplayFields,
  getDirectToolSchemaRelations,
  prepareDirectToolInput,
} from '../../src/tools/directToolCatalog.meta.js';

describe('prepareDirectToolInput', () => {
  it('publishes conditional field relations that flattened schemas cannot express', () => {
    expect(getDirectToolSchemaRelations('localAnalyzeGraph')).toEqual(
      expect.arrayContaining([
        expect.stringContaining('dependencies | dependents'),
        expect.stringContaining('path -> requires file + target'),
      ])
    );
    expect(getDirectToolSchemaRelations('lspGetSemantics')).toEqual(
      expect.arrayContaining([
        expect.stringContaining('workspaceSymbol'),
        expect.stringContaining('definition | references'),
      ])
    );
    expect(getDirectToolSchemaRelations('ghSearchIssues')).toEqual(
      expect.arrayContaining([
        expect.stringContaining('issueNumber'),
        expect.stringContaining('list mode'),
      ])
    );
  });

  it('provides valid hand-authored patterns for every graph operation and split mode', () => {
    const graph = buildDirectToolCommandPatterns('localAnalyzeGraph');
    expect(graph.map(pattern => pattern.query.operation)).toEqual([
      'deadCode',
      'cycles',
      'dependencies',
      'dependents',
      'path',
      'reachability',
    ]);
    expect(graph.every(pattern => pattern.query.path === '/ABS/repo')).toBe(
      true
    );

    const issues = buildDirectToolCommandPatterns('ghSearchIssues');
    expect(issues).toHaveLength(2);
    expect(issues[0]?.query.issueNumber).toBeUndefined();
    expect(issues[1]?.query.issueNumber).toBeTypeOf('number');

    const discussions = buildDirectToolCommandPatterns('ghSearchDiscussions');
    expect(discussions.length).toBeGreaterThan(0);
    expect(discussions[0]?.query).toMatchObject({
      owner: 'vitejs',
      repo: 'vite',
      keywordsToSearch: ['plugin'],
    });
    expect(
      discussions.every(pattern => pattern.query.after === undefined)
    ).toBe(true);
  });

  it('keeps every published command pattern inside its strict tool schema', () => {
    const toolNames = [
      'ghSearchCode',
      'ghSearchRepos',
      'ghSearchPullRequests',
      'ghSearchIssues',
      'ghSearchCommits',
      'ghGetFileContent',
      'ghViewRepoStructure',
      'ghCloneRepo',
      'ghSearchDiscussions',
      'ghListReleases',
      'localSearchCode',
      'localFindFiles',
      'localAnalyzeGraph',
      'localGetFileContent',
      'localViewStructure',
      'lspGetSemantics',
      'npmSearch',
    ];
    for (const toolName of toolNames) {
      for (const pattern of buildDirectToolCommandPatterns(toolName)) {
        expect(() =>
          prepareDirectToolInput(toolName, pattern.query, {
            rejectUnknownFields: true,
          })
        ).not.toThrow();
      }
    }
  });

  it('uses unmistakably absolute placeholders in every local command pattern', () => {
    for (const toolName of [
      'localSearchCode',
      'localFindFiles',
      'localAnalyzeGraph',
      'localGetFileContent',
      'localViewStructure',
      'lspGetSemantics',
    ]) {
      for (const pattern of buildDirectToolCommandPatterns(toolName)) {
        expect(pattern.query.path ?? pattern.query.uri).toMatch(/^\/ABS\//);
      }
    }
  });

  it('introspects discriminated graph operations without flattening required fields', () => {
    const fields = getDirectToolDisplayFields('localAnalyzeGraph');
    const byName = new Map(fields.map(field => [field.name, field]));
    expect(byName.get('operation')).toMatchObject({
      required: true,
      type: 'enum(deadCode, cycles, dependencies, dependents, path, reachability)',
    });
    expect(byName.get('path')?.required).toBe(true);
    expect(byName.get('file')?.required).toBe(false);
    expect(byName.get('target')?.required).toBe(false);
  });

  it('rejects unknown query fields when strict mode is enabled', () => {
    expect(() =>
      prepareDirectToolInput(
        'localSearchCode',
        { path: '.', keywords: 'runCLI', typo: true },
        { rejectUnknownFields: true }
      )
    ).toThrow(DirectToolInputError);

    expect(() =>
      prepareDirectToolInput(
        'localSearchCode',
        { path: '.', keywords: 'runCLI', typo: true },
        { rejectUnknownFields: true }
      )
    ).toThrow('Unknown field(s): typo');
  });

  it('folds well-known cross-tool field renames to the canonical field (no error)', () => {
    const cases: Array<{
      tool: string;
      query: Record<string, unknown>;
      canonical: string;
      alias: string;
    }> = [
      {
        tool: 'ghSearchCode',
        query: { keywordsToSearch: ['x'], owner: 'o', repo: 'r' },
        alias: 'keywordsToSearch',
        canonical: 'keywords',
      },
      {
        tool: 'ghSearchRepos',
        query: { keywordsToSearch: ['octocode'], concise: true, limit: 3 },
        alias: 'keywordsToSearch',
        canonical: 'keywords',
      },
      {
        tool: 'ghViewRepoStructure',
        query: { owner: 'o', repo: 'r', path: '', depth: 1 },
        alias: 'depth',
        canonical: 'maxDepth',
      },
      {
        tool: 'npmSearch',
        query: { name: 'zod' },
        alias: 'name',
        canonical: 'packageName',
      },
      {
        tool: 'lspGetSemantics',
        query: { path: '/tmp/x.ts', line: 12, op: 'references' },
        alias: 'line',
        canonical: 'lineHint',
      },
    ];
    for (const { tool, query, alias, canonical } of cases) {
      const prepared = prepareDirectToolInput(tool, query, {
        rejectUnknownFields: true,
      }) as { queries: Array<Record<string, unknown>> };
      const first = prepared.queries[0]!;
      expect(first[canonical], `${tool}.${canonical}`).toEqual(
        query[alias as keyof typeof query]
      );
      expect(first[alias], `${tool}.${alias} removed`).toBeUndefined();
    }
  });

  it('still suggests the closest field for real typos, but not for short unknowns', () => {
    try {
      prepareDirectToolInput(
        'ghSearchCode',
        { keywordz: ['x'], owner: 'o', repo: 'r' },
        { rejectUnknownFields: true }
      );
      expect.unreachable('expected ghSearchCode to reject unknown fields');
    } catch (error) {
      expect(error).toBeInstanceOf(DirectToolInputError);
      const details = (error as DirectToolInputError & { details?: string[] })
        .details;
      expect(details).toContain("'keywordz' → did you mean 'keywords'?");
    }

    // 2-char unknowns must not get fuzzy false friends ('xq' ≈ 'id' etc.).
    try {
      prepareDirectToolInput(
        'ghSearchCode',
        { xq: 1, owner: 'o', repo: 'r' },
        { rejectUnknownFields: true }
      );
      expect.unreachable('expected ghSearchCode to reject unknown fields');
    } catch (error) {
      const details = (error as DirectToolInputError & { details?: string[] })
        .details;
      expect(details?.some(d => d.includes('did you mean'))).toBe(false);
    }
  });

  it('keeps ghSearchRepos canonical keywords (does not rewrite to keywordsToSearch)', () => {
    const prepared = prepareDirectToolInput(
      'ghSearchRepos',
      { keywords: ['octocode'], concise: true, limit: 3 },
      { rejectUnknownFields: true }
    ) as { queries: Array<Record<string, unknown>> };
    const first = prepared.queries[0]!;
    expect(first.keywords).toEqual(['octocode']);
    expect(first.keywordsToSearch).toBeUndefined();
  });
});
