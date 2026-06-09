import { describe, expect, it, vi } from 'vitest';
import type { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import {
  registerSingleTool,
  registerToolsBatch,
  summarizeOutcomes,
} from '../../src/tools/registrationExecutor.js';
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

describe('registrationExecutor', () => {
  it('returns skipped when metadata policy rejects tool', async () => {
    const tool = makeTool({
      name: 't1',
      fn: vi.fn(),
    });
    const outcome = await registerSingleTool(
      tool,
      {} as McpServer,
      undefined,
      () => false
    );
    expect(outcome).toEqual({ status: 'skipped' });
    expect(tool.fn).not.toHaveBeenCalled();
  });

  it('returns failed with tool name and diagnostic when registration throws', async () => {
    const tool = makeTool({
      name: 't2',
      fn: vi.fn().mockRejectedValue(new Error('fail')),
    });
    const outcome = await registerSingleTool(
      tool,
      {} as McpServer,
      undefined,
      () => true
    );
    expect(outcome).toEqual({
      status: 'failed',
      toolName: 't2',
      error: 'fail',
    });
  });

  it('batch executes and summarizes outcomes', async () => {
    const t1 = makeTool({ name: 'a', fn: vi.fn().mockResolvedValue({}) });
    const t2 = makeTool({ name: 'b', fn: vi.fn().mockResolvedValue(null) });
    const t3 = makeTool({
      name: 'c',
      fn: vi.fn().mockRejectedValue(new Error('bad')),
    });

    const outcomes = await registerToolsBatch(
      [t1, t2, t3],
      {} as McpServer,
      undefined,
      () => true
    );

    const summary = summarizeOutcomes(outcomes);
    expect(summary.successCount).toBe(1);
    expect(summary.failedTools).toEqual(['c']);
    expect(summary.failedToolErrors).toEqual({ c: 'bad' });
  });
});
