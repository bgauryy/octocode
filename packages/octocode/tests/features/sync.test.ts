import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  readAllClientConfigs,
  analyzeSyncState,
  areMCPServersEqual,
  buildMergedConfig,
  prepareSyncPayload,
  isSyncNeeded,
  getClientDisplayName,
  getCanonicalConfig,
  executeSyncToClients,
  quickSync,
  type SyncAnalysis,
  type MCPDiff,
  type ConflictResolution,
  type ClientConfigSnapshot,
} from '../../src/features/sync.js';
import type { MCPServer, MCPConfig } from '../../src/types/index.js';

vi.mock('../../src/utils/mcp-paths.js', () => ({
  detectAvailableClients: vi.fn(),
  getMCPConfigPath: vi.fn(),
  configFileExists: vi.fn(),
  MCP_CLIENTS: {
    cursor: { id: 'cursor', name: 'Cursor', category: 'ide' },
    'claude-desktop': {
      id: 'claude-desktop',
      name: 'Claude Desktop',
      category: 'desktop',
    },
    'claude-code': { id: 'claude-code', name: 'Claude Code', category: 'cli' },
  },
}));

vi.mock('../../src/utils/mcp-io.js', () => ({
  readMCPConfig: vi.fn(),
  writeMCPConfig: vi.fn(),
}));

import {
  detectAvailableClients,
  getMCPConfigPath,
  configFileExists,
} from '../../src/utils/mcp-paths.js';
import { readMCPConfig, writeMCPConfig } from '../../src/utils/mcp-io.js';

describe('areMCPServersEqual', () => {
  it('should return true for identical configs', () => {
    const a: MCPServer = {
      command: 'npx',
      args: ['@octocodeai/mcp@latest'],
    };
    const b: MCPServer = {
      command: 'npx',
      args: ['@octocodeai/mcp@latest'],
    };
    expect(areMCPServersEqual(a, b)).toBe(true);
  });

  it('should return false for different commands', () => {
    const a: MCPServer = {
      command: 'npx',
      args: ['@octocodeai/mcp@latest'],
    };
    const b: MCPServer = {
      command: 'node',
      args: ['@octocodeai/mcp@latest'],
    };
    expect(areMCPServersEqual(a, b)).toBe(false);
  });

  it('should return false for different args', () => {
    const a: MCPServer = {
      command: 'npx',
      args: ['@octocodeai/mcp@latest'],
    };
    const b: MCPServer = {
      command: 'npx',
      args: ['@octocodeai/mcp@1.0.0'],
    };
    expect(areMCPServersEqual(a, b)).toBe(false);
  });

  it('should return false for different args length', () => {
    const a: MCPServer = {
      command: 'npx',
      args: ['@octocodeai/mcp@latest'],
    };
    const b: MCPServer = {
      command: 'npx',
      args: ['@octocodeai/mcp@latest', '--flag'],
    };
    expect(areMCPServersEqual(a, b)).toBe(false);
  });

  it('should handle env vars correctly', () => {
    const a: MCPServer = {
      command: 'npx',
      args: ['test'],
      env: { KEY: 'value' },
    };
    const b: MCPServer = {
      command: 'npx',
      args: ['test'],
      env: { KEY: 'value' },
    };
    expect(areMCPServersEqual(a, b)).toBe(true);
  });

  it('should return false for different env vars', () => {
    const a: MCPServer = {
      command: 'npx',
      args: ['test'],
      env: { KEY: 'value1' },
    };
    const b: MCPServer = {
      command: 'npx',
      args: ['test'],
      env: { KEY: 'value2' },
    };
    expect(areMCPServersEqual(a, b)).toBe(false);
  });

  it('should return false when env keys match but a value differs (multi-key)', () => {
    const a: MCPServer = {
      command: 'npx',
      args: ['test'],
      env: { ALPHA: 'same', BETA: 'one' },
    };
    const b: MCPServer = {
      command: 'npx',
      args: ['test'],
      env: { ALPHA: 'same', BETA: 'two' },
    };
    expect(areMCPServersEqual(a, b)).toBe(false);
  });

  it('should handle missing env vars', () => {
    const a: MCPServer = {
      command: 'npx',
      args: ['test'],
      env: { KEY: 'value' },
    };
    const b: MCPServer = {
      command: 'npx',
      args: ['test'],
    };
    expect(areMCPServersEqual(a, b)).toBe(false);
  });

  it('should handle empty env vars', () => {
    const a: MCPServer = {
      command: 'npx',
      args: ['test'],
      env: {},
    };
    const b: MCPServer = {
      command: 'npx',
      args: ['test'],
    };
    expect(areMCPServersEqual(a, b)).toBe(true);
  });
});

describe('buildMergedConfig', () => {
  it('should add MCPs to empty config', () => {
    const mcpsToSync = [
      { mcpId: 'octocode', server: { command: 'npx', args: ['test'] } },
    ];
    const result = buildMergedConfig(null, mcpsToSync);
    expect(result.mcpServers).toEqual({
      octocode: { command: 'npx', args: ['test'] },
    });
  });

  it('should merge MCPs into existing config', () => {
    const currentConfig: MCPConfig = {
      mcpServers: {
        existing: { command: 'node', args: ['existing.js'] },
      },
    };
    const mcpsToSync = [
      { mcpId: 'octocode', server: { command: 'npx', args: ['test'] } },
    ];
    const result = buildMergedConfig(currentConfig, mcpsToSync);
    expect(result.mcpServers).toEqual({
      existing: { command: 'node', args: ['existing.js'] },
      octocode: { command: 'npx', args: ['test'] },
    });
  });

  it('should override existing MCP with same id', () => {
    const currentConfig: MCPConfig = {
      mcpServers: {
        octocode: { command: 'node', args: ['old.js'] },
      },
    };
    const mcpsToSync = [
      { mcpId: 'octocode', server: { command: 'npx', args: ['new'] } },
    ];
    const result = buildMergedConfig(currentConfig, mcpsToSync);
    expect(result.mcpServers).toEqual({
      octocode: { command: 'npx', args: ['new'] },
    });
  });
});

describe('analyzeSyncState', () => {
  it('should identify fully consistent MCPs', () => {
    const snapshots: ClientConfigSnapshot[] = [
      {
        client: 'cursor',
        configPath: '/path/cursor.json',
        config: {
          mcpServers: {
            octocode: { command: 'npx', args: ['@octocodeai/mcp@latest'] },
          },
        },
        exists: true,
        mcpCount: 1,
      },
      {
        client: 'claude-desktop',
        configPath: '/path/claude.json',
        config: {
          mcpServers: {
            octocode: { command: 'npx', args: ['@octocodeai/mcp@latest'] },
          },
        },
        exists: true,
        mcpCount: 1,
      },
    ];

    const analysis = analyzeSyncState(snapshots);
    expect(analysis.summary.consistentMCPs).toBe(1);
    expect(analysis.summary.needsSyncCount).toBe(0);
    expect(analysis.summary.conflictCount).toBe(0);
  });

  it('should identify MCPs needing sync', () => {
    const snapshots: ClientConfigSnapshot[] = [
      {
        client: 'cursor',
        configPath: '/path/cursor.json',
        config: {
          mcpServers: {
            octocode: { command: 'npx', args: ['@octocodeai/mcp@latest'] },
          },
        },
        exists: true,
        mcpCount: 1,
      },
      {
        client: 'claude-desktop',
        configPath: '/path/claude.json',
        config: {
          mcpServers: {},
        },
        exists: true,
        mcpCount: 0,
      },
    ];

    const analysis = analyzeSyncState(snapshots);
    expect(analysis.summary.consistentMCPs).toBe(0);
    expect(analysis.summary.needsSyncCount).toBe(1);
    expect(analysis.summary.conflictCount).toBe(0);
    expect(analysis.needsSync[0].mcpId).toBe('octocode');
    expect(analysis.needsSync[0].missingIn).toContain('claude-desktop');
  });

  it('should identify conflicts', () => {
    const snapshots: ClientConfigSnapshot[] = [
      {
        client: 'cursor',
        configPath: '/path/cursor.json',
        config: {
          mcpServers: {
            octocode: { command: 'npx', args: ['@octocodeai/mcp@latest'] },
          },
        },
        exists: true,
        mcpCount: 1,
      },
      {
        client: 'claude-desktop',
        configPath: '/path/claude.json',
        config: {
          mcpServers: {
            octocode: { command: 'npx', args: ['@octocodeai/mcp@1.0.0'] },
          },
        },
        exists: true,
        mcpCount: 1,
      },
    ];

    const analysis = analyzeSyncState(snapshots);
    expect(analysis.summary.consistentMCPs).toBe(0);
    expect(analysis.summary.needsSyncCount).toBe(0);
    expect(analysis.summary.conflictCount).toBe(1);
    expect(analysis.conflicts[0].mcpId).toBe('octocode');
    expect(analysis.conflicts[0].hasConflict).toBe(true);
  });

  it('should skip clients whose config has no mcpServers', () => {
    const snapshots: ClientConfigSnapshot[] = [
      {
        client: 'cursor',
        configPath: '/path/cursor.json',
        config: {
          mcpServers: {
            octocode: { command: 'npx', args: ['@octocodeai/mcp@latest'] },
          },
        },
        exists: true,
        mcpCount: 1,
      },
      {
        client: 'claude-desktop',
        configPath: '/path/claude.json',
        config: {} as MCPConfig,
        exists: true,
        mcpCount: 0,
      },
    ];

    const analysis = analyzeSyncState(snapshots);
    expect(analysis.summary.needsSyncCount).toBe(1);
    expect(analysis.needsSync[0].missingIn).toContain('claude-desktop');
  });

  it('should use empty variants map when outer mcp map misses mcpId (defensive)', () => {
    const originalGet = Map.prototype.get;
    let octocodeOuterGets = 0;
    const spy = vi.spyOn(Map.prototype, 'get').mockImplementation(function (
      this: Map<unknown, unknown>,
      key: unknown
    ) {
      const val = originalGet.call(this, key);
      const values = [...this.values()];
      const looksLikeOuterMcpToClients =
        values.length > 0 && values.every(v => v instanceof Map);
      if (looksLikeOuterMcpToClients && key === 'octocode') {
        octocodeOuterGets++;
        if (octocodeOuterGets === 2) {
          return undefined;
        }
      }
      return val;
    });

    const snapshots: ClientConfigSnapshot[] = [
      {
        client: 'cursor',
        configPath: '/path/cursor.json',
        config: {
          mcpServers: {
            octocode: { command: 'npx', args: ['@octocodeai/mcp@latest'] },
          },
        },
        exists: true,
        mcpCount: 1,
      },
    ];

    const analysis = analyzeSyncState(snapshots);
    const diff = analysis.diffs.find(d => d.mcpId === 'octocode');
    expect(diff).toBeDefined();
    expect(diff!.presentIn).toEqual([]);
    expect(diff!.missingIn).toContain('cursor');

    spy.mockRestore();
  });
});

describe('prepareSyncPayload', () => {
  it('should prepare payload for MCPs needing sync', () => {
    const analysis: SyncAnalysis = {
      clients: [],
      allMCPs: new Set(['octocode']),
      diffs: [],
      fullyConsistent: [],
      needsSync: [
        {
          mcpId: 'octocode',
          presentIn: ['cursor'],
          missingIn: ['claude-desktop'],
          hasConflict: false,
          variants: new Map([
            ['cursor', { command: 'npx', args: ['@octocodeai/mcp@latest'] }],
          ]),
        },
      ],
      conflicts: [],
      summary: {
        totalClients: 2,
        clientsWithConfig: 2,
        totalUniqueMCPs: 1,
        consistentMCPs: 0,
        needsSyncCount: 1,
        conflictCount: 0,
      },
    };

    const payload = prepareSyncPayload(analysis, []);
    expect(payload).toHaveLength(1);
    expect(payload[0].mcpId).toBe('octocode');
    expect(payload[0].server.command).toBe('npx');
  });

  it('should include resolved conflicts in payload', () => {
    const analysis: SyncAnalysis = {
      clients: [],
      allMCPs: new Set(['octocode']),
      diffs: [],
      fullyConsistent: [],
      needsSync: [],
      conflicts: [
        {
          mcpId: 'octocode',
          presentIn: ['cursor', 'claude-desktop'],
          missingIn: [],
          hasConflict: true,
          variants: new Map([
            ['cursor', { command: 'npx', args: ['@octocodeai/mcp@latest'] }],
            [
              'claude-desktop',
              { command: 'npx', args: ['@octocodeai/mcp@1.0.0'] },
            ],
          ]),
        },
      ],
      summary: {
        totalClients: 2,
        clientsWithConfig: 2,
        totalUniqueMCPs: 1,
        consistentMCPs: 0,
        needsSyncCount: 0,
        conflictCount: 1,
      },
    };

    const resolutions: ConflictResolution[] = [
      {
        mcpId: 'octocode',
        chosenConfig: { command: 'npx', args: ['@octocodeai/mcp@latest'] },
        sourceClient: 'cursor',
      },
    ];

    const payload = prepareSyncPayload(analysis, resolutions);
    expect(payload).toHaveLength(1);
    expect(payload[0].mcpId).toBe('octocode');
    expect(payload[0].server.args).toContain('@octocodeai/mcp@latest');
  });

  it('should combine needsSync entries and resolved conflicts in one payload', () => {
    const needServer: MCPServer = { command: 'npx', args: ['need-mcp'] };
    const analysis: SyncAnalysis = {
      clients: [],
      allMCPs: new Set(['need-id', 'conflict-id']),
      diffs: [],
      fullyConsistent: [],
      needsSync: [
        {
          mcpId: 'need-id',
          presentIn: ['cursor'],
          missingIn: ['claude-desktop'],
          hasConflict: false,
          variants: new Map([['cursor', needServer]]),
        },
      ],
      conflicts: [
        {
          mcpId: 'conflict-id',
          presentIn: ['cursor', 'claude-desktop'],
          missingIn: [],
          hasConflict: true,
          variants: new Map([
            ['cursor', { command: 'npx', args: ['v1'] }],
            ['claude-desktop', { command: 'npx', args: ['v2'] }],
          ]),
        },
      ],
      summary: {
        totalClients: 2,
        clientsWithConfig: 2,
        totalUniqueMCPs: 2,
        consistentMCPs: 0,
        needsSyncCount: 1,
        conflictCount: 1,
      },
    };

    const resolutions: ConflictResolution[] = [
      {
        mcpId: 'conflict-id',
        chosenConfig: { command: 'npx', args: ['resolved'] },
        sourceClient: 'cursor',
      },
    ];

    const payload = prepareSyncPayload(analysis, resolutions);
    expect(payload).toHaveLength(2);
    expect(payload.find(p => p.mcpId === 'need-id')).toEqual({
      mcpId: 'need-id',
      server: needServer,
    });
    expect(payload.find(p => p.mcpId === 'conflict-id')?.server.args).toEqual([
      'resolved',
    ]);
  });
});

describe('isSyncNeeded', () => {
  it('should return true when sync is needed', () => {
    const analysis: SyncAnalysis = {
      clients: [],
      allMCPs: new Set(),
      diffs: [],
      fullyConsistent: [],
      needsSync: [{ mcpId: 'test' } as MCPDiff],
      conflicts: [],
      summary: {
        totalClients: 2,
        clientsWithConfig: 2,
        totalUniqueMCPs: 1,
        consistentMCPs: 0,
        needsSyncCount: 1,
        conflictCount: 0,
      },
    };
    expect(isSyncNeeded(analysis)).toBe(true);
  });

  it('should return true when there are conflicts', () => {
    const analysis: SyncAnalysis = {
      clients: [],
      allMCPs: new Set(),
      diffs: [],
      fullyConsistent: [],
      needsSync: [],
      conflicts: [{ mcpId: 'test' } as MCPDiff],
      summary: {
        totalClients: 2,
        clientsWithConfig: 2,
        totalUniqueMCPs: 1,
        consistentMCPs: 0,
        needsSyncCount: 0,
        conflictCount: 1,
      },
    };
    expect(isSyncNeeded(analysis)).toBe(true);
  });

  it('should return false when everything is synced', () => {
    const analysis: SyncAnalysis = {
      clients: [],
      allMCPs: new Set(),
      diffs: [],
      fullyConsistent: [{ mcpId: 'test' } as MCPDiff],
      needsSync: [],
      conflicts: [],
      summary: {
        totalClients: 2,
        clientsWithConfig: 2,
        totalUniqueMCPs: 1,
        consistentMCPs: 1,
        needsSyncCount: 0,
        conflictCount: 0,
      },
    };
    expect(isSyncNeeded(analysis)).toBe(false);
  });
});

describe('getClientDisplayName', () => {
  it('should return the display name for known clients', () => {
    expect(getClientDisplayName('cursor')).toBe('Cursor');
    expect(getClientDisplayName('claude-desktop')).toBe('Claude Desktop');
  });

  it('should return the client id for unknown clients', () => {
    expect(getClientDisplayName('unknown' as any)).toBe('unknown');
  });
});

describe('readAllClientConfigs', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should read configs from all available clients', () => {
    vi.mocked(detectAvailableClients).mockReturnValue([
      'cursor',
      'claude-desktop',
    ]);
    vi.mocked(getMCPConfigPath).mockImplementation(
      client => `/path/${client}.json`
    );
    vi.mocked(configFileExists).mockReturnValue(true);
    vi.mocked(readMCPConfig).mockImplementation(_path => ({
      mcpServers: {
        octocode: { command: 'npx', args: ['test'] },
      },
    }));

    const snapshots = readAllClientConfigs();
    expect(snapshots).toHaveLength(2);
    expect(snapshots[0].client).toBe('cursor');
    expect(snapshots[1].client).toBe('claude-desktop');
  });

  it('should handle non-existent configs', () => {
    vi.mocked(detectAvailableClients).mockReturnValue(['cursor']);
    vi.mocked(getMCPConfigPath).mockReturnValue('/path/cursor.json');
    vi.mocked(configFileExists).mockReturnValue(false);

    const snapshots = readAllClientConfigs();
    expect(snapshots).toHaveLength(1);
    expect(snapshots[0].exists).toBe(false);
    expect(snapshots[0].mcpCount).toBe(0);
  });
});

describe('getCanonicalConfig', () => {
  it('should return resolution config when provided', () => {
    const diff: MCPDiff = {
      mcpId: 'octocode',
      presentIn: ['cursor', 'claude-desktop'],
      missingIn: [],
      hasConflict: true,
      variants: new Map([
        ['cursor', { command: 'npx', args: ['@octocodeai/mcp@latest'] }],
        ['claude-desktop', { command: 'npx', args: ['@octocodeai/mcp@1.0.0'] }],
      ]),
    };

    const resolution: ConflictResolution = {
      mcpId: 'octocode',
      chosenConfig: { command: 'npx', args: ['@octocodeai/mcp@2.0.0'] },
      sourceClient: 'cursor',
    };

    const result = getCanonicalConfig(diff, resolution);
    expect(result).toEqual({ command: 'npx', args: ['@octocodeai/mcp@2.0.0'] });
  });

  it('should return first variant when no conflict and variants exist', () => {
    const diff: MCPDiff = {
      mcpId: 'octocode',
      presentIn: ['cursor'],
      missingIn: ['claude-desktop'],
      hasConflict: false,
      variants: new Map([
        ['cursor', { command: 'npx', args: ['@octocodeai/mcp@latest'] }],
      ]),
    };

    const result = getCanonicalConfig(diff);
    expect(result).toEqual({
      command: 'npx',
      args: ['@octocodeai/mcp@latest'],
    });
  });

  it('should return null when there is a conflict without resolution', () => {
    const diff: MCPDiff = {
      mcpId: 'octocode',
      presentIn: ['cursor', 'claude-desktop'],
      missingIn: [],
      hasConflict: true,
      variants: new Map([
        ['cursor', { command: 'npx', args: ['@octocodeai/mcp@latest'] }],
        ['claude-desktop', { command: 'npx', args: ['@octocodeai/mcp@1.0.0'] }],
      ]),
    };

    const result = getCanonicalConfig(diff);
    expect(result).toBeNull();
  });

  it('should return null when no variants exist', () => {
    const diff: MCPDiff = {
      mcpId: 'octocode',
      presentIn: [],
      missingIn: ['cursor', 'claude-desktop'],
      hasConflict: false,
      variants: new Map(),
    };

    const result = getCanonicalConfig(diff);
    expect(result).toBeNull();
  });
});

describe('executeSyncToClients', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should sync MCPs to all existing clients successfully', () => {
    vi.mocked(writeMCPConfig).mockReturnValue({
      success: true,
      backupPath: '/backup/path.json',
    });

    const snapshots: ClientConfigSnapshot[] = [
      {
        client: 'cursor',
        configPath: '/path/cursor.json',
        config: { mcpServers: {} },
        exists: true,
        mcpCount: 0,
      },
      {
        client: 'claude-desktop',
        configPath: '/path/claude.json',
        config: { mcpServers: {} },
        exists: true,
        mcpCount: 0,
      },
    ];

    const mcpsToSync = [
      { mcpId: 'octocode', server: { command: 'npx', args: ['test'] } },
    ];

    const result = executeSyncToClients(snapshots, mcpsToSync);

    expect(result.success).toBe(true);
    expect(result.mcpsSynced).toContain('octocode');
    expect(result.errors).toHaveLength(0);
    expect(result.clientResults.get('cursor')?.success).toBe(true);
    expect(result.clientResults.get('claude-desktop')?.success).toBe(true);
  });

  it('should handle write failures and collect errors', () => {
    vi.mocked(writeMCPConfig).mockImplementation(path => {
      if (path.includes('cursor')) {
        return { success: true };
      }
      return { success: false, error: 'Permission denied' };
    });

    const snapshots: ClientConfigSnapshot[] = [
      {
        client: 'cursor',
        configPath: '/path/cursor.json',
        config: { mcpServers: {} },
        exists: true,
        mcpCount: 0,
      },
      {
        client: 'claude-desktop',
        configPath: '/path/claude.json',
        config: { mcpServers: {} },
        exists: true,
        mcpCount: 0,
      },
    ];

    const mcpsToSync = [
      { mcpId: 'octocode', server: { command: 'npx', args: ['test'] } },
    ];

    const result = executeSyncToClients(snapshots, mcpsToSync);

    expect(result.success).toBe(false);
    expect(result.errors).toHaveLength(1);
    expect(result.errors[0]).toContain('Permission denied');
    expect(result.clientResults.get('cursor')?.success).toBe(true);
    expect(result.clientResults.get('claude-desktop')?.success).toBe(false);
  });

  it('should only sync to specified target clients', () => {
    vi.mocked(writeMCPConfig).mockReturnValue({ success: true });

    const snapshots: ClientConfigSnapshot[] = [
      {
        client: 'cursor',
        configPath: '/path/cursor.json',
        config: { mcpServers: {} },
        exists: true,
        mcpCount: 0,
      },
      {
        client: 'claude-desktop',
        configPath: '/path/claude.json',
        config: { mcpServers: {} },
        exists: true,
        mcpCount: 0,
      },
    ];

    const mcpsToSync = [
      { mcpId: 'octocode', server: { command: 'npx', args: ['test'] } },
    ];

    const result = executeSyncToClients(snapshots, mcpsToSync, ['cursor']);

    expect(result.success).toBe(true);
    expect(result.clientResults.size).toBe(1);
    expect(result.clientResults.has('cursor')).toBe(true);
    expect(result.clientResults.has('claude-desktop')).toBe(false);
  });

  it('should skip non-existing configs when no target clients specified', () => {
    vi.mocked(writeMCPConfig).mockReturnValue({ success: true });

    const snapshots: ClientConfigSnapshot[] = [
      {
        client: 'cursor',
        configPath: '/path/cursor.json',
        config: { mcpServers: {} },
        exists: true,
        mcpCount: 0,
      },
      {
        client: 'claude-desktop',
        configPath: '/path/claude.json',
        config: null,
        exists: false,
        mcpCount: 0,
      },
    ];

    const mcpsToSync = [
      { mcpId: 'octocode', server: { command: 'npx', args: ['test'] } },
    ];

    const result = executeSyncToClients(snapshots, mcpsToSync);

    expect(result.success).toBe(true);
    expect(result.clientResults.size).toBe(1);
    expect(result.clientResults.has('cursor')).toBe(true);
  });

  it('should sync multiple MCPs and deduplicate mcpsSynced', () => {
    vi.mocked(writeMCPConfig).mockReturnValue({ success: true });

    const snapshots: ClientConfigSnapshot[] = [
      {
        client: 'cursor',
        configPath: '/path/cursor.json',
        config: { mcpServers: {} },
        exists: true,
        mcpCount: 0,
      },
    ];

    const mcpsToSync = [
      { mcpId: 'octocode', server: { command: 'npx', args: ['octocode'] } },
      { mcpId: 'github', server: { command: 'npx', args: ['github'] } },
    ];

    const result = executeSyncToClients(snapshots, mcpsToSync);

    expect(result.success).toBe(true);
    expect(result.mcpsSynced).toHaveLength(2);
    expect(result.mcpsSynced).toContain('octocode');
    expect(result.mcpsSynced).toContain('github');
  });

  it('should handle unknown write error gracefully', () => {
    vi.mocked(writeMCPConfig).mockReturnValue({ success: false });

    const snapshots: ClientConfigSnapshot[] = [
      {
        client: 'cursor',
        configPath: '/path/cursor.json',
        config: { mcpServers: {} },
        exists: true,
        mcpCount: 0,
      },
    ];

    const mcpsToSync = [
      { mcpId: 'octocode', server: { command: 'npx', args: ['test'] } },
    ];

    const result = executeSyncToClients(snapshots, mcpsToSync);

    expect(result.success).toBe(false);
    expect(result.errors[0]).toContain('Unknown write error');
  });
});

describe('quickSync', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(getMCPConfigPath).mockImplementation(
      client => `/path/${client}.json`
    );
    vi.mocked(configFileExists).mockReturnValue(true);
    vi.mocked(writeMCPConfig).mockReturnValue({ success: true });
  });

  it('returns a no-sync failure when fewer than two clients have configs', async () => {
    vi.mocked(detectAvailableClients).mockReturnValue(['cursor']);
    vi.mocked(readMCPConfig).mockReturnValue({ mcpServers: {} });

    const result = await quickSync({});

    expect(result.success).toBe(false);
    expect(result.syncPerformed).toBe(false);
    expect(result.message).toContain('Not enough clients');
  });

  it('does not write when clients are already synced', async () => {
    vi.mocked(detectAvailableClients).mockReturnValue([
      'cursor',
      'claude-desktop',
    ]);
    vi.mocked(readMCPConfig).mockReturnValue({
      mcpServers: {
        octocode: { command: 'npx', args: ['@octocodeai/mcp@latest'] },
      },
    });

    const result = await quickSync({});

    expect(result.success).toBe(true);
    expect(result.syncPerformed).toBe(false);
    expect(result.message).toContain('already in sync');
    expect(writeMCPConfig).not.toHaveBeenCalled();
  });

  it('reports conflicts without writing unless force is enabled', async () => {
    vi.mocked(detectAvailableClients).mockReturnValue([
      'cursor',
      'claude-desktop',
    ]);
    vi.mocked(readMCPConfig).mockImplementation(path => ({
      mcpServers: {
        octocode: {
          command: 'npx',
          args: [path.includes('cursor') ? 'new' : 'old'],
        },
      },
    }));

    const result = await quickSync({});

    expect(result.success).toBe(false);
    expect(result.syncPerformed).toBe(false);
    expect(result.message).toContain('conflict');
    expect(writeMCPConfig).not.toHaveBeenCalled();
  });

  it('force mode resolves conflicts and writes to existing clients', async () => {
    vi.mocked(detectAvailableClients).mockReturnValue([
      'cursor',
      'claude-desktop',
    ]);
    vi.mocked(readMCPConfig).mockImplementation(path => ({
      mcpServers: {
        octocode: {
          command: 'npx',
          args: [path.includes('cursor') ? 'new' : 'old'],
        },
      },
    }));

    const result = await quickSync({ force: true });

    expect(result.success).toBe(true);
    expect(result.syncPerformed).toBe(true);
    expect(result.message).toContain('Synced 1 MCP');
    expect(writeMCPConfig).toHaveBeenCalledTimes(2);
  });

  it('dry-run reports the payload without writing', async () => {
    vi.mocked(detectAvailableClients).mockReturnValue([
      'cursor',
      'claude-desktop',
    ]);
    vi.mocked(readMCPConfig).mockImplementation((path): MCPConfig => {
      if (path.includes('cursor')) {
        return {
          mcpServers: {
            octocode: { command: 'npx', args: ['@octocodeai/mcp@latest'] },
          },
        };
      }

      return { mcpServers: {} };
    });

    const result = await quickSync({ dryRun: true });

    expect(result.success).toBe(true);
    expect(result.syncPerformed).toBe(false);
    expect(result.message).toContain('Would sync 1 MCP');
    expect(writeMCPConfig).not.toHaveBeenCalled();
  });
});
