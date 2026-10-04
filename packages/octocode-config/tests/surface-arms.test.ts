// Catalog-shape switches (S13 arms): opt-in, defaults keep the current catalog.
import { describe, expect, it } from 'vitest';

import { resolveConfigFields } from '../src/index.js';

describe('catalog-shape switches', () => {
  it('default to the full catalog, the queries view, no deferral and default instructions', () => {
    const resolved = resolveConfigFields({}, {});
    expect(resolved.tools.family).toBe('all');
    expect(resolved.mcp).toEqual({
      publishedView: 'queries',
      deferred: null,
      instructions: 'default',
    });
  });

  it('resolve from the environment', () => {
    const resolved = resolveConfigFields(
      {},
      {
        OCTOCODE_TOOL_FAMILY: 'GitHub',
        OCTOCODE_PUBLISHED_VIEW: 'flat',
        OCTOCODE_DEFER_TOOLS: 'clasify,ghSearchRepo',
        OCTOCODE_INSTRUCTIONS: 'guide',
      }
    );
    expect(resolved.tools.family).toBe('github');
    expect(resolved.mcp).toEqual({
      publishedView: 'flat',
      deferred: ['clasify', 'ghSearchRepo'],
      instructions: 'guide',
    });
  });

  it('resolve from .octocoderc and ignore an unknown family', () => {
    expect(
      resolveConfigFields(
        { tools: { family: 'local' }, mcp: { publishedView: 'flat' } },
        {}
      ).tools.family
    ).toBe('local');
    expect(
      resolveConfigFields({}, { OCTOCODE_TOOL_FAMILY: 'remote' }).tools.family
    ).toBe('all');
  });
});
