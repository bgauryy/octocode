import { describe, expect, it } from 'vitest';

import {
  buildDirectToolCommandPatterns,
  buildDirectToolExampleQuery,
  DIRECT_TOOL_CATEGORIES,
  getDirectToolCategory,
} from '../../src/tools/directToolCatalog.meta.js';
import {
  LSP_GET_SEMANTICS_TOOL_NAME,
  STATIC_TOOL_NAMES,
} from '../../src/tools/toolNames.js';

describe('direct-tool command patterns', () => {
  it('uses workflow-aware patterns for localSearchCode conditional inputs', () => {
    const patterns = buildDirectToolCommandPatterns(
      STATIC_TOOL_NAMES.LOCAL_RIPGREP
    );

    expect(patterns).toHaveLength(2);
    expect(patterns[0]).toMatchObject({
      label: 'text search',
      query: {
        path: 'packages/octocode-tools-core/src',
        keywords: 'buildDirectToolCommandPatterns',
        maxFiles: 20,
      },
    });
    expect(patterns[0]?.command).toBe(
      'tools localSearchCode --queries \'{"path":"packages/octocode-tools-core/src","keywords":"buildDirectToolCommandPatterns","maxFiles":20}\''
    );
    expect(patterns[1]).toMatchObject({
      label: 'structural code search',
      query: {
        path: 'packages/octocode-tools-core/src/tools',
        mode: 'structural',
        pattern: 'eval($X)',
      },
    });
    expect(
      buildDirectToolExampleQuery(STATIC_TOOL_NAMES.LOCAL_RIPGREP)
    ).toEqual({
      path: 'packages/octocode-tools-core/src',
      keywords: 'buildDirectToolCommandPatterns',
      maxFiles: 20,
    });
  });

  it('curates localViewStructure examples without schema placeholders', () => {
    const patterns = buildDirectToolCommandPatterns(
      STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE
    );

    expect(patterns.map(pattern => pattern.label)).toEqual([
      'shallow tree',
      'files only at depth 1',
    ]);
    expect(patterns[0]?.query).toEqual({
      path: 'packages/octocode-tools-core/src/tools',
      maxDepth: 2,
      itemsPerPage: 50,
    });
    expect(JSON.stringify(patterns[0]?.query)).not.toContain('"pattern"');
    expect(JSON.stringify(patterns[0]?.query)).not.toContain('"extensions"');
  });

  it('uses curated path/content patterns for GitHub code search', () => {
    const patterns = buildDirectToolCommandPatterns(
      STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE
    );

    expect(patterns.map(pattern => pattern.label)).toEqual([
      'path search',
      'content search',
    ]);
    expect(patterns[0]).toMatchObject({
      label: 'path search',
      query: {
        keywords: ['package.json'],
        owner: 'bgauryy',
        repo: 'octocode',
        match: 'path',
        limit: 5,
      },
    });
    expect(patterns[1]).toMatchObject({
      label: 'content search',
      query: {
        keywords: ['localSearchCode'],
        owner: 'bgauryy',
        repo: 'octocode',
        extension: 'ts',
        limit: 5,
      },
    });
    expect(patterns[0]?.command).toContain('tools ghSearchCode --queries');
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
      expect(getDirectToolCategory(STATIC_TOOL_NAMES.LOCAL_RIPGREP)).toBe(
        'Local Code'
      );
      expect(getDirectToolCategory(LSP_GET_SEMANTICS_TOOL_NAME)).toBe(
        'Local Code'
      );
    });

    it('returns no patterns for unknown tools', () => {
      expect(buildDirectToolCommandPatterns('missingTool')).toEqual([]);
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
