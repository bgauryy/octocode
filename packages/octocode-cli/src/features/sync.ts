import type { MCPClient, MCPConfig, MCPServer } from '../types/index.js';
import {
  getMCPConfigPath,
  detectAvailableClients,
  configFileExists,
  MCP_CLIENTS,
} from '../utils/mcp-paths.js';
import { readMCPConfig, writeMCPConfig } from '../utils/mcp-io.js';

export interface ClientConfigSnapshot {
  client: MCPClient;
  configPath: string;
  config: MCPConfig | null;
  exists: boolean;
  mcpCount: number;
}

export interface MCPDiff {
  mcpId: string;
  presentIn: MCPClient[];
  missingIn: MCPClient[];
  hasConflict: boolean;
  variants: Map<MCPClient, MCPServer>;
}

export interface SyncAnalysis {
  clients: ClientConfigSnapshot[];
  allMCPs: Set<string>;
  diffs: MCPDiff[];
  fullyConsistent: MCPDiff[];
  needsSync: MCPDiff[];
  conflicts: MCPDiff[];
  summary: {
    totalClients: number;
    clientsWithConfig: number;
    totalUniqueMCPs: number;
    consistentMCPs: number;
    needsSyncCount: number;
    conflictCount: number;
  };
}

export interface ConflictResolution {
  mcpId: string;
  chosenConfig: MCPServer;
  sourceClient: MCPClient;
}

export interface SyncResult {
  success: boolean;
  clientResults: Map<
    MCPClient,
    { success: boolean; error?: string; backupPath?: string }
  >;
  mcpsSynced: string[];
  errors: string[];
}

export interface QuickSyncResult {
  success: boolean;
  message: string;
  syncPerformed: boolean;
}

export function readAllClientConfigs(): ClientConfigSnapshot[] {
  const availableClients = detectAvailableClients();
  const snapshots: ClientConfigSnapshot[] = [];

  for (const client of availableClients) {
    const configPath = getMCPConfigPath(client);
    const exists = configFileExists(client);
    let config: MCPConfig | null = null;
    let mcpCount = 0;

    if (exists) {
      config = readMCPConfig(configPath);
      if (config?.mcpServers) {
        mcpCount = Object.keys(config.mcpServers).length;
      }
    }

    snapshots.push({
      client,
      configPath,
      config,
      exists,
      mcpCount,
    });
  }

  return snapshots;
}

export function areMCPServersEqual(a: MCPServer, b: MCPServer): boolean {
  if (a.command !== b.command) return false;

  const aArgs = a.args || [];
  const bArgs = b.args || [];
  if (aArgs.length !== bArgs.length) return false;
  for (let i = 0; i < aArgs.length; i++) {
    if (aArgs[i] !== bArgs[i]) return false;
  }

  const aEnvKeys = Object.keys(a.env || {}).sort();
  const bEnvKeys = Object.keys(b.env || {}).sort();

  if (aEnvKeys.length !== bEnvKeys.length) return false;

  for (let i = 0; i < aEnvKeys.length; i++) {
    if (aEnvKeys[i] !== bEnvKeys[i]) return false;
    if ((a.env || {})[aEnvKeys[i]] !== (b.env || {})[bEnvKeys[i]]) return false;
  }

  return true;
}

export function analyzeSyncState(
  snapshots: ClientConfigSnapshot[]
): SyncAnalysis {
  const clientsWithConfig = snapshots.filter(s => s.exists && s.config);
  const allMCPs = new Set<string>();
  const mcpToClients = new Map<string, Map<MCPClient, MCPServer>>();

  for (const snapshot of clientsWithConfig) {
    if (!snapshot.config?.mcpServers) continue;

    for (const [mcpId, server] of Object.entries(snapshot.config.mcpServers)) {
      allMCPs.add(mcpId);

      if (!mcpToClients.has(mcpId)) {
        mcpToClients.set(mcpId, new Map());
      }
      mcpToClients.get(mcpId)!.set(snapshot.client, server);
    }
  }

  const diffs: MCPDiff[] = [];
  const allClientIds = clientsWithConfig.map(s => s.client);

  for (const mcpId of allMCPs) {
    const variants = mcpToClients.get(mcpId) || new Map();
    const presentIn = Array.from(variants.keys());
    const missingIn = allClientIds.filter(c => !variants.has(c));

    let hasConflict = false;
    const configs = Array.from(variants.values());
    if (configs.length > 1) {
      const firstConfig = configs[0];
      for (let i = 1; i < configs.length; i++) {
        if (!areMCPServersEqual(firstConfig, configs[i])) {
          hasConflict = true;
          break;
        }
      }
    }

    diffs.push({
      mcpId,
      presentIn,
      missingIn,
      hasConflict,
      variants,
    });
  }

  const fullyConsistent = diffs.filter(
    d => d.missingIn.length === 0 && !d.hasConflict
  );
  const needsSync = diffs.filter(d => d.missingIn.length > 0 && !d.hasConflict);
  const conflicts = diffs.filter(d => d.hasConflict);

  return {
    clients: snapshots,
    allMCPs,
    diffs,
    fullyConsistent,
    needsSync,
    conflicts,
    summary: {
      totalClients: snapshots.length,
      clientsWithConfig: clientsWithConfig.length,
      totalUniqueMCPs: allMCPs.size,
      consistentMCPs: fullyConsistent.length,
      needsSyncCount: needsSync.length,
      conflictCount: conflicts.length,
    },
  };
}

export function buildMergedConfig(
  currentConfig: MCPConfig | null,
  mcpsToSync: Array<{ mcpId: string; server: MCPServer }>
): MCPConfig {
  const merged: MCPConfig = {
    mcpServers: { ...(currentConfig?.mcpServers || {}) },
  };

  for (const { mcpId, server } of mcpsToSync) {
    merged.mcpServers![mcpId] = server;
  }

  return merged;
}

export function getCanonicalConfig(
  diff: MCPDiff,
  resolution?: ConflictResolution
): MCPServer | null {
  if (resolution) {
    return resolution.chosenConfig;
  }

  if (!diff.hasConflict && diff.variants.size > 0) {
    return Array.from(diff.variants.values())[0];
  }

  return null;
}

export function executeSyncToClients(
  snapshots: ClientConfigSnapshot[],
  mcpsToSync: Array<{ mcpId: string; server: MCPServer }>,
  targetClients?: MCPClient[]
): SyncResult {
  const results = new Map<
    MCPClient,
    { success: boolean; error?: string; backupPath?: string }
  >();
  const errors: string[] = [];
  const mcpsSynced: string[] = [];

  const clients = targetClients
    ? snapshots.filter(s => targetClients.includes(s.client))
    : snapshots.filter(s => s.exists);

  for (const snapshot of clients) {
    if (snapshot.exists && snapshot.config === null) {
      results.set(snapshot.client, {
        success: false,
        error:
          'Config file exists but could not be parsed (corrupt JSON?) — skipping to avoid data loss',
      });
      errors.push(
        `${MCP_CLIENTS[snapshot.client]?.name || snapshot.client}: corrupt config — skipped`
      );
      continue;
    }
    const mergedConfig = buildMergedConfig(snapshot.config, mcpsToSync);

    const writeResult = writeMCPConfig(snapshot.configPath, mergedConfig);

    if (writeResult.success) {
      results.set(snapshot.client, {
        success: true,
        backupPath: writeResult.backupPath,
      });
    } else {
      const error = writeResult.error || 'Unknown write error';
      results.set(snapshot.client, { success: false, error });
      errors.push(
        `${MCP_CLIENTS[snapshot.client]?.name || snapshot.client}: ${error}`
      );
    }
  }

  for (const { mcpId } of mcpsToSync) {
    if (!mcpsSynced.includes(mcpId)) {
      mcpsSynced.push(mcpId);
    }
  }

  const allSuccess = Array.from(results.values()).every(r => r.success);

  return {
    success: allSuccess,
    clientResults: results,
    mcpsSynced,
    errors,
  };
}

export function prepareSyncPayload(
  analysis: SyncAnalysis,
  resolutions: ConflictResolution[]
): Array<{ mcpId: string; server: MCPServer }> {
  const payload: Array<{ mcpId: string; server: MCPServer }> = [];
  const resolutionMap = new Map(resolutions.map(r => [r.mcpId, r]));

  for (const diff of analysis.needsSync) {
    const server = getCanonicalConfig(diff);
    if (server) {
      payload.push({ mcpId: diff.mcpId, server });
    }
  }

  for (const diff of analysis.conflicts) {
    const resolution = resolutionMap.get(diff.mcpId);
    if (resolution) {
      payload.push({ mcpId: diff.mcpId, server: resolution.chosenConfig });
    }
  }

  return payload;
}

export function isSyncNeeded(analysis: SyncAnalysis): boolean {
  return (
    analysis.summary.needsSyncCount > 0 || analysis.summary.conflictCount > 0
  );
}

export function getClientDisplayName(client: MCPClient): string {
  return MCP_CLIENTS[client]?.name || client;
}

export async function quickSync(options: {
  force?: boolean;
  dryRun?: boolean;
}): Promise<QuickSyncResult> {
  const snapshots = readAllClientConfigs();
  const analysis = analyzeSyncState(snapshots);

  if (analysis.summary.clientsWithConfig < 2) {
    return {
      success: false,
      message: `Not enough clients to sync (found ${analysis.summary.clientsWithConfig})`,
      syncPerformed: false,
    };
  }

  if (!isSyncNeeded(analysis)) {
    return {
      success: true,
      message: 'All MCPs are already in sync',
      syncPerformed: false,
    };
  }

  if (analysis.conflicts.length > 0 && !options.force) {
    return {
      success: false,
      message: `${analysis.conflicts.length} conflict(s) found. Use --force to auto-resolve or run interactive mode.`,
      syncPerformed: false,
    };
  }

  const resolutions: ConflictResolution[] = [];
  if (options.force) {
    for (const diff of analysis.conflicts) {
      const firstVariant = Array.from(diff.variants.entries())[0];
      if (firstVariant) {
        resolutions.push({
          mcpId: diff.mcpId,
          chosenConfig: firstVariant[1],
          sourceClient: firstVariant[0],
        });
      }
    }
  }

  const payload = prepareSyncPayload(analysis, resolutions);

  if (options.dryRun) {
    return {
      success: true,
      message: `Would sync ${payload.length} MCP(s) to ${analysis.summary.clientsWithConfig} client(s)`,
      syncPerformed: false,
    };
  }

  const targetClients = analysis.clients.filter(s => s.exists);
  const result = executeSyncToClients(
    analysis.clients,
    payload,
    targetClients.map(c => c.client)
  );

  if (result.success) {
    return {
      success: true,
      message: `Synced ${result.mcpsSynced.length} MCP(s) to ${targetClients.length} client(s)`,
      syncPerformed: true,
    };
  }

  return {
    success: false,
    message: `Sync failed: ${result.errors.join(', ')}`,
    syncPerformed: true,
  };
}
