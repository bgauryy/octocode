import type { CLICommand, ParsedArgs } from '../types.js';
import { c, bold, dim } from '../../utils/colors.js';
import { formatAuthStatusAsJson, printAuthStatus } from './shared.js';
import { paths, getDirectorySizeBytes, formatBytes } from 'octocode-shared';
import {
  DETECTABLE_MCP_CLIENTS,
  getMCPConfigPath,
  configFileExists,
  MCP_CLIENTS,
} from '../../utils/mcp-paths.js';
import { readMCPConfig } from '../../utils/mcp-io.js';
import { getSkillsCacheDir } from '../../utils/skills-fetch.js';
import { readAllClientConfigs, analyzeSyncState } from '../../features/sync.js';
import path from 'node:path';

export const statusCommand: CLICommand = {
  name: 'status',
  description: 'Show full Octocode health status (auth + MCPs + cache)',
  usage: 'octocode status [--hostname <host>] [--sync] [--json]',
  options: [
    {
      name: 'hostname',
      description: 'GitHub Enterprise hostname (default: github.com)',
      hasValue: true,
    },
    {
      name: 'sync',
      description: 'Include MCP sync analysis (needsSync, conflicts)',
    },
    {
      name: 'json',
      description: 'Output as JSON: { auth, mcpClients, cache, sync? }',
    },
  ],
  handler: async (args: ParsedArgs) => {
    const hostnameOpt = args.options['hostname'];
    const hostname =
      (typeof hostnameOpt === 'string' ? hostnameOpt : undefined) ||
      'github.com';
    const jsonOutput = Boolean(args.options['json']);
    const includeSyncAnalysis = Boolean(args.options['sync']);

    const auth = formatAuthStatusAsJson(hostname);

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

    const octocodeHome =
      paths.home ||
      process.env.OCTOCODE_HOME ||
      path.join(process.env.HOME || '', '.octocode');
    const reposDir = paths.repos || path.join(octocodeHome, 'repos');
    const logsDir = paths.logs || path.join(octocodeHome, 'logs');
    const skillsDir = getSkillsCacheDir();
    const reposBytes = getDirectorySizeBytes(reposDir);
    const skillsBytes = getDirectorySizeBytes(skillsDir);
    const logsBytes = getDirectorySizeBytes(logsDir);

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
            repos: {
              path: reposDir,
              sizeBytes: reposBytes,
              sizeFormatted: formatBytes(reposBytes),
            },
            skills: {
              path: skillsDir,
              sizeBytes: skillsBytes,
              sizeFormatted: formatBytes(skillsBytes),
            },
            logs: {
              path: logsDir,
              sizeBytes: logsBytes,
              sizeFormatted: formatBytes(logsBytes),
            },
            totalBytes: reposBytes + skillsBytes + logsBytes,
            totalFormatted: formatBytes(reposBytes + skillsBytes + logsBytes),
          },
          ...(syncData ? { sync: syncData } : {}),
        })
      );
      if (!auth['authenticated']) process.exitCode = 1;
      return;
    }

    console.log();
    console.log(`  ${bold('🟢 Octocode Status')}`);
    console.log();

    printAuthStatus(hostname);

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
      `  ${bold('Cache')}  ${dim(formatBytes(reposBytes + skillsBytes + logsBytes))} total`
    );
    console.log(`    ${c('cyan', '•')} repos:  ${formatBytes(reposBytes)}`);
    console.log(`    ${c('cyan', '•')} skills: ${formatBytes(skillsBytes)}`);
    console.log(`    ${c('cyan', '•')} logs:   ${formatBytes(logsBytes)}`);

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
        `  ${dim('Run')} ${c('cyan', 'octocode status --sync')} ${dim('to include sync analysis.')}`
      );
    }
    console.log();
  },
};
