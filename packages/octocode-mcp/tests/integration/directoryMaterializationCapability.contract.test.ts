import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { _resetRuntimeSurface, setRuntimeSurface } from '@octocodeai/config';
import { cleanup as cleanupConfig } from '@octocodeai/octocode-tools-core';
import { ALL_TOOLS } from '../../src/tools/toolConfig.js';
import { createMockMcpServer } from '../fixtures/mcp-fixtures.js';

describe('MCP directory materialization capability policy', () => {
  const originalEnableLocal = process.env.ENABLE_LOCAL;
  const originalEnableClone = process.env.ENABLE_CLONE;

  beforeEach(() => {
    setRuntimeSurface('mcp');
    process.env.ENABLE_LOCAL = 'true';
    process.env.ENABLE_CLONE = 'true';
    cleanupConfig();
  });

  afterEach(() => {
    if (originalEnableLocal === undefined) delete process.env.ENABLE_LOCAL;
    else process.env.ENABLE_LOCAL = originalEnableLocal;
    if (originalEnableClone === undefined) delete process.env.ENABLE_CLONE;
    else process.env.ENABLE_CLONE = originalEnableClone;
    _resetRuntimeSurface();
    cleanupConfig();
  });

  it.each([
    ['ENABLE_LOCAL', 'false', 'localToolsDisabled'],
    ['ENABLE_CLONE', 'false', 'cloneDisabled'],
  ] as const)(
    'returns a structured capability error when %s=%s',
    async (flag, value, errorCode) => {
      process.env[flag] = value;
      cleanupConfig();
      const mcp = createMockMcpServer();
      const tool = ALL_TOOLS.find(item => item.name === 'ghGetFileContent');
      expect(tool).toBeDefined();
      tool!.fn(mcp.server);

      const result = await mcp.callTool('ghGetFileContent', {
        queries: [
          {
            owner: 'octocat',
            repo: 'Hello-World',
            path: 'src',
            type: 'directory',
          },
        ],
      });

      expect(JSON.stringify(result.structuredContent)).toContain(errorCode);
    }
  );
});
