// Catalog-shape switches (S13 arms): opt-in, defaults keep the current catalog.
import { describe, expect, it } from 'vitest';

import { resolveConfigFields } from '../src/index.js';

describe('catalog-shape switches', () => {
  it('default to the full catalog and no deferral', () => {
    const resolved = resolveConfigFields({}, {});
    expect(resolved.tools.family).toBe('all');
    expect(resolved.mcp).toEqual({ deferred: null });
  });

  it('resolve from the environment', () => {
    const resolved = resolveConfigFields(
      {},
      {
        OCTOCODE_TOOL_FAMILY: 'GitHub',
        OCTOCODE_DEFER_TOOLS: 'clasify,ghSearchRepo',
      }
    );
    expect(resolved.tools.family).toBe('github');
    expect(resolved.mcp).toEqual({ deferred: ['clasify', 'ghSearchRepo'] });
  });

  it('resolve from .octocoderc and ignore an unknown family', () => {
    expect(
      resolveConfigFields(
        { tools: { family: 'local' }, mcp: { deferred: ['clasify'] } },
        {}
      ).tools.family
    ).toBe('local');
    expect(
      resolveConfigFields({}, { OCTOCODE_TOOL_FAMILY: 'remote' }).tools.family
    ).toBe('all');
  });
});
