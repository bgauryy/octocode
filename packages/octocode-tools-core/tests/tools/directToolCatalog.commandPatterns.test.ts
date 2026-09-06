import { describe, expect, it } from 'vitest';

import {
  buildDirectToolCommandPatterns,
  buildDirectToolExampleQuery,
} from '../../src/tools/directToolCatalog/toolCommandPatterns.js';
import {
  DIRECT_TOOL_CATEGORIES,
  getDirectToolCategory,
} from '../../src/tools/directToolCatalog/toolCatalogDefinitions.js';
import {
  LSP_GET_SEMANTICS_TOOL_NAME,
  LOCAL_SEARCH_TOOL_NAME,
  STATIC_TOOL_NAMES,
} from '../../src/tools/toolNames.js';

describe('direct-tool command patterns', () => {
  it('uses operation-aware patterns for unified localSearch inputs', () => {
    const patterns = buildDirectToolCommandPatterns(LOCAL_SEARCH_TOOL_NAME);

    expect(patterns).toHaveLength(4);
    expect(patterns[0]).toMatchObject({
      label: 'text anchors',
      query: {
        operation: 'text',
        path: '/ABS/repo/src',
        searchText: 'buildDirectToolCommandPatterns',
        maxFiles: 20,
      },
    });
    expect(patterns[0]?.command).toBe(
      'tools localSearch --queries \'{"operation":"text","path":"/ABS/repo/src","searchText":"buildDirectToolCommandPatterns","maxFiles":20}\''
    );
    expect(patterns[1]).toMatchObject({
      label: 'structural matches',
      query: {
        operation: 'structural',
        path: '/ABS/repo/src',
        pattern: 'eval($X)',
        langType: 'typescript',
      },
    });
    expect(patterns[2]?.query).toMatchObject({ operation: 'files' });
    expect(patterns[3]?.query).toMatchObject({ operation: 'tree' });
    expect(buildDirectToolExampleQuery(LOCAL_SEARCH_TOOL_NAME)).toEqual({
      operation: 'text',
      path: '/ABS/repo/src',
      searchText: 'buildDirectToolCommandPatterns',
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

  it('keeps semantic patterns compact for definition and outline flows', () => {
    const patterns = buildDirectToolCommandPatterns(
      LSP_GET_SEMANTICS_TOOL_NAME
    );

    expect(patterns.map(pattern => pattern.label)).toEqual([
      'symbol outline (absolute uri)',
      'semantic definition (absolute uri + lineHint)',
    ]);
    expect(patterns[0]?.query).toEqual({
      uri: '/ABS/packages/octocode-tools-core/src/scheme/pagination.ts',
      type: 'documentSymbols',
    });
    expect(patterns[1]?.query).toMatchObject({
      uri: '/ABS/packages/octocode-tools-core/src/scheme/pagination.ts',
      type: 'definition',
      symbolName: 'buildNextPageContinuation',
      lineHint: 72,
    });
  });

  it('groups structural search and semantic LSP under local code tooling', () => {
    const categoryLabels = DIRECT_TOOL_CATEGORIES as readonly string[];

    expect(DIRECT_TOOL_CATEGORIES).toContain('Local Code');
    expect(categoryLabels).not.toContain('LSP');
    expect(getDirectToolCategory(LOCAL_SEARCH_TOOL_NAME)).toBe('Local Code');
    expect(getDirectToolCategory(LSP_GET_SEMANTICS_TOOL_NAME)).toBe(
      'Local Code'
    );
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
        query: { keywords: ['schema', 'validation'], page: 1 },
      })
    );
  });

  it('generates no examples referencing facebook/react', () => {
    const allToolNames = [
      ...Object.values(STATIC_TOOL_NAMES),
      LSP_GET_SEMANTICS_TOOL_NAME,
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
