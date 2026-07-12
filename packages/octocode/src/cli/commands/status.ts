import type { CLICommand, ParsedArgs } from '../types.js';
import { c, bold, dim } from '../../utils/colors.js';
import { formatAuthStatusAsJson, printAuthStatus } from './shared.js';
import { paths } from '@octocodeai/octocode-tools-core/paths';
import {
  getDirectorySizeBytes,
  formatBytes,
} from '@octocodeai/octocode-tools-core/fs-utils';
import {
  DETECTABLE_MCP_CLIENTS,
  getMCPConfigPath,
  configFileExists,
  MCP_CLIENTS,
} from '../../utils/mcp-paths.js';
import { readMCPConfig } from '../../utils/mcp-io.js';
import { readAllClientConfigs, analyzeSyncState } from '../../features/sync.js';

export const statusCommand: CLICommand = {
  name: 'status',
  options: [
    { name: 'hostname', hasValue: true },
    { name: 'sync' },
    { name: 'json' },
  ],
  handler: async (args: ParsedArgs) => {
    const hostnameOpt = args.options['hostname'];
    const hostname =
      (typeof hostnameOpt === 'string' ? hostnameOpt : undefined) ||
      'github.com';
    const jsonOutput = Boolean(args.options['json']);
    const includeSyncAnalysis = Boolean(args.options['sync']);

    // `status` reports GitHub auth + cache health and takes no positional. A
    // stray arg (commonly `status <language>`, conflated with `lsp-server
    // status`) would otherwise be silently dropped — warn and point the way.
    const stray = args.args[0];
    if (stray && !jsonOutput) {
      console.error(
        `  ${c('yellow', '!')} ${dim(`'${stray}' ignored — status reports GitHub auth + cache health (no positional args). For a language server use`)} ${c('cyan', 'lsp-server status <file>')} ${dim('or')} ${c('cyan', 'lsp-server list')}${dim('.')}`
      );
    }

    const auth = await formatAuthStatusAsJson(hostname);

    const mcpClients = DETECTABLE_MCP_CLIENTS.map(clientId => {
      const cfgPath = getMCPConfigPath(clientId);
      const exists = configFileExists(clientId);
      const servers = exists
        ? Object.keys(readMCPConfig(cfgPath)?.mcpServers || {})
        : [];
      return {
        client: clientId,
        name: MCP_CLIENTS[clientId]?.name || clientId,
        configPath: cfgPath,
        exists,
        serverCount: servers.length,
        servers,
        octocodeInstalled: servers.includes('octocode-mcp'),
      };
    });

    const octocodeHome = paths.home;
    const tmpDir = paths.tmp;
    const cloneDir = paths.clone;
    const treeDir = paths.tree;
    const cloneBytes = getDirectorySizeBytes(cloneDir);
    const treeBytes = getDirectorySizeBytes(treeDir);
    const tmpBytes = cloneBytes + treeBytes;
    const totalCacheBytes = tmpBytes;

    let syncData: {
      summary: {
        needsSyncCount: number;
        conflictCount: number;
        consistentMCPs: number;
        totalUniqueMCPs: number;
      };
      needsSync: Array<{ mcpId: string; missingIn: string[] }>;
      conflicts: Array<{ mcpId: string; presentIn: string[] }>;
    } | null = null;

    if (includeSyncAnalysis) {
      const snapshots = readAllClientConfigs();
      const analysis = analyzeSyncState(snapshots);
      syncData = {
        summary: {
          needsSyncCount: analysis.summary.needsSyncCount,
          conflictCount: analysis.summary.conflictCount,
          consistentMCPs: analysis.summary.consistentMCPs,
          totalUniqueMCPs: analysis.summary.totalUniqueMCPs,
        },
        needsSync: analysis.needsSync.map(d => ({
          mcpId: d.mcpId,
          missingIn: d.missingIn,
        })),
        conflicts: analysis.conflicts.map(d => ({
          mcpId: d.mcpId,
          presentIn: d.presentIn,
        })),
      };
    }

    if (jsonOutput) {
      console.log(
        JSON.stringify({
          auth,
          mcpClients,
          cache: {
            home: octocodeHome,
            tmp: {
              path: tmpDir,
              sizeBytes: tmpBytes,
              sizeFormatted: formatBytes(tmpBytes),
            },
            clone: {
              path: cloneDir,
              sizeBytes: cloneBytes,
              sizeFormatted: formatBytes(cloneBytes),
            },
            tree: {
              path: treeDir,
              sizeBytes: treeBytes,
              sizeFormatted: formatBytes(treeBytes),
            },
            totalBytes: totalCacheBytes,
            totalFormatted: formatBytes(totalCacheBytes),
          },
          ...(syncData ? { sync: syncData } : {}),
        })
      );
      return;
    }

    console.log();
    console.log(`  ${bold('🟢 Octocode Status')}`);
    console.log();

    await printAuthStatus(hostname);

    const found = mcpClients.filter(c => c.exists);
    console.log();
    console.log(
      `  ${bold('MCP Clients')}  ${dim(`(${found.length}/${mcpClients.length} configured)`)}`
    );
    console.log();
    for (const cl of mcpClients) {
      if (!cl.exists) continue;
      const octocodeIcon = cl.octocodeInstalled
        ? c('green', '✓')
        : c('yellow', '○');
      console.log(
        `  ${octocodeIcon} ${bold(cl.name)}  ${dim(`${cl.serverCount} MCPs  ${cl.configPath}`)}`
      );
    }
    if (found.length === 0) {
      console.log(`  ${dim('No MCP config files found.')}`);
    }

    console.log();
    console.log(
      `  ${bold('Cache')}  ${dim(formatBytes(totalCacheBytes))} total`
    );
    console.log(`    ${c('cyan', '•')} tmp:    ${formatBytes(tmpBytes)}`);
    console.log(`    ${c('cyan', '•')} clone:  ${formatBytes(cloneBytes)}`);
    console.log(`    ${c('cyan', '•')} tree:   ${formatBytes(treeBytes)}`);

    if (syncData) {
      console.log();
      console.log(
        `  ${bold('Sync')}  ${dim(`${syncData.summary.totalUniqueMCPs} unique MCPs`)}`
      );
      if (syncData.summary.consistentMCPs > 0) {
        console.log(
          `    ${c('green', '✓')} ${syncData.summary.consistentMCPs} fully synced`
        );
      }
      if (syncData.summary.needsSyncCount > 0) {
        console.log(
          `    ${c('yellow', '○')} ${syncData.summary.needsSyncCount} missing in some configs`
        );
      }
      if (syncData.summary.conflictCount > 0) {
        console.log(
          `    ${c('red', '!')} ${syncData.summary.conflictCount} conflicts across MCP configs`
        );
      }
    } else {
      console.log();
      console.log(
        `  ${dim('Run')} ${c('cyan', 'status --sync')} ${dim('to include sync analysis.')}`
      );
    }
    console.log();
  },
};
