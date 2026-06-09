import { describe, expect, it } from 'vitest';
import {
  getToolFilterConfigSafe,
  hasToolFilterConflict,
  isToolEnabled,
  TOOL_FILTER_CONFLICT_WARNING,
  type ToolFilterConfig,
} from '../../src/tools/toolFilters.js';
import type { ToolConfig } from '../../src/tools/toolConfig.js';
import { z } from 'zod';

function makeTool(
  overrides: Partial<ToolConfig> & Pick<ToolConfig, 'name'>
): ToolConfig {
  return {
    name: overrides.name,
    description: '',
    isDefault: overrides.isDefault ?? true,
    isLocal: overrides.isLocal ?? false,
    isClone: overrides.isClone,
    type: overrides.type ?? 'search',
    skipMetadataCheck: overrides.skipMetadataCheck,
    fn: overrides.fn ?? (() => ({}) as never),
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
      enableTools: [],
      disableTools: [],
    });
  });

  it('detects toolsToRun conflict with enable/disable lists', () => {
    const cfg: ToolFilterConfig = {
      toolsToRun: ['a'],
      enableTools: ['b'],
      disableTools: [],
    };
    expect(hasToolFilterConflict(cfg)).toBe(true);
    expect(TOOL_FILTER_CONFLICT_WARNING).toContain('TOOLS_TO_RUN');
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
      enableTools: [],
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
        localEnabled: true,
        cloneEnabled: false,
        filterConfig: cfg,
      })
    ).toBe(false);
  });

  it('applies precedence toolsToRun > disableTools > enableTools > isDefault', () => {
    const tool = makeTool({ name: 'x', isDefault: false });

    expect(
      isToolEnabled(tool, {
        localEnabled: true,
        cloneEnabled: true,
        filterConfig: {
          toolsToRun: ['x'],
          enableTools: [],
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
          enableTools: ['x'],
          disableTools: ['x'],
        },
      })
    ).toBe(false);

    expect(
      isToolEnabled(tool, {
        localEnabled: true,
        cloneEnabled: true,
        filterConfig: {
          toolsToRun: [],
          enableTools: ['x'],
          disableTools: [],
        },
      })
    ).toBe(true);
  });
});
