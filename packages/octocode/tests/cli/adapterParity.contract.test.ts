import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  ADAPTER_PARITY_CASES,
  CANONICAL_ADAPTER_TOOL_NAMES,
  createAdapterParityErrorResult,
  createAdapterParityPageResult,
  getAdapterParityContinuation,
} from '../../../octocode-tools-core/tests/fixtures/adapterParityFixture.js';

const directExecution = vi.hoisted(() => vi.fn());

vi.mock('@octocodeai/octocode-tools-core/direct', () => ({
  executeDirectTool: directExecution,
  formatCallToolResultForOutput: (result: unknown) => JSON.stringify(result),
}));

describe('raw tools command adapter parity', () => {
  let consoleSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    vi.resetModules();
    process.env.ENABLE_LOCAL = 'true';
    process.env.ENABLE_CLONE = 'true';
    delete process.env.TOOLS_TO_RUN;
    delete process.env.DISABLE_TOOLS;
    process.exitCode = undefined;
    consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});

    const invocationCount = new Map<string, number>();
    directExecution.mockImplementation(async (name: string) => {
      const count = (invocationCount.get(name) ?? 0) + 1;
      invocationCount.set(name, count);
      const testCase = ADAPTER_PARITY_CASES.find(item => item.name === name)!;
      if (count === 1) return createAdapterParityPageResult(testCase, 1);
      if (count === 2) return createAdapterParityPageResult(testCase, 2);
      return createAdapterParityErrorResult(testCase);
    });
  });

  afterEach(() => {
    consoleSpy.mockRestore();
    delete process.env.ENABLE_LOCAL;
    delete process.env.ENABLE_CLONE;
    delete process.env.TOOLS_TO_RUN;
    delete process.env.DISABLE_TOOLS;
    process.exitCode = undefined;
    directExecution.mockReset();
  });

  function readCompactOutput(): Record<string, unknown> {
    const call = consoleSpy.mock.calls.at(-1);
    expect(call).toBeDefined();
    return JSON.parse(String(call![0])) as Record<string, unknown>;
  }

  it('exposes the exact canonical fixture tool set from the schema registry', async () => {
    const { TOOL_DEFINITIONS } =
      await import('../../src/cli/tool-command/registry.js');
    const { DIRECT_TOOL_DISCOVERY_DEFINITIONS } =
      await import('@octocodeai/octocode-tools-core/schema');
    expect(TOOL_DEFINITIONS.map(tool => tool.name)).toEqual(
      CANONICAL_ADAPTER_TOOL_NAMES
    );
    for (const tool of TOOL_DEFINITIONS) {
      const canonical = DIRECT_TOOL_DISCOVERY_DEFINITIONS.find(
        definition => definition.name === tool.name
      )!;
      expect(tool.inputSchema).toBe(canonical.inputSchema);
    }
  });

  it('marks every local tool disabled when ENABLE_LOCAL=false', async () => {
    process.env.ENABLE_LOCAL = 'false';
    const { TOOL_DEFINITIONS } =
      await import('../../src/cli/tool-command/registry.js');
    const localNames = [
      'localSearch',
      'localGetFileContent',
      'localAnalyzeGraph',
      'lspGetSemantics',
      'ghCloneRepo',
    ];
    for (const name of localNames) {
      expect(
        TOOL_DEFINITIONS.find(tool => tool.name === name)?.disabled
      ).toEqual({ envVar: 'ENABLE_LOCAL' });
    }
  });

  it.each([
    ['TOOLS_TO_RUN', 'npmSearch', 'add ghSearch to TOOLS_TO_RUN'],
    ['DISABLE_TOOLS', 'ghSearch', 'remove ghSearch from DISABLE_TOOLS'],
  ])('explains the effective %s gate', async (key, value, instruction) => {
    process.env[key] = value;
    const { getToolEnableInstruction } =
      await import('../../src/cli/tool-command/registry.js');
    expect(getToolEnableInstruction('ghSearch')).toBe(instruction);
    expect(getToolEnableInstruction('npmSearch')).toBeUndefined();
  });

  it.each(ADAPTER_PARITY_CASES)(
    '$name preserves schema, result, row error, outer error, and executable continuation contracts',
    async testCase => {
      const { TOOL_DEFINITIONS } =
        await import('../../src/cli/tool-command/registry.js');
      const { executeToolCommand } =
        await import('../../src/cli/tool-command/execute.js');
      const definition = TOOL_DEFINITIONS.find(
        tool => tool.name === testCase.name
      )!;
      expect(definition.inputSchema.safeParse(testCase.input).success).toBe(
        true
      );

      await expect(
        executeToolCommand({
          command: 'tools',
          args: [testCase.name],
          options: {
            compact: true,
            queries: JSON.stringify(testCase.input),
          },
        })
      ).resolves.toBe(true);
      expect(readCompactOutput()).toEqual(
        createAdapterParityPageResult(testCase, 1).structuredContent
      );
      expect(directExecution).toHaveBeenLastCalledWith(
        testCase.name,
        expect.any(Object),
        { resultProjection: 'structured' }
      );
      expect(readCompactOutput()).toMatchObject({
        results: [
          { index: 0 },
          {
            index: 1,
            status: 'error',
            data: { error: { code: 'ADAPTER_FIXTURE_ROW_ERROR' } },
          },
        ],
      });

      const next = getAdapterParityContinuation(
        createAdapterParityPageResult(testCase, 1)
      );
      const continuationInput = { queries: [next.query] };
      expect(definition.inputSchema.safeParse(continuationInput).success).toBe(
        true
      );
      await expect(
        executeToolCommand({
          command: 'tools',
          args: [next.tool],
          options: {
            compact: true,
            queries: JSON.stringify(continuationInput),
          },
        })
      ).resolves.toBe(true);
      expect(readCompactOutput()).toEqual(
        createAdapterParityPageResult(testCase, 2).structuredContent
      );

      await expect(
        executeToolCommand({
          command: 'tools',
          args: [testCase.name],
          options: {
            compact: true,
            queries: JSON.stringify(testCase.input),
          },
        })
      ).resolves.toBe(false);
      expect(readCompactOutput()).toEqual(
        createAdapterParityErrorResult(testCase).structuredContent
      );
    }
  );

  it.each<Record<string, boolean>>([{}, { json: true }])(
    'keeps full direct output for %j',
    async options => {
      const testCase = ADAPTER_PARITY_CASES[0]!;
      const { executeToolCommand } =
        await import('../../src/cli/tool-command/execute.js');
      await executeToolCommand({
        command: 'tools',
        args: [testCase.name],
        options: { ...options, queries: JSON.stringify(testCase.input) },
      });
      expect(directExecution).toHaveBeenLastCalledWith(
        testCase.name,
        expect.any(Object)
      );
    }
  );

  it('preserves text-only compact fallback', async () => {
    const testCase = ADAPTER_PARITY_CASES[0]!;
    const fallback = {
      content: [{ type: 'text', text: 'sanitized fallback' }],
    };
    directExecution.mockResolvedValueOnce(fallback);
    const { executeToolCommand } =
      await import('../../src/cli/tool-command/execute.js');
    await executeToolCommand({
      command: 'tools',
      args: [testCase.name],
      options: { compact: true, queries: JSON.stringify(testCase.input) },
    });
    expect(readCompactOutput()).toEqual(fallback);
  });

  it('preserves exit classification when only error text identifies the failure', async () => {
    const testCase = ADAPTER_PARITY_CASES[0]!;
    directExecution.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'rate limit exceeded' }],
      structuredContent: { results: [], status: 'error' },
      isError: true,
    });
    const { executeToolCommand } =
      await import('../../src/cli/tool-command/execute.js');
    await expect(
      executeToolCommand({
        command: 'tools',
        args: [testCase.name],
        options: { compact: true, queries: JSON.stringify(testCase.input) },
      })
    ).resolves.toBe(false);
    expect(process.exitCode).toBe(7);
    expect(readCompactOutput()).toEqual({ results: [], status: 'error' });
  });
});
