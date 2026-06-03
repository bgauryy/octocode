import type { CLICommand, ParsedArgs } from '../types.js';
import type { MCPClient, MCPServer } from '../../types/index.js';
import { c, bold, dim } from '../../utils/colors.js';
import { MCP_REGISTRY } from '../../configs/mcp-registry.js';
import {
  MCP_CLIENTS,
  getMCPConfigPath,
  configFileExists,
  DETECTABLE_MCP_CLIENTS,
} from '../../utils/mcp-paths.js';
import { readMCPConfig, writeMCPConfig } from '../../utils/mcp-io.js';
import {
  formatSupportedMCPClients,
  normalizeMCPClient,
  parseMCPEnv,
} from './shared.js';
import https from 'node:https';
import http from 'node:http';

/** Fire-and-forget HEAD request to check if a URL is reachable. */
function checkUrlReachable(url: string, timeoutMs = 5000): Promise<boolean> {
  return new Promise(resolve => {
    try {
      const parsed = new URL(url);
      const lib = parsed.protocol === 'https:' ? https : http;
      const req = lib.request(
        {
          method: 'HEAD',
          hostname: parsed.hostname,
          path: parsed.pathname || '/',
          timeout: timeoutMs,
        },
        () => resolve(true)
      );
      req.on('error', () => resolve(false));
      req.on('timeout', () => {
        req.destroy();
        resolve(false);
      });
      req.end();
    } catch {
      resolve(false);
    }
  });
}

/** Return the best URL to preflight for a registry entry, or null if no check needed. */
function getPreflightUrl(entry: {
  installationType?: string;
  npmPackage?: string;
  pythonPackage?: string;
  repository: string;
  installConfig: { command: string };
}): string | null {
  const cmd = entry.installConfig.command;
  // Only preflight internet-fetching commands
  if (cmd !== 'npx' && cmd !== 'uvx' && cmd !== 'pip' && cmd !== 'pipx') {
    return null;
  }
  // npm package → check npm registry
  if (entry.npmPackage) {
    const pkg = entry.npmPackage.replace(/^@/, '').replace('/', '%2F');
    return `https://registry.npmjs.org/${pkg}`;
  }
  // python package → check PyPI
  if ((entry as { pythonPackage?: string }).pythonPackage) {
    return `https://pypi.org/pypi/${(entry as { pythonPackage?: string }).pythonPackage}/json`;
  }
  // fallback: GitHub repo
  return entry.repository;
}

/** Check if a required env var is currently set (non-empty). */
function envVarStatus(name: string): 'set' | 'missing' {
  const val = process.env[name];
  return val && val.trim().length > 0 ? 'set' : 'missing';
}

const SUPPORTED_MCP_CLIENTS_TEXT = formatSupportedMCPClients();

export const mcpCommand: CLICommand = {
  name: 'mcp',
  description: 'Non-interactive MCP marketplace management',
  usage:
    'octocode mcp [list|install|remove|status] [--id <mcp-id>] [--client <client>|--config <path>] [--search <text>] [--category <name>] [--env KEY=VALUE[,KEY=VALUE]] [--force]',
  options: [
    {
      name: 'id',
      description:
        'MCP registry id — comma-separated for batch install: --id id1,id2,id3',
      hasValue: true,
    },
    {
      name: 'client',
      short: 'c',
      description: `Target client: ${SUPPORTED_MCP_CLIENTS_TEXT}`,
      hasValue: true,
    },
    {
      name: 'config',
      description: 'Custom MCP config path (uses custom client)',
      hasValue: true,
    },
    {
      name: 'search',
      description: 'Filter list by id/name/description/tags',
      hasValue: true,
    },
    {
      name: 'category',
      description: 'Filter list by category',
      hasValue: true,
    },
    {
      name: 'env',
      description: 'Comma-separated env values: KEY=VALUE,KEY2=VALUE2',
      hasValue: true,
    },
    {
      name: 'installed',
      description: 'List only MCPs installed in target config',
    },
    {
      name: 'force',
      short: 'f',
      description: 'Overwrite existing MCP entry on install',
    },
    {
      name: 'json',
      description: 'Output results as JSON',
    },
  ],
  handler: async (args: ParsedArgs) => {
    const subcommand = (args.args[0] || 'list').toLowerCase();
    const rawId = args.options['id'];
    const mcpId =
      typeof rawId === 'string' && rawId.trim().length > 0
        ? rawId.trim()
        : undefined;
    const rawClient = args.options['client'] ?? args.options['c'];
    const rawConfig = args.options['config'];
    const rawSearch = args.options['search'];
    const rawCategory = args.options['category'];
    const rawEnv = args.options['env'];
    const installedOnly = Boolean(args.options['installed']);
    const force = Boolean(args.options['force'] || args.options['f']);
    const jsonOutput = Boolean(args.options['json']);

    let client: MCPClient = 'claude-code';
    let customPath: string | undefined;

    if (typeof rawConfig === 'string' && rawConfig.trim().length > 0) {
      client = 'custom';
      customPath = rawConfig.trim();
    } else if (typeof rawClient === 'string' && rawClient.trim().length > 0) {
      const normalizedClient = normalizeMCPClient(rawClient);
      if (!normalizedClient) {
        console.log();
        console.log(
          `  ${c('red', 'X')} Invalid --client value: ${c('yellow', rawClient)}`
        );
        console.log(
          `  ${dim('Allowed values:')} ${SUPPORTED_MCP_CLIENTS_TEXT}`
        );
        console.log();
        process.exitCode = 1;
        return;
      }
      client = normalizedClient;
    }

    const configPath = getMCPConfigPath(client, customPath);
    const config = readMCPConfig(configPath) || { mcpServers: {} };
    const installedMap = config.mcpServers || {};

    if (subcommand === 'list') {
      // When no client/config AND no registry filters: scan all OS config files
      const scanAll =
        !rawClient &&
        !rawConfig &&
        typeof rawSearch !== 'string' &&
        typeof rawCategory !== 'string' &&
        !installedOnly;

      if (scanAll) {
        const configs = DETECTABLE_MCP_CLIENTS.map(clientId => {
          const cfgPath = getMCPConfigPath(clientId);
          const exists = configFileExists(clientId);
          const servers = exists
            ? Object.keys(readMCPConfig(cfgPath)?.mcpServers || {})
            : null;
          return {
            client: clientId,
            name: MCP_CLIENTS[clientId]?.name || clientId,
            configPath: cfgPath,
            exists,
            servers,
          };
        });

        if (jsonOutput) {
          console.log(JSON.stringify({ configs }));
          return;
        }

        const found = configs.filter(cfg => cfg.exists);
        console.log();
        console.log(
          `  ${bold('MCP Configs on OS')}  ${dim(`(${found.length}/${configs.length} found)`)}`
        );

        if (found.length === 0) {
          console.log();
          console.log(`  ${dim('No MCP config files found on OS.')}`);
          console.log();
          return;
        }

        for (const cfg of found) {
          const count = cfg.servers?.length ?? 0;
          console.log();
          console.log(`  ${c('cyan', bold(cfg.name))}  ${dim(cfg.configPath)}`);
          if (!cfg.servers || count === 0) {
            console.log(`    ${dim('(no servers configured)')}`);
          } else {
            for (const srv of cfg.servers) {
              console.log(`    ${c('green', '•')} ${srv}`);
            }
          }
        }
        console.log();
        return;
      }

      // --client specified: show registry filtered by that client's installed MCPs
      let entries = MCP_REGISTRY;
      if (typeof rawSearch === 'string' && rawSearch.trim().length > 0) {
        const query = rawSearch.trim().toLowerCase();
        entries = entries.filter(
          entry =>
            entry.id.toLowerCase().includes(query) ||
            entry.name.toLowerCase().includes(query) ||
            entry.description.toLowerCase().includes(query) ||
            entry.tags?.some(tag => tag.toLowerCase().includes(query))
        );
      }
      if (typeof rawCategory === 'string' && rawCategory.trim().length > 0) {
        const category = rawCategory.trim().toLowerCase();
        entries = entries.filter(entry => entry.category === category);
      }
      if (installedOnly) {
        const installedIds = new Set(Object.keys(installedMap));
        entries = entries.filter(entry => installedIds.has(entry.id));
      }

      if (jsonOutput) {
        console.log(
          JSON.stringify({
            client,
            configPath,
            results: entries.map(e => ({
              id: e.id,
              name: e.name,
              category: e.category,
              installed: Boolean(installedMap[e.id]),
              requiredEnvVars: installedOnly
                ? (e.requiredEnvVars ?? []).map(v => ({
                    name: v.name,
                    description: v.description,
                    status: envVarStatus(v.name),
                  }))
                : undefined,
            })),
          })
        );
        return;
      }

      console.log();
      console.log(`  ${bold('MCP Marketplace')}`);
      console.log(`  ${dim('Client:')} ${MCP_CLIENTS[client]?.name || client}`);
      console.log(`  ${dim('Config:')} ${configPath}`);
      console.log(`  ${dim('Results:')} ${entries.length}`);
      console.log();
      if (entries.length === 0) {
        console.log(`  ${dim('No MCP entries matched your filters.')}`);
        console.log();
        return;
      }
      for (const entry of entries) {
        const installed = Boolean(installedMap[entry.id]);
        const status = installed
          ? c('green', 'installed')
          : dim('not installed');
        console.log(
          `  ${c('cyan', '•')} ${entry.id} ${dim('(' + entry.category + ')')} ${status}`
        );
        if (installed && installedOnly && entry.requiredEnvVars?.length) {
          for (const ev of entry.requiredEnvVars) {
            const evStatus = envVarStatus(ev.name);
            const evIcon = evStatus === 'set' ? c('green', '✓') : c('red', '✗');
            console.log(
              `    ${evIcon} ${ev.name}  ${dim(evStatus === 'set' ? '(set)' : '(missing)')}`
            );
          }
        }
      }
      console.log();
      return;
    }

    if (subcommand === 'status') {
      const installedIds = Object.keys(installedMap).sort((a, b) =>
        a.localeCompare(b)
      );

      if (jsonOutput) {
        console.log(
          JSON.stringify({
            client,
            configPath,
            servers: installedIds,
          })
        );
        return;
      }

      console.log();
      console.log(`  ${bold('MCP Config Status')}`);
      console.log(`  ${dim('Client:')} ${MCP_CLIENTS[client]?.name || client}`);
      console.log(`  ${dim('Config:')} ${configPath}`);
      console.log(`  ${dim('Installed MCPs:')} ${installedIds.length}`);
      console.log();
      for (const id of installedIds) {
        console.log(`  ${c('cyan', '•')} ${id}`);
      }
      if (installedIds.length === 0) {
        console.log(`  ${dim('No MCP servers configured yet.')}`);
      }
      console.log();
      return;
    }

    if (subcommand === 'install') {
      if (!mcpId) {
        if (jsonOutput) {
          console.log(
            JSON.stringify({
              success: false,
              error: 'Missing required option: --id <mcp-id>',
            })
          );
        } else {
          console.log();
          console.log(
            `  ${c('red', 'X')} Missing required option: --id <mcp-id>`
          );
          console.log();
        }
        process.exitCode = 1;
        return;
      }

      // Batch: --id accepts comma-separated ids
      const mcpIds = mcpId
        .split(',')
        .map(s => s.trim())
        .filter(Boolean);

      const envResult = parseMCPEnv(
        typeof rawEnv === 'string' ? rawEnv : undefined
      );
      if (envResult.error) {
        if (jsonOutput) {
          console.log(
            JSON.stringify({ success: false, error: envResult.error })
          );
        } else {
          console.log();
          console.log(`  ${c('red', 'X')} ${envResult.error}`);
          console.log();
        }
        process.exitCode = 1;
        return;
      }

      const batchResults: Array<{
        id: string;
        success: boolean;
        preflightUrl?: string;
        error?: string;
        hint?: string;
        preflightFailed?: boolean;
      }> = [];

      // Resolve all entries; fail-fast on unknown ids
      const resolved = mcpIds.map(id => ({
        id,
        entry: MCP_REGISTRY.find(
          item => item.id.toLowerCase() === id.toLowerCase()
        ),
      }));
      for (const u of resolved.filter(r => !r.entry)) {
        batchResults.push({
          id: u.id,
          success: false,
          error: `MCP not found in registry: ${u.id}`,
        });
      }
      const known = resolved.filter(
        (
          r
        ): r is {
          id: string;
          entry: NonNullable<(typeof r)['entry']>;
        } => Boolean(r.entry)
      );

      // Already-installed guard
      const toInstall = known.filter(r => {
        if (installedMap[r.entry.id] && !force) {
          batchResults.push({
            id: r.entry.id,
            success: false,
            error: `MCP already installed: ${r.entry.id}`,
            hint: 'Use --force to overwrite',
          });
          return false;
        }
        return true;
      });

      // Parallel preflight — only for internet-fetching commands
      const preflightMap = new Map<
        string,
        { ok: boolean; url: string | null }
      >();
      await Promise.allSettled(
        toInstall.map(async r => {
          const url = getPreflightUrl(r.entry);
          const ok = url ? await checkUrlReachable(url) : true;
          preflightMap.set(r.entry.id, { ok, url });
        })
      );

      // Write phase
      for (const r of toInstall) {
        const pf = preflightMap.get(r.entry.id);
        if (pf && !pf.ok && !force) {
          batchResults.push({
            id: r.entry.id,
            success: false,
            preflightUrl: pf.url ?? undefined,
            error: `Pre-flight check failed: ${pf.url} is unreachable`,
            hint: 'Use --force to skip pre-flight and install anyway',
            preflightFailed: true,
          });
          continue;
        }

        const serverConfig: MCPServer = {
          command: r.entry.installConfig.command,
          args: [...r.entry.installConfig.args],
        };
        const mergedEnv = {
          ...(r.entry.installConfig.env || {}),
          ...envResult.values,
        };
        if (Object.keys(mergedEnv).length > 0) {
          serverConfig.env = mergedEnv;
        }

        // Read fresh config each time so batch writes accumulate
        const currentConfig = readMCPConfig(configPath) || { mcpServers: {} };
        const nextConfig = {
          ...currentConfig,
          mcpServers: {
            ...(currentConfig.mcpServers || {}),
            [r.entry.id]: serverConfig,
          },
        };
        const writeResult = writeMCPConfig(configPath, nextConfig);
        batchResults.push(
          writeResult.success
            ? { id: r.entry.id, success: true }
            : {
                id: r.entry.id,
                success: false,
                error: writeResult.error || 'Failed to write MCP config',
              }
        );
      }

      const failed = batchResults.filter(r => !r.success);

      if (jsonOutput) {
        if (mcpIds.length === 1) {
          const r = batchResults[0];
          console.log(
            JSON.stringify({
              success: r.success,
              id: r.id,
              client,
              configPath,
              error: r.error ?? null,
              hint: r.hint ?? null,
            })
          );
        } else {
          console.log(
            JSON.stringify({ results: batchResults, client, configPath })
          );
        }
        if (failed.length > 0) process.exitCode = 1;
        return;
      }

      console.log();
      for (const r of batchResults) {
        if (r.success) {
          console.log(`  ${c('green', '✅')} Installed: ${r.id}`);
        } else {
          console.log(`  ${c('red', 'X')} ${r.error}`);
          if (r.hint) console.log(`     ${dim(r.hint)}`);
        }
      }
      if (batchResults.length > 1) {
        console.log();
        console.log(
          `  ${dim('Client:')} ${MCP_CLIENTS[client]?.name || client}  ${dim('Config:')} ${configPath}`
        );
      } else if (batchResults[0]?.success) {
        console.log(
          `  ${dim('Client:')} ${MCP_CLIENTS[client]?.name || client}`
        );
        console.log(`  ${dim('Config:')} ${configPath}`);
      }
      console.log();
      if (failed.length > 0) process.exitCode = 1;
      return;
    }

    if (subcommand === 'remove') {
      if (!mcpId) {
        if (jsonOutput) {
          console.log(
            JSON.stringify({
              success: false,
              error: 'Missing required option: --id <mcp-id>',
            })
          );
        } else {
          console.log();
          console.log(
            `  ${c('red', 'X')} Missing required option: --id <mcp-id>`
          );
          console.log();
        }
        process.exitCode = 1;
        return;
      }
      const installedKey = Object.keys(installedMap).find(
        key => key.toLowerCase() === mcpId.toLowerCase()
      );
      if (!installedKey) {
        if (jsonOutput) {
          console.log(
            JSON.stringify({
              success: false,
              error: `MCP not installed: ${mcpId}`,
            })
          );
        } else {
          console.log();
          console.log(`  ${c('yellow', 'WARN')} MCP not installed: ${mcpId}`);
          console.log(`  ${dim('Nothing to remove from target config.')}`);
          console.log();
        }
        process.exitCode = 1;
        return;
      }
      const nextServers = { ...installedMap };
      delete nextServers[installedKey];
      const result = writeMCPConfig(configPath, {
        ...config,
        mcpServers: nextServers,
      });
      if (!result.success) {
        if (jsonOutput) {
          console.log(
            JSON.stringify({
              success: false,
              error: result.error || 'Failed to update MCP config',
            })
          );
        } else {
          console.log();
          console.log(`  ${c('red', 'X')} Failed to update MCP config`);
          console.log(`  ${dim(result.error || 'Unknown write error')}`);
          console.log();
        }
        process.exitCode = 1;
        return;
      }
      if (jsonOutput) {
        console.log(
          JSON.stringify({ success: true, id: mcpId, client, configPath })
        );
      } else {
        console.log();
        console.log(`  ${c('green', '✅')} Removed MCP: ${mcpId}`);
        console.log(
          `  ${dim('Client:')} ${MCP_CLIENTS[client]?.name || client}`
        );
        console.log(`  ${dim('Config:')} ${configPath}`);
        console.log();
      }
      return;
    }

    console.log();
    console.log(`  ${c('red', 'X')} Unknown mcp subcommand: ${subcommand}`);
    console.log(`  ${dim('Usage:')} octocode mcp [list|install|remove|status]`);
    console.log();
    process.exitCode = 1;
  },
};
