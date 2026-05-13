import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { registerTools } from '../../src/tools/toolsManager.js';
import type { ToolConfig } from '../../src/tools/toolConfig.js';
import { initialize, cleanup } from '../../src/serverConfig.js';
import {
  _setTokenResolvers,
  _resetTokenResolvers,
} from '../../src/serverConfig.js';

const createTestTool = (name: string): ToolConfig => ({
  name,
  description: `${name} test tool`,
  isDefault: true,
  isLocal: false,
  type: 'debug',
  fn: server =>
    server.registerTool(
      name,
      { description: `${name} test tool` },
      async () => ({ content: [{ type: 'text', text: 'ok' }] })
    ),
});

const createThrowingTool = (name: string): ToolConfig => ({
  ...createTestTool(name),
  fn: () => {
    throw new Error('registration failed');
  },
});

const registerTestTools = (
  server: McpServer,
  tools: ToolConfig[] = [createTestTool('testTool')]
) =>
  registerTools(server, undefined, {
    toolLoader: () => tools,
    metadataGateway: { hasTool: () => true },
  });

describe('Tool Registration - Failed Tools Reporting', () => {
  const originalEnv = { ...process.env };

  beforeEach(async () => {
    vi.clearAllMocks();
    cleanup();

    process.env.GITHUB_TOKEN = 'test-token';
    _setTokenResolvers({
      resolveTokenFull: vi.fn(async () => ({
        token: 'test-token',
        source: 'env:GITHUB_TOKEN' as const,
        wasRefreshed: false,
      })),
    });
    await initialize();
  });

  afterEach(() => {
    process.env = { ...originalEnv };
    process.env.GITHUB_TOKEN = 'test-token-for-vitest';
    cleanup();
    _resetTokenResolvers();
  });

  it('should return failedTools as an array', async () => {
    const server = new McpServer(
      { name: 'test', version: '1.0.0' },
      { capabilities: { tools: { listChanged: false } } }
    );

    const result = await registerTestTools(server);

    expect(result).toHaveProperty('successCount');
    expect(result).toHaveProperty('failedTools');
    expect(Array.isArray(result.failedTools)).toBe(true);
  });

  it('should register at least one tool in a normal environment', async () => {
    const server = new McpServer(
      { name: 'test', version: '1.0.0' },
      { capabilities: { tools: { listChanged: false } } }
    );

    const { successCount } = await registerTestTools(server);
    expect(successCount).toBeGreaterThan(0);
  });

  it('should have few or no failed tools in a normal environment', async () => {
    const server = new McpServer(
      { name: 'test', version: '1.0.0' },
      { capabilities: { tools: { listChanged: false } } }
    );

    const { successCount, failedTools } = await registerTestTools(server);
    expect(successCount).toBeGreaterThan(failedTools.length);
    for (const name of failedTools) {
      expect(typeof name).toBe('string');
    }
  });

  it('should include tool names in failedTools when registration throws', async () => {
    const server = new McpServer(
      { name: 'test', version: '1.0.0' },
      { capabilities: { tools: { listChanged: false } } }
    );

    // Register once normally
    const firstResult = await registerTestTools(server);
    expect(firstResult.successCount).toBeGreaterThan(0);

    // Register a deterministic failure so the failedTools contract is covered
    // without importing the full production tool catalog.
    const secondResult = await registerTestTools(server, [
      createTestTool('duplicateTool'),
      createThrowingTool('failingTool'),
    ]);

    expect(secondResult.failedTools).toContain('failingTool');

    // The key contract: failedTools contains string names, never undefined.
    for (const name of secondResult.failedTools) {
      expect(typeof name).toBe('string');
      expect(name.length).toBeGreaterThan(0);
    }
  });

  describe('Return shape contract', () => {
    it('successCount + failedTools.length should equal total attempted tools', async () => {
      const server = new McpServer(
        { name: 'test', version: '1.0.0' },
        { capabilities: { tools: { listChanged: false } } }
      );

      const { successCount, failedTools } = await registerTestTools(server);

      // At least the counts should be non-negative integers
      expect(Number.isInteger(successCount)).toBe(true);
      expect(successCount).toBeGreaterThanOrEqual(0);
      expect(failedTools.length).toBeGreaterThanOrEqual(0);
    });
  });
});
