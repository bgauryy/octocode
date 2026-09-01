import { describe, expect, it } from 'vitest';

import {
  buildDirectToolCommandPatterns,
  DirectToolInputError,
  getDirectToolDisplayFields,
  getDirectToolSchemaRelations,
  getDirectToolSchemaVariants,
  getDirectToolVariantDisplayFields,
  prepareDirectToolInput,
} from '../../src/tools/directToolCatalog.meta.js';

describe('prepareDirectToolInput', () => {
  const publicToolNames = [
    'ghSearch',
    'ghSearchHistory',
    'ghGetHistoryItem',
    'ghGetFileContent',
    'ghCloneRepo',
    'ghSearchDiscussions',
    'ghListReleases',
    'localSearch',
    'localAnalyzeGraph',
    'localGetFileContent',
    'lspGetSemantics',
    'npmSearch',
  ];

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
    expect(getDirectToolSchemaRelations('ghGetHistoryItem')).toEqual(
      expect.arrayContaining([
        expect.stringContaining('number'),
        expect.stringContaining('base+head'),
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

    const history = buildDirectToolCommandPatterns('ghSearchHistory');
    expect(history.map(pattern => pattern.query.operation)).toEqual([
      'pullRequests',
      'issues',
      'commits',
    ]);
    const items = buildDirectToolCommandPatterns('ghGetHistoryItem');
    expect(items.map(pattern => pattern.query.operation)).toEqual([
      'pullRequest',
      'issue',
      'commit',
      'compare',
    ]);

    const discussions = buildDirectToolCommandPatterns('ghSearchDiscussions');
    expect(discussions.length).toBeGreaterThan(0);
    expect(discussions[0]?.query).toMatchObject({
      owner: 'vitejs',
      repo: 'vite',
      keywords: ['plugin'],
    });
    expect(
      discussions.every(pattern => pattern.query.after === undefined)
    ).toBe(true);
  });

  it('keeps every published command pattern inside its strict tool schema', () => {
    for (const toolName of publicToolNames) {
      for (const pattern of buildDirectToolCommandPatterns(toolName)) {
        expect(() =>
          prepareDirectToolInput(toolName, pattern.query, {
            rejectUnknownFields: true,
          })
        ).not.toThrow();
      }
    }
  });

  it('keeps every compact variant example constructable and its requirements honest', () => {
    for (const toolName of publicToolNames) {
      for (const variant of getDirectToolSchemaVariants(toolName)) {
        for (const required of variant.requires) {
          expect(
            variant.example,
            `${toolName}.${variant.name}.${required}`
          ).toHaveProperty(required);
        }
        expect(() =>
          prepareDirectToolInput(toolName, variant.example, {
            rejectUnknownFields: true,
          })
        ).not.toThrow();
      }
    }
  });

  it('uses unmistakably absolute placeholders in every local command pattern', () => {
    for (const toolName of [
      'localSearch',
      'localAnalyzeGraph',
      'localGetFileContent',
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

  it('preserves both ghSearch match shapes and uses a neutral operation description', () => {
    const fields = getDirectToolDisplayFields('ghSearch');
    const byName = new Map(fields.map(field => [field.name, field]));

    expect(byName.get('match')?.type).toBe(
      'enum(file, path) | array<enum(name, description, readme)>'
    );
    expect(byName.get('operation')).toMatchObject({
      required: true,
      type: 'enum(code, repositories, tree)',
      description: 'Required operation selector.',
    });
  });

  it('publishes actual ghSearch branch requirements', () => {
    const variants = new Map(
      getDirectToolSchemaVariants('ghSearch').map(variant => [
        variant.name,
        variant,
      ])
    );
    expect(variants.get('code')?.requires).toEqual(['operation']);
    expect(variants.get('code')?.excludes).toEqual(['branch']);
    expect(variants.get('repositories')?.requires).toEqual(['operation']);
    expect(variants.get('tree')?.requires).toEqual([
      'operation',
      'owner',
      'repo',
    ]);
    expect(variants.get('code')?.fields).toEqual([
      'keywords',
      'owner',
      'repo',
      'extension',
      'filename',
      'path',
      'language',
      'match',
      'page',
      'concise',
      'pageSize',
    ]);
    expect(variants.get('tree')?.fields).toEqual([
      'owner',
      'repo',
      'branch',
      'path',
      'maxDepth',
      'page',
      'include',
      'pageSize',
    ]);
  });

  it('keeps workspaceSymbol root optional in compact introspection', () => {
    const workspace = getDirectToolSchemaVariants('lspGetSemantics').find(
      variant => variant.name === 'workspace'
    );

    expect(workspace?.requires).toEqual(['type', 'symbolName']);
  });

  it('derives localSearch operation fields from the executable schema', () => {
    const variants = new Map(
      getDirectToolSchemaVariants('localSearch').map(variant => [
        variant.name,
        variant.fields,
      ])
    );

    expect(variants.get('text')).toContain('searchText');
    expect(variants.get('text')).not.toContain('pattern');
    expect(variants.get('structural')).toContain('pattern');
    expect(variants.get('structural')).not.toContain('searchText');
    expect(variants.get('files')).toContain('pathRegex');
    expect(variants.get('files')).not.toContain('namePattern');
    expect(variants.get('tree')).toContain('namePattern');
    expect(variants.get('tree')).not.toContain('pathRegex');
  });

  it('keeps alternative requirements and branch-specific limits honest', () => {
    const variants = new Map(
      getDirectToolSchemaVariants('localSearch').map(variant => [
        variant.name,
        variant,
      ])
    );
    expect(variants.get('structural')?.requires).toEqual(['operation', 'path']);
    expect(variants.get('structural')?.fields).toEqual(
      expect.arrayContaining(['pattern', 'rule'])
    );

    const fields = getDirectToolVariantDisplayFields('localSearch');
    expect(fields.text?.find(field => field.name === 'pageSize')).toMatchObject(
      {
        constraints: '1-1000',
      }
    );
    expect(
      fields.files?.find(field => field.name === 'pageSize')
    ).toMatchObject({
      constraints: '1-50',
    });
  });

  it('describes PR search as filter-driven and repository-optional', () => {
    const list = getDirectToolSchemaVariants('ghSearchHistory').find(
      variant => variant.name === 'pullRequests'
    );
    expect(list?.requires).toEqual(['operation']);
    expect(getDirectToolSchemaRelations('ghSearchHistory')).toContain(
      'issues and commits require owner+repo; pullRequests may search globally.'
    );
  });

  it('publishes exact ghSearch field scopes', () => {
    expect(getDirectToolSchemaRelations('ghSearch')).toEqual([
      'Use only fields listed for the selected operation.',
      'code and repositories need at least one search term or scope filter.',
      'match: code=file|path; repositories=name|description|readme.',
      "code cannot select branch; it searches GitHub's indexed default branch.",
    ]);
  });

  it('rejects unknown query fields when strict mode is enabled', () => {
    expect(() =>
      prepareDirectToolInput(
        'localSearch',
        { operation: 'text', path: '.', searchText: 'runCLI', typo: true },
        { rejectUnknownFields: true }
      )
    ).toThrow(DirectToolInputError);

    expect(() =>
      prepareDirectToolInput(
        'localSearch',
        { operation: 'text', path: '.', searchText: 'runCLI', typo: true },
        { rejectUnknownFields: true }
      )
    ).toThrow('Unknown field(s): typo');
  });

  it('still suggests the closest field for real typos, but not for short unknowns', () => {
    try {
      prepareDirectToolInput(
        'ghSearch',
        { operation: 'code', keywordz: ['x'], owner: 'o', repo: 'r' },
        { rejectUnknownFields: true }
      );
      expect.unreachable('expected ghSearch to reject unknown fields');
    } catch (error) {
      expect(error).toBeInstanceOf(DirectToolInputError);
      const details = (error as DirectToolInputError & { details?: string[] })
        .details;
      expect(details).toContain("'keywordz' → did you mean 'keywords'?");
    }

    // 2-char unknowns must not get fuzzy false friends ('xq' ≈ 'id' etc.).
    try {
      prepareDirectToolInput(
        'ghSearch',
        { operation: 'code', xq: 1, owner: 'o', repo: 'r' },
        { rejectUnknownFields: true }
      );
      expect.unreachable('expected ghSearch to reject unknown fields');
    } catch (error) {
      const details = (error as DirectToolInputError & { details?: string[] })
        .details;
      expect(details?.some(d => d.includes('did you mean'))).toBe(false);
    }
  });

  it('keeps ghSearch repository keywords canonical', () => {
    const prepared = prepareDirectToolInput(
      'ghSearch',
      {
        operation: 'repositories',
        keywords: ['octocode'],
        concise: true,
        pageSize: 3,
      },
      { rejectUnknownFields: true }
    ) as { queries: Array<Record<string, unknown>> };
    const first = prepared.queries[0]!;
    expect(first.keywords).toEqual(['octocode']);
    expect(first.keywordsToSearch).toBeUndefined();
  });
});
