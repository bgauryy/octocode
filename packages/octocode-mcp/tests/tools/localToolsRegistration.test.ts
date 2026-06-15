import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { registerTools } from '../../src/tools/toolsManager.js';
import { STATIC_TOOL_NAMES } from '../../../octocode-tools-core/src/tools/toolNames.js';

type MockServer = {
  registerTool: (name: string, options: unknown, handler: unknown) => unknown;
};

vi.mock('../../src/tools/toolConfig.js', () => ({
  ALL_TOOLS: [
    {
      name: 'localSearchCode',
      isDefault: true,
      isLocal: true,
      fn: vi.fn((server: MockServer) => {
        server.registerTool(
          'localSearchCode',
          { inputSchema: { type: 'object' } },
          () => {}
        );
        return {};
      }),
    },
    {
      name: 'localViewStructure',
      isDefault: true,
      isLocal: true,
      fn: vi.fn((server: MockServer) => {
        server.registerTool(
          'localViewStructure',
          { inputSchema: { type: 'object' } },
          () => {}
        );
        return {};
      }),
    },
    {
      name: 'localFindFiles',
      isDefault: true,
      isLocal: true,
      fn: vi.fn((server: MockServer) => {
        server.registerTool(
          'localFindFiles',
          { inputSchema: { type: 'object' } },
          () => {}
        );
        return {};
      }),
    },
    {
      name: 'localGetFileContent',
      isDefault: true,
      isLocal: true,
      fn: vi.fn((server: MockServer) => {
        server.registerTool(
          'localGetFileContent',
          { inputSchema: { type: 'object' } },
          () => {}
        );
        return {};
      }),
    },
  ],
}));

vi.mock(
  '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js',
  async () => {
    const actual = await vi.importActual<
      typeof import('../../../octocode-tools-core/src/tools/toolMetadata/proxies.js')
    >('../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
    return {
      ...actual,
      isToolInMetadata: vi.fn().mockReturnValue(true),
    };
  }
);

vi.mock('../../../octocode-tools-core/src/serverConfig.js', () => ({
  getServerConfig: vi.fn().mockReturnValue({
    version: '1.0.0',
    githubApiUrl: 'https://api.github.com',
    timeout: 30000,
    maxRetries: 3,
    loggingEnabled: true,
    enableLocal: true,
    enableClone: false,
  }),
  isLocalEnabled: vi.fn().mockReturnValue(true),
  isCloneEnabled: vi.fn().mockReturnValue(false),
}));

vi.mock('../../../octocode-tools-core/src/session.js', () => ({
  logSessionError: vi.fn(),
}));

vi.mock('../../src/tools/local_ripgrep/register.js', () => ({
  searchContentRipgrep: vi.fn().mockResolvedValue({ status: 'hasResults' }),
}));
vi.mock('../../src/tools/local_view_structure/register.js', () => ({
  viewStructure: vi.fn().mockResolvedValue({ status: 'hasResults' }),
}));
vi.mock('../../src/tools/local_find_files/register.js', () => ({
  findFiles: vi.fn().mockResolvedValue({ status: 'hasResults' }),
}));
vi.mock('../../src/tools/local_fetch_content/register.js', () => ({
  fetchContent: vi.fn().mockResolvedValue({ status: 'hasResults' }),
}));
vi.mock('../../../octocode-tools-core/src/utils/bulkOperations.js', () => ({
  executeBulkOperation: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'test' }],
  }),
}));

describe('Local Tools Registration (TDD)', () => {
  let mockServer: McpServer;
  let registeredTools: Map<string, unknown>;
  const originalStderr = process.stderr.write;

  beforeEach(() => {
    vi.clearAllMocks();
    registeredTools = new Map();
    process.stderr.write = vi.fn();

    mockServer = {
      registerTool: vi.fn(
        (name: string, options: unknown, handler: unknown) => {
          registeredTools.set(name, { options, handler });
        }
      ),
      prompt: vi.fn(),
    } as unknown as McpServer;
  });

  afterEach(() => {
    process.stderr.write = originalStderr;
  });

  it('should register all 4 local tools when ENABLE_LOCAL is true', async () => {
    const result = await registerTools(mockServer);

    expect(result.successCount).toBe(4);
    expect(result.failedTools).toHaveLength(0);
  });

  it('should register localSearchCode with correct name', async () => {
    await registerTools(mockServer);

    expect(registeredTools.has(STATIC_TOOL_NAMES.LOCAL_RIPGREP)).toBe(true);
    expect(registeredTools.has('localSearchCode')).toBe(true);
  });

  it('should register localViewStructure with correct name', async () => {
    await registerTools(mockServer);

    expect(registeredTools.has(STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE)).toBe(
      true
    );
    expect(registeredTools.has('localViewStructure')).toBe(true);
  });

  it('should register localFindFiles with correct name', async () => {
    await registerTools(mockServer);

    expect(registeredTools.has(STATIC_TOOL_NAMES.LOCAL_FIND_FILES)).toBe(true);
    expect(registeredTools.has('localFindFiles')).toBe(true);
  });

  it('should register localGetFileContent with correct name', async () => {
    await registerTools(mockServer);

    expect(registeredTools.has(STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT)).toBe(
      true
    );
    expect(registeredTools.has('localGetFileContent')).toBe(true);
  });

  it('should NOT register tools with undefined name', async () => {
    await registerTools(mockServer);

    expect(registeredTools.has('undefined')).toBe(false);
    expect(registeredTools.has(undefined as unknown as string)).toBe(false);
  });

  it('should call server.registerTool 4 times for local tools', async () => {
    await registerTools(mockServer);

    expect(mockServer.registerTool).toHaveBeenCalledTimes(4);
  });

  it('should register tools with valid inputSchema', async () => {
    await registerTools(mockServer);

    for (const [name, value] of registeredTools.entries()) {
      const { options } = value as { options: unknown; handler: unknown };
      const opts = options as { inputSchema?: unknown };
      expect(opts.inputSchema).toBeDefined();
      expect(name).not.toBe('undefined');
    }
  });
});
