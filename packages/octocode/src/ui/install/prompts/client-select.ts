import type { MCPClient } from '../../../types/index.js';
import { c, dim, bold } from '../../../utils/colors.js';
import { select } from '../../../utils/prompts.js';
import { separatorChoice } from '../../../utils/prompt-separator.js';
import { MCP_CLIENTS, detectCurrentClient } from '../../../utils/mcp-paths.js';
import type { ClientChoice, ClientWithStatus } from './types.js';
import {
  getClientStatusIndicator,
  getAllClientsWithStatus,
} from './client-status.js';
import { promptCustomPath } from './custom-path.js';

export async function selectMCPClient(): Promise<{
  client: MCPClient;
  customPath?: string;
} | null> {
  const currentClient = detectCurrentClient();
  const allClients = getAllClientsWithStatus();

  const installedClients = allClients.filter(c => c.status.octocodeInstalled);
  const availableClients = allClients.filter(
    c => c.isAvailable && !c.status.octocodeInstalled
  );

  if (installedClients.length === 0) {
    return await promptNoConfigurationsFound(availableClients, currentClient);
  }

  return await promptExistingConfigurations(
    installedClients,
    availableClients,
    currentClient
  );
}

async function promptNoConfigurationsFound(
  availableClients: ClientWithStatus[],
  currentClient: MCPClient | null
): Promise<{ client: MCPClient; customPath?: string } | null> {
  console.log();
  console.log(c('yellow', '  ┌' + '─'.repeat(60) + '┐'));
  console.log(
    c('yellow', '  │ ') +
      `${c('yellow', 'INFO')} No octocode configurations found` +
      ' '.repeat(24) +
      c('yellow', '│')
  );
  console.log(c('yellow', '  └' + '─'.repeat(60) + '┘'));
  console.log();
  console.log(`  ${dim('Octocode is not configured in any MCP client yet.')}`);
  console.log();

  if (availableClients.length === 0) {
    console.log(
      `  ${c('red', 'X')} ${dim('No MCP clients detected on this system.')}`
    );
    console.log();
    console.log(`  ${dim('Supported clients:')}`);
    console.log(`    ${dim('• Cursor, Claude Desktop, Claude Code')}`);
    console.log(`    ${dim('• Windsurf, Zed, VS Code (Cline/Roo/Continue)')}`);
    console.log();
    console.log(`  ${dim('Install a supported client and try again,')}`);
    console.log(`  ${dim('or use "Custom Path" to specify a config file.')}`);
    console.log();

    const choices: ClientChoice[] = [
      {
        name: `${c('cyan', '-')} Custom Path - ${dim('Specify your own MCP config path')}`,
        value: 'custom' as MCPClient,
      },
      separatorChoice<ClientChoice>(),
      {
        name: `${c('dim', '- Back')}`,
        value: 'back',
      },
    ];

    const selected = await select<MCPClient | 'back'>({
      message: 'What would you like to do?',
      choices: choices as Array<{ name: string; value: MCPClient | 'back' }>,
      loop: false,
    });

    if (selected === 'back') return null;

    if (selected === 'custom') {
      const customPath = await promptCustomPath();
      if (!customPath) return null;
      return { client: 'custom', customPath };
    }

    return null;
  }

  const choices: ClientChoice[] = [];

  for (const { clientId, status } of availableClients) {
    const client = MCP_CLIENTS[clientId];
    let name = `${client.name} - ${dim(client.description)}`;

    name += ` ${getClientStatusIndicator(status)}`;

    if (currentClient === clientId) {
      name = `${c('green', '★')} ${name} ${c('yellow', '(Current)')}`;
    }

    choices.push({
      name,
      value: clientId,
    });
  }

  choices.sort((a, b) => {
    if (currentClient === a.value) return -1;
    if (currentClient === b.value) return 1;
    return 0;
  });

  choices.push(separatorChoice<ClientChoice>());
  choices.push({
    name: `${c('cyan', '-')} Custom Path - ${dim('Specify your own MCP config path')}`,
    value: 'custom' as MCPClient,
  });
  choices.push(separatorChoice<ClientChoice>());
  choices.push({
    name: `${c('dim', '- Back')}`,
    value: 'back',
  });

  const selected = await select<MCPClient | 'back'>({
    message: 'Select a client to install Octocode:',
    choices: choices as Array<{ name: string; value: MCPClient | 'back' }>,
    loop: false,
  });

  if (selected === 'back') return null;

  if (selected === 'custom') {
    const customPath = await promptCustomPath();
    if (!customPath) return null;
    return { client: 'custom', customPath };
  }

  return { client: selected };
}

async function promptExistingConfigurations(
  installedClients: ClientWithStatus[],
  availableClients: ClientWithStatus[],
  currentClient: MCPClient | null
): Promise<{ client: MCPClient; customPath?: string } | null> {
  console.log();
  console.log(
    `  ${c('green', '✅')} Found ${bold(String(installedClients.length))} octocode configuration${installedClients.length > 1 ? 's' : ''}`
  );
  console.log();

  const choices: ClientChoice[] = [];

  for (const { clientId } of installedClients) {
    const client = MCP_CLIENTS[clientId];
    let name = `${c('green', '✅')} ${client.name} - ${dim('View/Edit configuration')}`;

    if (currentClient === clientId) {
      name += ` ${c('yellow', '(Current)')}`;
    }

    choices.push({
      name,
      value: clientId,
    });
  }

  choices.sort((a, b) => {
    if (currentClient === a.value) return -1;
    if (currentClient === b.value) return 1;
    return 0;
  });

  if (availableClients.length > 0) {
    choices.push(separatorChoice<ClientChoice>());
    choices.push({
      name: `${c('blue', '+')} Install to another client - ${dim(`${availableClients.length} available`)}`,
      value: 'install-new',
    });
  }

  choices.push(separatorChoice<ClientChoice>());
  choices.push({
    name: `${c('cyan', '-')} Custom Path - ${dim('Specify your own MCP config path')}`,
    value: 'custom' as MCPClient,
  });
  choices.push(separatorChoice<ClientChoice>());
  choices.push({
    name: `${c('dim', '- Back')}`,
    value: 'back',
  });

  const selected = await select<MCPClient | 'back' | 'install-new'>({
    message: 'Select configuration to manage:',
    choices: choices as Array<{
      name: string;
      value: MCPClient | 'back' | 'install-new';
    }>,
    loop: false,
  });

  if (selected === 'back') return null;

  if (selected === 'install-new') {
    return await promptInstallToNewClient(availableClients, currentClient);
  }

  if (selected === 'custom') {
    const customPath = await promptCustomPath();
    if (!customPath) return null;
    return { client: 'custom', customPath };
  }

  return { client: selected };
}

async function promptInstallToNewClient(
  availableClients: ClientWithStatus[],
  currentClient: MCPClient | null
): Promise<{ client: MCPClient; customPath?: string } | null> {
  console.log();
  console.log(`  ${c('blue', 'INFO')} Select a client for new installation:`);
  console.log();

  const choices: ClientChoice[] = [];

  for (const { clientId, status } of availableClients) {
    const client = MCP_CLIENTS[clientId];
    let name = `${client.name} - ${dim(client.description)}`;

    name += ` ${getClientStatusIndicator(status)}`;

    if (currentClient === clientId) {
      name = `${c('green', '★')} ${name} ${c('yellow', '(Current)')}`;
    }

    choices.push({
      name,
      value: clientId,
    });
  }

  choices.sort((a, b) => {
    if (currentClient === a.value) return -1;
    if (currentClient === b.value) return 1;
    return 0;
  });

  choices.push(separatorChoice<ClientChoice>());
  choices.push({
    name: `${c('dim', '- Back to configurations')}`,
    value: 'back',
  });

  const selected = await select<MCPClient | 'back'>({
    message: 'Select client to install Octocode:',
    choices: choices as Array<{ name: string; value: MCPClient | 'back' }>,
    loop: false,
  });

  if (selected === 'back') {
    const allClients = getAllClientsWithStatus();
    const installedClients = allClients.filter(c => c.status.octocodeInstalled);
    return await promptExistingConfigurations(
      installedClients,
      availableClients,
      currentClient
    );
  }

  return { client: selected };
}
