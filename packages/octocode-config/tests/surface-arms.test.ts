// Catalog-shape switch (S13 arm): opt-in, defaults keep the current catalog.
import { describe, expect, it } from 'vitest';

import { resolveConfigFields } from '../src/index.js';

describe('catalog-shape switches', () => {
  it('default to no deferral', () => {
    expect(resolveConfigFields({}, {}).mcp).toEqual({ deferred: null });
  });

  it('resolve from the environment', () => {
    const resolved = resolveConfigFields(
      {},
      { OCTOCODE_DEFER_TOOLS: 'clasify,ghSearchRepo' }
    );
    expect(resolved.mcp).toEqual({ deferred: ['clasify', 'ghSearchRepo'] });
  });

  it('resolve from .octocoderc', () => {
    expect(
      resolveConfigFields({ mcp: { deferred: ['clasify'] } }, {}).mcp
    ).toEqual({ deferred: ['clasify'] });
  });

  it('has no tools.family preset', () => {
    const resolved = resolveConfigFields({}, { OCTOCODE_TOOL_FAMILY: 'github' });
    expect(Object.keys(resolved.tools).sort()).toEqual(['disabled', 'enabled']);
  });
});
