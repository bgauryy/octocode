import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { McpServer } from '@modelcontextprotocol/server';
import { registerTools } from '../../src/tools/toolsManager.js';

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
      toolsToRun: ['ghGetPullRequest'],
    } as ReturnType<typeof getServerConfig>);
    const toolFn = vi.fn().mockResolvedValue({});

    await expect(
      registerTools(mockServer, undefined, {
        toolLoader: () => [
          {
            name: 'ghSearchPullRequests',
            isDefault: true,
            isLocal: false,
            fn: toolFn,
          },
        ],
      })
    ).rejects.toThrow(/ghGetPullRequest.*ghSearchPullRequests/i);
    expect(toolFn).not.toHaveBeenCalled();
  });

  it('warns but registers valid tools from a mixed allowlist', async () => {
    mockGetServerConfig.mockReturnValue({
      toolsToRun: ['ghSearchIssues', 'ghGetPullRequest'],
    } as ReturnType<typeof getServerConfig>);
    const stderr = vi.spyOn(process.stderr, 'write').mockReturnValue(true);
    const toolFn = vi.fn().mockResolvedValue({});

    const result = await registerTools(mockServer, undefined, {
      toolLoader: () => [
        {
          name: 'ghSearchIssues',
          isDefault: true,
          isLocal: false,
          fn: toolFn,
        },
        {
          name: 'ghSearchPullRequests',
          isDefault: true,
          isLocal: false,
          fn: vi.fn().mockResolvedValue({}),
        },
      ],
    });

    expect(result.successCount).toBe(1);
    expect(toolFn).toHaveBeenCalledTimes(1);
    expect(stderr).toHaveBeenCalledWith(
      expect.stringMatching(/ghGetPullRequest.*ghSearchPullRequests/i)
    );
    stderr.mockRestore();
  });
});
