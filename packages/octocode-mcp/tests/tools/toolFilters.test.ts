import { describe, expect, it } from 'vitest';
import {
  getToolFilterConfigSafe,
  isToolEnabled,
  validateToolFilterConfig,
  type ToolFilterConfig,
} from '../../src/tools/toolFilters.js';
import type { ToolConfig } from '../../src/tools/toolConfig.js';
import { z } from 'zod';

function makeTool(
  overrides: Partial<ToolConfig> & Pick<ToolConfig, 'name'>
): ToolConfig {
  return {
    name: overrides.name,
    title: overrides.title ?? overrides.name,
    description: '',
    isDefault: overrides.isDefault ?? true,
    isLocal: overrides.isLocal ?? false,
    isClone: overrides.isClone,
    type: overrides.type ?? 'search',
    direct: overrides.direct ?? {
      schema: z.object({}),
      inputSchema: z.object({}),
      executionFn: async () => ({ content: [] }),
      security: 'basic',
    },
  };
}

describe('toolFilters', () => {
  it('returns safe defaults when config provider throws', () => {
    const cfg = getToolFilterConfigSafe(() => {
      throw new Error('not initialized');
    });
    expect(cfg).toEqual({
      toolsToRun: [],
      disableTools: [],
    });
  });

  it('normalizes omitted filter lists from a valid config provider', () => {
    expect(getToolFilterConfigSafe(() => ({}))).toEqual({
      toolsToRun: [],
      disableTools: [],
    });
  });

  it('honors local and clone gates before list filters', () => {
    const localCloneTool = makeTool({
      name: 'clone',
      isLocal: true,
      isClone: true,
      isDefault: true,
    });

    const cfg: ToolFilterConfig = {
      toolsToRun: [],
      disableTools: [],
    };

    expect(
      isToolEnabled(localCloneTool, {
        localEnabled: true,
        cloneEnabled: true,
        filterConfig: cfg,
      })
    ).toBe(true);

    expect(
      isToolEnabled(localCloneTool, {
        localEnabled: false,
        cloneEnabled: true,
        filterConfig: cfg,
      })
    ).toBe(false);

    expect(
      isToolEnabled(localCloneTool, {
        localEnabled: true,
        cloneEnabled: false,
        filterConfig: cfg,
      })
    ).toBe(false);
  });

  it('applies precedence toolsToRun > disableTools > isDefault', () => {
    const tool = makeTool({ name: 'x', isDefault: false });

    expect(
      isToolEnabled(tool, {
        localEnabled: true,
        cloneEnabled: true,
        filterConfig: {
          toolsToRun: ['x'],
          disableTools: ['x'],
        },
      })
    ).toBe(true);

    expect(
      isToolEnabled(tool, {
        localEnabled: true,
        cloneEnabled: true,
        filterConfig: {
          toolsToRun: [],
          disableTools: ['x'],
        },
      })
    ).toBe(false);

    expect(
      isToolEnabled(tool, {
        localEnabled: true,
        cloneEnabled: true,
        filterConfig: { toolsToRun: [], disableTools: [] },
      })
    ).toBe(false);
  });

  it('rejects an all-invalid strict allowlist with a close suggestion', () => {
    expect(() =>
      validateToolFilterConfig(
        {
          toolsToRun: ['ghGetHistory'],
          disableTools: [],
        },
        ['ghSearchHistory', 'ghGetHistoryItem']
      )
    ).toThrowError(/Unknown tool name.*ghGetHistory.*ghGetHistoryItem/i);
  });

  it('keeps valid names and reports invalid names in mixed filters', () => {
    const result = validateToolFilterConfig(
      {
        toolsToRun: ['ghSearchHistory', 'ghGetHistory'],
        disableTools: ['local.text', 'missing'],
      },
      ['ghSearchHistory', 'ghGetHistoryItem', 'npmSearch', 'local.text']
    );

    expect(result.config).toEqual({
      toolsToRun: ['ghSearchHistory'],
      disableTools: ['local.text'],
    });
    expect(result.warnings.join('\n')).toMatch(/ghGetHistory/);
    expect(result.warnings.join('\n')).toMatch(/missing/);
  });
});
