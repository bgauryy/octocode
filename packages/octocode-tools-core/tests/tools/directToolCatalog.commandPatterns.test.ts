import { describe, expect, it } from 'vitest';

import {
  buildDirectToolCommandPatterns,
  buildDirectToolExampleQuery,
  DIRECT_TOOL_CATEGORIES,
  getDirectToolCategory,
  LSP_SEARCH_TOOL_NAME,
  LOCAL_SEARCH_TOOL_NAME,
  STATIC_TOOL_NAMES,
} from '@octocodeai/octocode-core/schema';

describe('direct-tool command patterns', () => {
  it('uses lexical patterns for localSearch inputs', () => {
    const patterns = buildDirectToolCommandPatterns(LOCAL_SEARCH_TOOL_NAME);

    expect(patterns).toHaveLength(1);
    expect(patterns[0]).toMatchObject({
      label: 'text anchors',
      query: {
        path: '/ABS/repo/src',
        searchText: 'buildDirectToolCommandPatterns',
        regex: 'literal',
        maxFiles: 20,
      },
    });
    expect(patterns[0]?.command).toBe(
      'tools localSearch --queries \'{"path":"/ABS/repo/src","searchText":"buildDirectToolCommandPatterns","regex":"literal","maxFiles":20}\''
    );
    expect(buildDirectToolExampleQuery(LOCAL_SEARCH_TOOL_NAME)).toEqual({
      path: '/ABS/repo/src',
      searchText: 'buildDirectToolCommandPatterns',
      regex: 'literal',
      maxFiles: 20,
    });
  });

  it('uses one operation-aware pattern set for GitHub discovery', () => {
    const patterns = buildDirectToolCommandPatterns('ghSearch');

    expect(patterns.map(pattern => pattern.label)).toEqual([
      'code search',
      'repository search',
      'repository tree',
    ]);
    expect(patterns[0]).toMatchObject({
      label: 'code search',
      query: {
        operation: 'code',
        keywords: ['localSearch'],
        owner: 'bgauryy',
        repo: 'octocode',
        pageSize: 5,
      },
    });
    expect(patterns[1]).toMatchObject({
      label: 'repository search',
      query: {
        operation: 'repositories',
        pageSize: 5,
      },
    });
    expect(patterns[0]?.command).toContain('tools ghSearch --queries');
  });

  it('starts semantic patterns with anchored definition and references', () => {
    const patterns = buildDirectToolCommandPatterns(LSP_SEARCH_TOOL_NAME);

    expect(patterns.map(pattern => pattern.label)).toEqual([
      'semantic definition (absolute uri + lineHint)',
      'symbol references from an observed anchor',
      'symbol outline (absolute uri)',
    ]);
    expect(patterns[2]?.query).toEqual({
      uri: '/ABS/packages/octocode-tools-core/src/scheme/pagination.ts',
      operation: 'documentSymbols',
    });
    expect(patterns[0]?.query).toMatchObject({
      uri: '/ABS/packages/octocode-tools-core/src/scheme/pagination.ts',
      operation: 'definition',
      symbolName: 'buildNextPageContinuation',
      lineHint: 72,
    });
    expect(patterns[1]?.query).toMatchObject({
      operation: 'references',
      symbolName: 'run',
      lineHint: 10,
      includeDeclaration: false,
    });
  });

  it('groups structural search and semantic LSP under local code tooling', () => {
    const categoryLabels = DIRECT_TOOL_CATEGORIES as readonly string[];

    expect(DIRECT_TOOL_CATEGORIES).toContain('Local Code');
    expect(categoryLabels).not.toContain('LSP');
    expect(getDirectToolCategory(LOCAL_SEARCH_TOOL_NAME)).toBe('Local Code');
    expect(getDirectToolCategory(LSP_SEARCH_TOOL_NAME)).toBe('Local Code');
  });

  it('returns no patterns for unknown tools', () => {
    expect(buildDirectToolCommandPatterns('missingTool')).toEqual([]);
  });

  it('uses the keywords selector for npm keyword discovery', () => {
    const patterns = buildDirectToolCommandPatterns(
      STATIC_TOOL_NAMES.PACKAGE_SEARCH
    );
    expect(patterns).toContainEqual(
      expect.objectContaining({
        label: 'keyword discovery (paged candidates)',
        query: { type: 'npm', keywords: ['schema', 'validation'] },
      })
    );
  });

  it('generates no examples referencing facebook/react', () => {
    const allToolNames = [
      ...Object.values(STATIC_TOOL_NAMES),
      LSP_SEARCH_TOOL_NAME,
    ];
    for (const name of allToolNames) {
      const patterns = buildDirectToolCommandPatterns(name);
      for (const pattern of patterns) {
        const serialized = JSON.stringify(pattern.query);
        expect(serialized).not.toContain('facebook');
        expect(pattern.command ?? '').not.toContain('facebook');
        // repo field should not be 'react' when owner context implies GitHub
        if (
          typeof pattern.query === 'object' &&
          pattern.query !== null &&
          'owner' in pattern.query
        ) {
          expect((pattern.query as Record<string, unknown>).owner).not.toBe(
            'facebook'
          );
        }
      }
    }
  });
});
