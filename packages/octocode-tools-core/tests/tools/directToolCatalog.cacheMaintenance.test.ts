import { beforeEach, describe, expect, it, vi } from 'vitest';

const { runCacheMaintenanceIfDue } = vi.hoisted(() => ({
  runCacheMaintenanceIfDue: vi.fn().mockResolvedValue(false),
}));

vi.mock('../../src/cacheMaintenance.js', () => ({
  runCacheMaintenanceIfDue,
}));

import {
  _overrideInitialize,
  _resetInitialize,
  executeDirectTool,
} from '../../src/tools/directToolCatalog.exec.js';
import {
  AST_SEARCH_TOOL_NAME,
  LSP_SEARCH_TOOL_NAME,
} from '@octocodeai/octocode-core/schema';

describe('direct CLI cache bootstrap', () => {
  beforeEach(() => {
    _resetInitialize();
    vi.clearAllMocks();
  });

  it('checks maintenance before a local tool that does not initialize the server runtime', async () => {
    await executeDirectTool(AST_SEARCH_TOOL_NAME, {
      queries: [
        { operation: 'tree', path: process.cwd(), maxDepth: 1, pageSize: 1 },
      ],
    });

    expect(runCacheMaintenanceIfDue).toHaveBeenCalledOnce();
    expect(runCacheMaintenanceIfDue).toHaveBeenCalledWith(expect.any(String));
  });

  it('runs lspSearch without initializing provider/server runtime', async () => {
    let initializeCalls = 0;
    _overrideInitialize(async () => {
      initializeCalls += 1;
    });

    await executeDirectTool(LSP_SEARCH_TOOL_NAME, {
      queries: [
        {
          uri: '/definitely/missing/octocode-lsp-maintenance-test.ts',
          operation: 'diagnostic',
        },
      ],
    });

    expect(initializeCalls).toBe(0);
    expect(runCacheMaintenanceIfDue).toHaveBeenCalledOnce();
  });
});
