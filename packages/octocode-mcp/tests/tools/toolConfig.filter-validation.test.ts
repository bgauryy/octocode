import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { McpServer, RegisteredTool } from '@modelcontextprotocol/server';
import { registerTools } from '../../src/tools/toolsManager.js';
import { ALL_TOOLS, type McpToolConfig } from '../../src/tools/toolConfig.js';

vi.mock(
  '../../../octocode-tools-core/src/serverConfig.js',
  async importOriginal => ({
    ...(await importOriginal<object>()),
    getServerConfig: vi.fn(),
    isLocalEnabled: vi.fn(() => false),
    isCloneEnabled: vi.fn(() => false),
  })
);

vi.mock('../../src/utils/secureServer.js', () => ({
  withOutputSanitization: vi.fn((server: unknown) => server),
}));

import { getServerConfig } from '../../../octocode-tools-core/src/serverConfig.js';

const mockGetServerConfig = vi.mocked(getServerConfig);

function registeredTool(): RegisteredTool {
  return {
    handler: vi.fn(async () => ({ content: [] })),
    executor: vi.fn(async () => ({ content: [] })),
    enabled: true,
    enable: vi.fn(),
    disable: vi.fn(),
    update: vi.fn(),
    remove: vi.fn(),
  };
}

function toolWithRegistration(
  name: string,
  fn: McpToolConfig['fn']
): McpToolConfig {
  const tool = ALL_TOOLS.find(candidate => candidate.name === name);
  if (!tool) throw new Error(`Missing test tool: ${name}`);
  return { ...tool, fn };
}

describe('ToolsManager filter validation', () => {
  const mockServer = {} as McpServer;

  beforeEach(() => {
    vi.clearAllMocks();
    mockGetServerConfig.mockReturnValue(
      {} as ReturnType<typeof getServerConfig>
    );
  });

  it('fails before registration when TOOLS_TO_RUN contains no valid names', async () => {
    mockGetServerConfig.mockReturnValue({
      toolsToRun: ['ghGetHistory'],
    } as ReturnType<typeof getServerConfig>);
    const toolFn = vi.fn(() => registeredTool());

    await expect(
      registerTools(mockServer, undefined, {
        toolLoader: () => [toolWithRegistration('ghGetHistoryItem', toolFn)],
      })
    ).rejects.toThrow(/ghGetHistory.*ghGetHistoryItem/i);
    expect(toolFn).not.toHaveBeenCalled();
  });

  it('warns but registers valid tools from a mixed allowlist', async () => {
    mockGetServerConfig.mockReturnValue({
      toolsToRun: ['ghSearchHistory', 'ghGetHistory'],
    } as ReturnType<typeof getServerConfig>);
    const stderr = vi.spyOn(process.stderr, 'write').mockReturnValue(true);
    const toolFn = vi.fn(() => registeredTool());

    const result = await registerTools(mockServer, undefined, {
      toolLoader: () => [
        toolWithRegistration('ghSearchHistory', toolFn),
        toolWithRegistration('ghGetHistoryItem', () => registeredTool()),
      ],
    });

    expect(result.successCount).toBe(1);
    expect(toolFn).toHaveBeenCalledTimes(1);
    expect(stderr).toHaveBeenCalledWith(
      expect.stringMatching(/ghGetHistory.*ghGetHistoryItem/i)
    );
    stderr.mockRestore();
  });
});
