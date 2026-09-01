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
    const { TOOL_DEFINITIONS } = await import('../../src/cli/tool-command.js');
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

  it.each(ADAPTER_PARITY_CASES)(
    '$name preserves schema, result, row error, outer error, and executable continuation contracts',
    async testCase => {
      const { TOOL_DEFINITIONS, executeToolCommand } =
        await import('../../src/cli/tool-command.js');
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
});
