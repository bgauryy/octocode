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

  it('derives useful command fields from the GitHub code search schema', () => {
    const patterns = buildDirectToolCommandPatterns(
      STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE
    );

    expect(patterns).toHaveLength(1);
    expect(patterns[0]).toMatchObject({
      label: 'schema-derived',
      query: {
        keywords: ['runCLI'],
        owner: 'facebook',
        repo: 'react',
        extension: 'ts',
      },
    });
    expect(patterns[0]?.query).not.toHaveProperty('page');
    expect(patterns[0]?.query).not.toHaveProperty('limit');
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
