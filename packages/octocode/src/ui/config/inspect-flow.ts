import { bold, c, dim } from '../../utils/colors.js';
import { select, confirm, loadInquirer } from '../../utils/prompts.js';
import { separatorChoice } from '../../utils/prompt-separator.js';
import {
  getAllClientInstallStatus,
  readMCPConfig,
  writeMCPConfig,
  removeExternalMCPConfig,
  MCP_CLIENTS,
  type ClientInstallStatus,
} from '../../utils/mcp-config.js';
import type { MCPServer } from '../../types/index.js';
import { Spinner } from '../../utils/spinner.js';

type InspectMenuChoice = 'back' | string;
type ServerMenuChoice = 'remove' | 'back';

function getClientDisplayName(client: ClientInstallStatus): string {
  const name = MCP_CLIENTS[client.client]?.name || client.client;
  return client.octocodeInstalled
    ? `${name} ${dim('(Octocode installed)')}`
    : name;
}

async function inspectMCPServer(
  clientStatus: ClientInstallStatus,
  serverId: string,
  serverConfig: MCPServer
): Promise<void> {
  console.log();
  console.log(c('blue', '━'.repeat(66)));
  console.log(`  ${bold('MCP Server Details')}`);
  console.log(c('blue', '━'.repeat(66)));
  console.log();

  console.log(`  ${dim('ID:')} ${c('cyan', serverId)}`);
  console.log(
    `  ${dim('Command:')} ${c('green', serverConfig.command || 'unknown')}`
  );

  if (serverConfig.args && serverConfig.args.length > 0) {
    console.log(`  ${dim('Args:')}`);
    serverConfig.args.forEach(arg => {
      console.log(`    ${arg}`);
    });
  }

  if (serverConfig.env && Object.keys(serverConfig.env).length > 0) {
    console.log(`  ${dim('Environment:')}`);
    Object.entries(serverConfig.env).forEach(([key, value]) => {
      const isSensitive =
        key.includes('KEY') || key.includes('TOKEN') || key.includes('SECRET');
      const displayValue = isSensitive ? '********' : value;
      console.log(`    ${key}=${displayValue}`);
    });
  }
  console.log();

  const choice = await select<ServerMenuChoice>({
    message: '',
    choices: [
      {
        name: `${c('red', 'Delete')} - Remove Server`,
        value: 'remove',
        description: 'Remove this server from configuration',
      },
      separatorChoice<{ name: string; value: ServerMenuChoice }>(),
      {
        name: `${c('dim', '- Back')}`,
        value: 'back',
      },
    ],
    loop: false,
    theme: {
      prefix: '  ',
      style: {
        highlight: (text: string) => c('cyan', text),
      },
    },
  });

  if (choice === 'remove') {
    const confirmed = await confirm({
      message: `Are you sure you want to remove "${serverId}" from ${MCP_CLIENTS[clientStatus.client]?.name}?`,
      default: false,
    });

    if (confirmed) {
      const configPath = clientStatus.configPath;
      const config = readMCPConfig(configPath);

      if (config) {
        const spinner = new Spinner('Removing server...').start();
        const newConfig = removeExternalMCPConfig(config, serverId);
        const result = writeMCPConfig(configPath, newConfig);

        if (result.success) {
          spinner.succeed(`Removed ${serverId}`);
        } else {
          spinner.fail(`Failed to remove: ${result.error}`);
        }
        console.log();

        await new Promise(resolve => setTimeout(resolve, 1000));
      }
    }
  }
}

async function inspectClient(clientStatus: ClientInstallStatus): Promise<void> {
  let inMenu = true;
  while (inMenu) {
    const configPath = clientStatus.configPath;
    const config = readMCPConfig(configPath);

    if (!config || !config.mcpServers) {
      console.log();
      console.log(
        `  ${c('yellow', 'WARN')} No MCP configuration found or empty.`
      );
      console.log();
      await new Promise(resolve => setTimeout(resolve, 1500));
      return;
    }

    const serverIds = Object.keys(config.mcpServers);

    if (serverIds.length === 0) {
      console.log();
      console.log(`  ${c('yellow', 'WARN')} No MCP servers configured.`);
      console.log();
      await new Promise(resolve => setTimeout(resolve, 1500));
      return;
    }

    console.log();
    console.log(
      `  ${bold(MCP_CLIENTS[clientStatus.client]?.name || clientStatus.client)}`
    );
    console.log(`  ${dim(configPath)}`);
    console.log();

    const choices: Array<{
      name: string;
      value: InspectMenuChoice;
      description?: string;
    }> = serverIds.map(id => ({
      name: id === 'octocode' ? `${id}` : `MCP ${id}`,
      value: id,
      description: config.mcpServers![id].command,
    }));

    choices.push(separatorChoice<{ name: string; value: InspectMenuChoice }>());
    choices.push({
      name: `${c('dim', '- Back')}`,
      value: 'back',
    });

    const choice = await select<InspectMenuChoice>({
      message: 'Select an MCP server to inspect:',
      choices,
      pageSize: 10,
      loop: false,
      theme: {
        prefix: '  ',
        style: {
          highlight: (text: string) => c('cyan', text),
          message: (text: string) => bold(text),
        },
      },
    });

    if (choice === 'back') {
      inMenu = false;
    } else {
      await inspectMCPServer(clientStatus, choice, config.mcpServers[choice]);
    }
  }
}

export async function runInspectFlow(): Promise<void> {
  await loadInquirer();

  let inMenu = true;
  while (inMenu) {
    const allClients = getAllClientInstallStatus();
    const configuredClients = allClients.filter(cl => cl.configExists);

    if (configuredClients.length === 0) {
      console.log();
      console.log(`  ${c('yellow', 'WARN')} No configured MCP clients found.`);
      console.log();
      return;
    }

    console.log();
    console.log(c('blue', '━'.repeat(66)));
    console.log(`  Info: ${bold('MCP Configuration Details')}`);
    console.log(c('blue', '━'.repeat(66)));
    console.log();

    const choices: Array<{
      name: string;
      value: InspectMenuChoice;
      description?: string;
    }> = configuredClients.map(client => ({
      name: getClientDisplayName(client),
      value: client.client,
      description: client.configPath,
    }));

    choices.push(separatorChoice<{ name: string; value: InspectMenuChoice }>());
    choices.push({
      name: `${c('dim', '- Back')}`,
      value: 'back',
    });

    const choice = await select<InspectMenuChoice>({
      message: 'Select a client to inspect:',
      choices,
      pageSize: 10,
      loop: false,
      theme: {
        prefix: '  ',
        style: {
          highlight: (text: string) => c('cyan', text),
          message: (text: string) => bold(text),
        },
      },
    });

    if (choice === 'back') {
      inMenu = false;
    } else {
      const selectedClient = configuredClients.find(cl => cl.client === choice);
      if (selectedClient) {
        await inspectClient(selectedClient);
      }
    }
  }
}
