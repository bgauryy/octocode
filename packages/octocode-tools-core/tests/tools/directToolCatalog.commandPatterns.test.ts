import { describe, expect, it } from 'vitest';

import {
  buildDirectToolCommandPatterns,
  buildDirectToolExampleQuery,
  DIRECT_TOOL_CATEGORIES,
  getDirectToolCategory,
} from '../../src/tools/directToolCatalog.meta.js';
import {
  LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
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
        path: '.',
        keywords: 'runCLI',
      },
    });
    expect(patterns[0]?.command).toBe(
      'tools localSearchCode --queries \'{"path":".","keywords":"runCLI"}\''
    );
    expect(patterns[1]).toMatchObject({
        label: 'structural code search',
        query: {
          path: 'src',
        mode: 'structural',
        pattern: 'eval($X)',
      },
    });
    expect(
      buildDirectToolExampleQuery(STATIC_TOOL_NAMES.LOCAL_RIPGREP)
    ).toEqual({
      path: '.',
      keywords: 'runCLI',
    });
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
        owner: 'facebook',
        repo: 'react',
        match: 'path',
        limit: 5,
      },
    });
    expect(patterns[1]).toMatchObject({
      label: 'content search',
      query: {
        keywords: ['useState'],
        owner: 'facebook',
        repo: 'react',
        extension: 'js',
        limit: 5,
      },
    });
    expect(patterns[0]?.command).toContain('tools ghSearchCode --queries');
  });

    it('keeps semantic patterns compact for definition and outline flows', () => {
      const patterns = buildDirectToolCommandPatterns(
        LSP_GET_SEMANTIC_CONTENT_TOOL_NAME
      );

      expect(patterns.map(pattern => pattern.label)).toEqual([
        'semantic definition',
        'symbol outline',
      ]);
    expect(patterns[0]?.query).toMatchObject({
      uri: '/path/to/file.ts',
      type: 'definition',
      symbolName: 'myFunction',
      lineHint: 42,
    });
    expect(patterns[1]?.query).toEqual({
      uri: '/path/to/file.ts',
      type: 'documentSymbols',
      });
    });

    it('groups structural search and semantic LSP under local code tooling', () => {
      const categoryLabels = DIRECT_TOOL_CATEGORIES as readonly string[];

      expect(DIRECT_TOOL_CATEGORIES).toContain('Local Code');
      expect(categoryLabels).not.toContain('LSP');
      expect(getDirectToolCategory(STATIC_TOOL_NAMES.LOCAL_RIPGREP)).toBe(
        'Local Code'
      );
      expect(getDirectToolCategory(LSP_GET_SEMANTIC_CONTENT_TOOL_NAME)).toBe(
        'Local Code'
      );
    });

    it('returns no patterns for unknown tools', () => {
      expect(buildDirectToolCommandPatterns('missingTool')).toEqual([]);
    });
});
