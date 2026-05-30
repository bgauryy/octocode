import type { CLICommand, ParsedArgs } from '../types.js';
import type { MCPClient, MCPServer } from '../../types/index.js';
import { c, bold, dim } from '../../utils/colors.js';
import { MCP_REGISTRY } from '../../configs/mcp-registry.js';
import { MCP_CLIENTS, getMCPConfigPath } from '../../utils/mcp-paths.js';
import { readMCPConfig, writeMCPConfig } from '../../utils/mcp-io.js';
import {
  formatSupportedMCPClients,
  normalizeMCPClient,
  parseMCPEnv,
} from './shared.js';

const SUPPORTED_MCP_CLIENTS_TEXT = formatSupportedMCPClients();

export const mcpCommand: CLICommand = {
  name: 'mcp',
  description: 'Non-interactive MCP marketplace management',
  usage:
    'octocode-cli mcp [list|install|remove|status] [--id <mcp-id>] [--client <client>|--config <path>] [--search <text>] [--category <name>] [--env KEY=VALUE[,KEY=VALUE]] [--force]',
  options: [
    {
      name: 'id',
      description: 'MCP registry id (required for install/remove)',
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
      console.log();
      console.log(`  ${bold('MCP Marketplace (non-interactive)')}`);
      console.log(`  ${dim('Config:')} ${configPath}`);
      console.log(`  ${dim('Results:')} ${entries.length}`);
      console.log();
      if (entries.length === 0) {
        console.log(`  ${dim('No MCP entries matched your filters.')}`);
        console.log();
        return;
      }
      for (const entry of entries) {
        const status = installedMap[entry.id]
          ? c('green', 'installed')
          : dim('not installed');
        console.log(
          `  ${c('cyan', '•')} ${entry.id} ${dim('(' + entry.category + ')')} ${status}`
        );
      }
      console.log();
      return;
    }

    if (subcommand === 'status') {
      const installedIds = Object.keys(installedMap).sort((a, b) =>
        a.localeCompare(b)
      );
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
        console.log();
        console.log(
          `  ${c('red', 'X')} Missing required option: --id <mcp-id>`
        );
        console.log();
        process.exitCode = 1;
        return;
      }
      const entry = MCP_REGISTRY.find(
        item => item.id.toLowerCase() === mcpId.toLowerCase()
      );
      if (!entry) {
        console.log();
        console.log(`  ${c('red', 'X')} MCP not found in registry: ${mcpId}`);
        console.log();
        process.exitCode = 1;
        return;
      }
      const envResult = parseMCPEnv(
        typeof rawEnv === 'string' ? rawEnv : undefined
      );
      if (envResult.error) {
        console.log();
        console.log(`  ${c('red', 'X')} ${envResult.error}`);
        console.log();
        process.exitCode = 1;
        return;
      }
      if (installedMap[entry.id] && !force) {
        console.log();
        console.log(
          `  ${c('yellow', 'WARN')} MCP already installed: ${entry.id}`
        );
        console.log(
          `  ${dim('Use --force to overwrite existing configuration.')}`
        );
        console.log();
        process.exitCode = 1;
        return;
      }
      const serverConfig: MCPServer = {
        command: entry.installConfig.command,
        args: [...entry.installConfig.args],
      };
      const mergedEnv = {
        ...(entry.installConfig.env || {}),
        ...envResult.values,
      };
      if (Object.keys(mergedEnv).length > 0) {
        serverConfig.env = mergedEnv;
      }
      const nextConfig = {
        ...config,
        mcpServers: { ...installedMap, [entry.id]: serverConfig },
      };
      const result = writeMCPConfig(configPath, nextConfig);
      if (!result.success) {
        console.log();
        console.log(`  ${c('red', 'X')} Failed to write MCP config`);
        console.log(`  ${dim(result.error || 'Unknown write error')}`);
        console.log();
        process.exitCode = 1;
        return;
      }
      console.log();
      console.log(`  ${c('green', '✅')} Installed MCP: ${entry.id}`);
      console.log(`  ${dim('Client:')} ${MCP_CLIENTS[client]?.name || client}`);
      console.log(`  ${dim('Config:')} ${configPath}`);
      console.log();
      return;
    }

    if (subcommand === 'remove') {
      if (!mcpId) {
        console.log();
        console.log(
          `  ${c('red', 'X')} Missing required option: --id <mcp-id>`
        );
        console.log();
        process.exitCode = 1;
        return;
      }
      const installedKey = Object.keys(installedMap).find(
        key => key.toLowerCase() === mcpId.toLowerCase()
      );
      if (!installedKey) {
        console.log();
        console.log(`  ${c('yellow', 'WARN')} MCP not installed: ${mcpId}`);
        console.log(`  ${dim('Nothing to remove from target config.')}`);
        console.log();
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
        console.log();
        console.log(`  ${c('red', 'X')} Failed to update MCP config`);
        console.log(`  ${dim(result.error || 'Unknown write error')}`);
        console.log();
        process.exitCode = 1;
        return;
      }
      console.log();
      console.log(`  ${c('green', '✅')} Removed MCP: ${mcpId}`);
      console.log(`  ${dim('Client:')} ${MCP_CLIENTS[client]?.name || client}`);
      console.log(`  ${dim('Config:')} ${configPath}`);
      console.log();
      return;
    }

    console.log();
    console.log(`  ${c('red', 'X')} Unknown mcp subcommand: ${subcommand}`);
    console.log(
      `  ${dim('Usage:')} octocode-cli mcp [list|install|remove|status]`
    );
    console.log();
    process.exitCode = 1;
  },
};
