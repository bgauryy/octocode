import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { registerTools } from '../../src/tools/toolsManager.js';

vi.mock('../../../octocode-tools-core/src/serverConfig.js', () => ({
  getServerConfig: vi.fn(),
  isLocalEnabled: vi.fn(),
  isCloneEnabled: vi.fn(),
}));

vi.mock(
  '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js',
  async importOriginal => ({
    ...(await importOriginal<object>()),
    isToolInMetadata: vi.fn(),
  })
);

vi.mock('../../src/utils/secureServer.js', () => ({
  withOutputSanitization: vi.fn((server: unknown) => server),
}));

import {
  getServerConfig,
  isLocalEnabled,
  isCloneEnabled,
} from '../../../octocode-tools-core/src/serverConfig.js';
import { isToolInMetadata } from '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js';

const mockGetServerConfig = vi.mocked(getServerConfig);
const mockIsLocalEnabled = vi.mocked(isLocalEnabled);
const mockIsCloneEnabled = vi.mocked(isCloneEnabled);
const mockIsToolInMetadata = vi.mocked(isToolInMetadata);

describe('ToolsManager - Dependency Injection', () => {
  let mockServer: McpServer;

  beforeEach(() => {
    vi.clearAllMocks();
    mockServer = {} as McpServer;
    mockGetServerConfig.mockReturnValue(
      {} as ReturnType<typeof getServerConfig>
    );
    mockIsLocalEnabled.mockReturnValue(false);
    mockIsCloneEnabled.mockReturnValue(false);
    mockIsToolInMetadata.mockReturnValue(true);
  });

  it('uses injected tool loader instead of default catalog', async () => {
    const toolFn = vi.fn().mockResolvedValue({});
    const toolLoader = vi.fn().mockResolvedValue([
      {
        name: 'injectedTool',
        isDefault: true,
        isLocal: false,
        fn: toolFn,
      },
    ]);

    const result = await registerTools(mockServer, undefined, { toolLoader });

    expect(toolLoader).toHaveBeenCalledTimes(1);
    expect(result).toEqual({
      successCount: 1,
      failedTools: [],
    });
    expect(toolFn).toHaveBeenCalledTimes(1);
  });

  it('supports skipMetadataCheck on injected tools and tracks failures', async () => {
    const throwingFn = vi.fn().mockRejectedValue(new Error('fail'));
    const toolLoader = vi.fn().mockReturnValue([
      {
        name: 'skipMetadataTool',
        isDefault: true,
        isLocal: false,
        skipMetadataCheck: true,
        fn: throwingFn,
      },
    ]);

    const result = await registerTools(mockServer, undefined, { toolLoader });

    expect(result).toEqual({
      successCount: 0,
      failedTools: ['skipMetadataTool'],
      failedToolErrors: { skipMetadataTool: 'fail' },
    });
    expect(mockIsToolInMetadata).not.toHaveBeenCalled();
  });
});
