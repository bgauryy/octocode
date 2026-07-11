import { c, bold, dim } from '../../utils/colors.js';
import { select, input } from '../../utils/prompts.js';
import { separatorChoice } from '../../utils/prompt-separator.js';
import { openInEditor } from '../../utils/platform.js';
import {
  readMCPConfig,
  getMCPConfigPath,
  isOctocodeConfigured,
  MCP_CLIENTS,
} from '../../utils/mcp-config.js';
import { selectMCPClient } from '../install/prompts.js';
import {
  ALL_CONFIG_OPTIONS,
  getExampleValue,
  getDisplayDefault,
} from './config-data.js';

export async function pressEnterToContinue(): Promise<void> {
  console.log();
  await input({
    message: dim('Press Enter to continue...'),
    default: '',
  });
}

type OpenChoice = 'cursor' | 'vscode' | 'default' | 'no';

export async function promptOpenConfigFile(configPath: string): Promise<void> {
  console.log();
  const openChoice = await select<OpenChoice>({
    message: 'Open config file?',
    choices: [
      {
        name: '- Open in Cursor',
        value: 'cursor',
        description: 'Open in Cursor IDE',
      },
      {
        name: '- Open in VS Code',
        value: 'vscode',
        description: 'Open in Visual Studio Code',
      },
      {
        name: '- Open in default app',
        value: 'default',
        description: 'Open with system default application',
      },
      separatorChoice<{ name: string; value: OpenChoice }>(),
      {
        name: `${c('dim', '- Skip')}`,
        value: 'no',
      },
    ],
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

  if (openChoice === 'no') {
    return;
  }

  const success = openInEditor(configPath, openChoice);
  if (success) {
    console.log(`  ${c('green', '✅')} Opened ${configPath}`);
  } else {
    console.log(`  ${c('yellow', 'WARN')} Could not open file automatically`);
    console.log(`  ${dim('Try opening manually:')} ${c('cyan', configPath)}`);
  }
  console.log();
}

export async function showCurrentJsonConfig(): Promise<void> {
  const selection = await selectMCPClient();
  if (!selection) return;

  const { client, customPath } = selection;
  const clientInfo = MCP_CLIENTS[client];
  const configPath = customPath || getMCPConfigPath(client);

  const config = readMCPConfig(configPath);
  if (!config) {
    console.log();
    console.log(`  ${c('red', 'X')} Failed to read config file: ${configPath}`);
    console.log();
    return;
  }

  if (!isOctocodeConfigured(config)) {
    console.log();
    console.log(
      `  ${c('yellow', 'WARN')} Octocode is not configured for ${clientInfo.name}.`
    );
    console.log(
      `  ${dim('Please install Octocode first using "Install @octocodeai/mcp".')}`
    );
    console.log();
    return;
  }

  const octocodeConfig = config.mcpServers?.octocode;

  console.log();
  console.log(
    `  ${dim('Client:')} ${clientInfo.name} ${dim('•')} ${c('cyan', configPath)}`
  );
  console.log();

  const jsonString = JSON.stringify({ octocode: octocodeConfig }, null, 2);
  const lines = jsonString.split('\n');
  for (const line of lines) {
    const highlighted = line
      .replace(/"([^"]+)":/g, `${c('cyan', '"$1"')}:`)
      .replace(/: "([^"]+)"/g, `: ${c('green', '"$1"')}`)
      .replace(/: (\d+)/g, `: ${c('yellow', '$1')}`)
      .replace(/: (true|false)/g, `: ${c('magenta', '$1')}`);
    console.log(`  ${highlighted}`);
  }

  console.log();

  await promptOpenConfigFile(configPath);
}

export function showConfigInfo(): void {
  console.log();
  console.log(`  ${bold('All Available Configuration Options')}`);
  console.log();
  console.log(
    `  ${dim('These options can be set as environment variables in your MCP config.')}`
  );
  console.log(
    `  ${dim('Add them to the "env" object in your octocode server configuration.')}`
  );
  console.log();
  console.log(`  ${dim('Example config:')}`);
  console.log(`  ${dim('{')}
  ${dim('  "mcpServers": {')}
  ${dim('    "octocode": {')}
  ${dim('      "command": "npx",')}
  ${dim('      "args": ["-y", "@octocodeai/mcp@latest"],')}
  ${c('green', '      "env": { "ENABLE_LOCAL": "1" }')}
  ${dim('    }')}
  ${dim('  }')}
  ${dim('}')}`);
  console.log();
  console.log(c('blue', '━'.repeat(66)));
  console.log();

  for (const option of ALL_CONFIG_OPTIONS) {
    const typeColor =
      option.type === 'boolean'
        ? 'green'
        : option.type === 'number'
          ? 'yellow'
          : option.type === 'array'
            ? 'magenta'
            : 'cyan';

    console.log(`  ${c('cyan', option.envVar)} ${dim(`(${option.type})`)}`);
    console.log(`    ${option.description}`);
    console.log(`    ${dim('Default:')} ${getDisplayDefault(option)}`);
    console.log(
      `    ${dim('Example:')} ${c(typeColor, getExampleValue(option))}`
    );
    console.log();
  }
}
