import { describe, expect, it } from 'vitest';

import { buildDirectToolCommandPatterns } from '@octocodeai/octocode-core/schema';
import { DirectToolInputError } from '@octocodeai/octocode-core/schema';
import {
  getDirectToolDisplayFields,
  getDirectToolVariantDisplayFields,
} from '@octocodeai/octocode-core/schema';
import {
  getDirectToolSchemaRelations,
  getDirectToolSchemaVariants,
} from '@octocodeai/octocode-core/schema';
import { prepareDirectToolInput } from '../helpers/prepareDirectToolInput.js';
import { getToolSchemaRelations } from '@octocodeai/octocode-core/schema';

describe('prepareDirectToolInput', () => {
  it('publishes topology analysis restrictions consistently with execution', () => {
    const relations = getDirectToolSchemaRelations('astSearch');
    expect(relations).toEqual(getToolSchemaRelations('astSearch'));
    expect(relations.join(' ')).toContain(
      'entrypoints and includeTests are valid only for reachability and deadCode'
    );
    for (const analysis of ['reachability', 'deadCode', 'cycles']) {
      const prepare = () =>
        prepareDirectToolInput(
          'astSearch',
          {
            operation: 'topology',
            analysis,
            path: '/ABS/repo',
            includeTests: false,
          },
          { rejectUnknownFields: true }
        );
      if (analysis === 'cycles') expect(prepare).toThrow(DirectToolInputError);
      else expect(prepare).not.toThrow();
    }
  });
  const publicToolNames = [
    'ghSearch',
    'ghSearchHistory',
    'ghGetHistoryItem',
    'ghGetFileContent',
    'ghCloneRepo',
    'localSearch',
    'astSearch',
    'localFetch',
    'lspSearch',
    'artifactSearch',
  ];

  it.each([
    [
      'artifactSearch',
      {},
      'Set exactly one non-empty packageName or keywords.',
    ],
    [
      'lspSearch',
      {},
      'workspaceSymbol needs symbolName and either uri or workspaceRoot. documentSymbols/diagnostic need uri. definition | references | hover | callers | callees | callHierarchy | implementation | typeDefinition | supertypes | subtypes -> requires uri and exactly one anchor: position or symbolName+lineHint.',
    ],
  ])(
    'reports the public relation for invalid %s union input',
    (toolName, query, expectedDetail) => {
      try {
        prepareDirectToolInput(toolName, query, {
          rejectUnknownFields: true,
        });
        expect.unreachable('expected invalid input');
      } catch (error) {
        expect(error).toBeInstanceOf(DirectToolInputError);
        expect((error as DirectToolInputError).details).toContain(
          expectedDetail
        );
      }
    }
  );

  it.each([
    [
      'ghSearch',
      {
        operation: 'code',
        owner: 'o',
        repo: 'r',
        keywords: ['x'],
        topicsToSearch: ['mcp'],
      },
      'topics',
    ],
    ['astSearch', { operation: 'cycles', path: '/repo', maxDepth: 3 }, 'depth'],
  ])(
    'does not suggest %s fields that are invalid for the active variant',
    (tool, query, invalidSuggestion) => {
      try {
        prepareDirectToolInput(tool, query, { rejectUnknownFields: true });
        expect.unreachable(`expected ${tool} to reject an unknown field`);
      } catch (error) {
        expect(error).toBeInstanceOf(DirectToolInputError);
        expect((error as DirectToolInputError).details.join(' ')).not.toContain(
          `did you mean '${invalidSuggestion}'`
        );
      }
    }
  );

  it('retains an alias suggestion when it is valid for the active LSP variant', () => {
    try {
      prepareDirectToolInput(
        'lspSearch',
        {
          type: 'references',
          uri: '/repo/file.ts',
          symbolName: 'Thing',
          lineHint: 1,
          path: '/repo/other.ts',
        },
        { rejectUnknownFields: true }
      );
      expect.unreachable('expected lspSearch to reject path');
    } catch (error) {
      expect(error).toBeInstanceOf(DirectToolInputError);
      expect((error as DirectToolInputError).details).toContain(
        "'path' → did you mean 'uri'?"
      );
    }
  });

  it.each([
    ['ghGetFileContent', 'queries.0.owner'],
    ['localFetch', 'queries.0.path'],
  ])('keeps the query index in flattened %s union errors', (toolName, path) => {
    try {
      prepareDirectToolInput(toolName, {}, { rejectUnknownFields: true });
      expect.unreachable('expected invalid input');
    } catch (error) {
      expect(error).toBeInstanceOf(DirectToolInputError);
      expect((error as DirectToolInputError).details).toEqual(
        expect.arrayContaining([expect.stringContaining(`${path}:`)])
      );
    }
  });

  it('publishes conditional field relations that flattened schemas cannot express', () => {
    expect(getDirectToolSchemaRelations('astSearch')).toEqual(
      expect.arrayContaining([
        expect.stringContaining('topology uses analysis'),
      ])
    );
    expect(getDirectToolSchemaRelations('lspSearch')).toEqual(
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
    const graph = buildDirectToolCommandPatterns('astSearch');
    expect(
      graph
        .filter(pattern => pattern.query.operation === 'topology')
        .map(pattern => pattern.query.analysis)
    ).toEqual([
      'deadCode',
      'cycles',
      'dependencies',
      'dependents',
      'path',
      'reachability',
    ]);
    expect(
      graph
        .filter(pattern => pattern.query.operation === 'topology')
        .every(pattern => pattern.query.path === '/ABS/repo')
    ).toBe(true);

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
      'astSearch',
      'localFetch',
      'lspSearch',
    ]) {
      for (const pattern of buildDirectToolCommandPatterns(toolName)) {
        expect(pattern.query.path ?? pattern.query.uri).toMatch(/^\/ABS\//);
      }
    }
  });

  it('introspects discriminated graph operations without flattening required fields', () => {
    const fields = getDirectToolDisplayFields('astSearch');
    const byName = new Map(fields.map(field => [field.name, field]));
    expect(byName.get('operation')).toMatchObject({
      required: true,
      type: 'enum(match, files, tree, symbols, topology)',
    });
    // path is optional: omitting it is valid when file/target is absolute
    // (the root is inferred by walking up to the nearest package.json).
    expect(byName.get('path')?.required).toBe(false);
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
    expect(variants.get('code')?.requires).toEqual(['operation', 'reasoning']);
    expect(variants.get('code')?.excludes).toEqual(['branch']);
    expect(variants.get('repositories')?.requires).toEqual([
      'operation',
      'reasoning',
    ]);
    expect(variants.get('tree')?.requires).toEqual([
      'operation',
      'owner',
      'repo',
      'reasoning',
    ]);
    expect(variants.get('code')?.fields).toEqual([
      'reasoning',
      'debug',
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
      'reasoning',
      'debug',
      'owner',
      'repo',
      'branch',
      'path',
      'maxDepth',
      'page',
      'metadataPage',
      'include',
      'materialize',
      'materializeOffset',
      'pageSize',
    ]);
  });

  it('publishes both workspaceSymbol root choices in compact introspection', () => {
    const variants = getDirectToolSchemaVariants('lspSearch');
    const workspaceUri = variants.find(
      variant => variant.name === 'workspace:uri'
    );
    const workspaceRoot = variants.find(
      variant => variant.name === 'workspace:root'
    );

    expect(workspaceUri?.requires).toEqual([
      'operation',
      'symbolName',
      'uri',
      'reasoning',
    ]);
    expect(workspaceRoot?.requires).toEqual([
      'operation',
      'symbolName',
      'workspaceRoot',
      'reasoning',
    ]);
  });

  it('derives astSearch operation fields from the executable schema', () => {
    const variants = new Map(
      getDirectToolSchemaVariants('astSearch').map(variant => [
        variant.name,
        variant.fields,
      ])
    );

    expect(variants.get('match')).toContain('pattern');
    expect(variants.get('match')).toContain('rule');
    expect(variants.get('files')).toContain('pathRegex');
    expect(variants.get('files')).not.toContain('namePattern');
    expect(variants.get('tree:filesystem')).toContain('namePattern');
    expect(variants.get('tree:filesystem')).not.toContain('pathRegex');
    expect(variants.get('tree:syntax')).toContain('nodeLimit');
    expect(variants.get('tree:syntax')).not.toContain('namePattern');
    expect(variants.get('tree:syntax')).not.toContain('maxDepth');
  });

  it('keeps alternative requirements and branch-specific limits honest', () => {
    const variants = new Map(
      getDirectToolSchemaVariants('astSearch').map(variant => [
        variant.name,
        variant,
      ])
    );
    expect(variants.get('match')?.requires).toEqual([
      'operation',
      'path',
      'reasoning',
    ]);
    expect(variants.get('match')?.fields).toEqual(
      expect.arrayContaining(['pattern', 'rule'])
    );

    const fields = getDirectToolVariantDisplayFields('astSearch');
    expect(
      fields.match?.find(field => field.name === 'pageSize')
    ).toMatchObject({
      constraints: '1-1000',
    });
    expect(
      fields.files?.find(field => field.name === 'pageSize')
    ).toMatchObject({
      constraints: '1-100',
    });
  });

  it('describes PR search as filter-driven and repository-optional', () => {
    const list = getDirectToolSchemaVariants('ghSearchHistory').find(
      variant => variant.name === 'pullRequests'
    );
    expect(list?.requires).toEqual(['operation', 'reasoning']);
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
        { path: '.', searchText: 'runCLI', typo: true },
        { rejectUnknownFields: true }
      )
    ).toThrow(DirectToolInputError);

    expect(() =>
      prepareDirectToolInput(
        'localSearch',
        { path: '.', searchText: 'runCLI', typo: true },
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

  it.each([
    ['lexical', { path: '.', searchText: 'needle', maxResults: 5 }, 'maxFiles'],
  ])(
    'suggests a field valid for the active localSearch %s variant',
    (_operation, query, expectedField) => {
      try {
        prepareDirectToolInput('localSearch', query, {
          rejectUnknownFields: true,
        });
        expect.unreachable('expected localSearch to reject maxResults');
      } catch (error) {
        expect(error).toBeInstanceOf(DirectToolInputError);
        const details = (error as DirectToolInputError).details.join(' ');
        expect(details).toContain(
          `'maxResults' → did you mean '${expectedField}'?`
        );
      }
    }
  );

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
