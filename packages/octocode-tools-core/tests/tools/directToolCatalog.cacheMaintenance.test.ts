import { beforeEach, describe, expect, it, vi } from 'vitest';

const { runCacheMaintenanceIfDue } = vi.hoisted(() => ({
  runCacheMaintenanceIfDue: vi.fn().mockResolvedValue(false),
}));

vi.mock('../../src/cacheMaintenance.js', () => ({
  runCacheMaintenanceIfDue,
}));

import {
  _resetInitialize,
  executeDirectTool,
} from '../../src/tools/directToolCatalog.exec.js';
import { LOCAL_SEARCH_TOOL_NAME } from '../../src/tools/toolNames.js';

describe('direct CLI cache bootstrap', () => {
  beforeEach(() => {
    _resetInitialize();
    vi.clearAllMocks();
  });

  it('checks maintenance before a local tool that does not initialize the server runtime', async () => {
    await executeDirectTool(LOCAL_SEARCH_TOOL_NAME, {
      queries: [
        { operation: 'tree', path: process.cwd(), maxDepth: 1, limit: 1 },
      ],
    });

    expect(runCacheMaintenanceIfDue).toHaveBeenCalledOnce();
    expect(runCacheMaintenanceIfDue).toHaveBeenCalledWith(expect.any(String));
  });
});
