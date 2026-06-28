export { runInspectFlow } from './inspect-flow.js';

import { c, bold, dim } from '../../utils/colors.js';
import {
  loadInquirer,
  confirm,
  select,
  input,
  checkbox,
} from '../../utils/prompts.js';
import { separatorChoice } from '../../utils/prompt-separator.js';
import { Spinner } from '../../utils/spinner.js';
import { openInEditor } from '../../utils/platform.js';
import {
  readMCPConfig,
  writeMCPConfig,
  getMCPConfigPath,
  isOctocodeConfigured,
  MCP_CLIENTS,
} from '../../utils/mcp-config.js';
import { selectMCPClient } from '../install/prompts.js';
import type { MCPConfig } from '../../types/index.js';

const ALL_AVAILABLE_TOOLS = {
  github: [
    {
      id: 'ghSearchCode',
      name: 'Search Code',
      description: 'Search for code patterns in GitHub repositories',
    },
    {
      id: 'ghGetFileContent',
      name: 'Get File Content',
      description: 'Fetch file content from GitHub repositories',
    },
    {
      id: 'ghViewRepoStructure',
      name: 'View Repo Structure',
      description: 'Browse repository directory structure',
    },
    {
      id: 'ghSearchRepos',
      name: 'Search Repositories',
      description: 'Search for GitHub repositories',
    },
    {
      id: 'ghHistoryResearch',
      name: 'History Research',
      description: 'Search pull requests or commit history',
    },
    {
      id: 'npmSearch',
      name: 'Package Search',
      description: 'Search npm/Python packages and find their repos',
    },
  ],

  local: [
    {
      id: 'localSearchCode',
      name: 'Ripgrep Search',
      description: 'Fast content search with regex support',
    },
    {
      id: 'localViewStructure',
      name: 'View Structure',
      description: 'Browse local directory structure',
    },
    {
      id: 'localFindFiles',
      name: 'Find Files',
      description: 'Find files by name, time, size, permissions',
    },
    {
      id: 'localGetFileContent',
      name: 'Fetch Content',
      description: 'Read targeted sections of local files',
    },
  ],
} as const;

interface ConfigOption {
  id: string;
  envVar: string;
  name: string;
  description: string;
  type: 'boolean' | 'string' | 'number' | 'array';
  defaultValue: string;
  validation?: {
    min?: number;
    max?: number;
    pattern?: RegExp;
  };

  toolCategory?: 'all' | 'github' | 'local';
}

type ConfigMenuChoice = 'edit' | 'view' | 'show-json' | 'back';
type EditConfigChoice = string | 'save' | 'reset' | 'back';

const ALL_CONFIG_OPTIONS: ConfigOption[] = [
  {
    id: 'enableLocal',
    envVar: 'ENABLE_LOCAL',
    name: 'Local File Tools',
    description:
      'Enable local file exploration tools for searching and browsing local files',
    type: 'boolean',
    defaultValue: 'false',
  },
  {
    id: 'githubApiUrl',
    envVar: 'GITHUB_API_URL',
    name: 'GitHub API URL',
    description: 'Custom GitHub API endpoint (for GitHub Enterprise)',
    type: 'string',
    defaultValue: 'https://api.github.com',
  },
  {
    id: 'toolsToRun',
    envVar: 'TOOLS_TO_RUN',
    name: 'Tools to Run',
    description: 'Specific tools to enable (all others disabled)',
    type: 'array',
    defaultValue: '',
    toolCategory: 'all',
  },
  {
    id: 'enableTools',
    envVar: 'ENABLE_TOOLS',
    name: 'Enable Tools',
    description: 'Additional tools to enable',
    type: 'array',
    defaultValue: '',
    toolCategory: 'all',
  },
  {
    id: 'disableTools',
    envVar: 'DISABLE_TOOLS',
    name: 'Disable Tools',
    description: 'Tools to disable',
    type: 'array',
    defaultValue: '',
    toolCategory: 'all',
  },
  {
    id: 'requestTimeout',
    envVar: 'REQUEST_TIMEOUT',
    name: 'Request Timeout',
    description: 'API request timeout in milliseconds',
    type: 'number',
    defaultValue: '30000',
    validation: { min: 30000, max: 600000 },
  },
  {
    id: 'maxRetries',
    envVar: 'MAX_RETRIES',
    name: 'Max Retries',
    description: 'Maximum number of API retry attempts',
    type: 'number',
    defaultValue: '3',
    validation: { min: 0, max: 10 },
  },
];

function getAllTools(): Array<{
  id: string;
  name: string;
  description: string;
  category: 'github' | 'local';
}> {
  return [
    ...ALL_AVAILABLE_TOOLS.github.map(t => ({
      ...t,
      category: 'github' as const,
    })),
    ...ALL_AVAILABLE_TOOLS.local.map(t => ({
      ...t,
      category: 'local' as const,
    })),
  ];
}

function getCurrentValue(
  env: Record<string, string>,
  option: ConfigOption
): string {
  const value = env[option.envVar];
  if (value === undefined || value === null || value === '') {
    return option.defaultValue;
  }
  return value;
}

function formatDisplayValue(
  option: ConfigOption,
  value: string,
  isModified = false
): string {
  const modifiedMarker = isModified ? c('yellow', ' •') : '';

  if (option.type === 'boolean') {
    const isEnabled = value === '1' || value.toLowerCase() === 'true';
    const icon = isEnabled ? c('green', '✅') : c('dim', '○');
    const label = isEnabled ? c('green', 'enabled') : c('dim', 'disabled');
    return `${icon} ${label}${modifiedMarker}`;
  }
  if (option.type === 'array') {
    if (!value || value === '') {
      const defaultLabel =
        option.id === 'toolsToRun' ? '(all tools)' : '(none)';
      return `${c('dim', '○')} ${c('dim', defaultLabel)}${modifiedMarker}`;
    }
    const tools = value.split(',').filter(t => t.trim());
    const toolsDisplay =
      tools.length > 2
        ? `${tools.slice(0, 2).join(', ')} ${c('dim', `+${tools.length - 2} more`)}`
        : tools.join(', ');
    return `${c('green', '●')} ${toolsDisplay}${modifiedMarker}`;
  }
  if (option.type === 'number') {
    if (value === option.defaultValue) {
      return `${c('dim', '○')} ${value} ${c('dim', '(default)')}${modifiedMarker}`;
    }
    return `${c('cyan', '●')} ${c('cyan', value)}${modifiedMarker}`;
  }

  if (value === option.defaultValue) {
    return `${c('dim', '○')} ${c('dim', value)}${modifiedMarker}`;
  }
  return `${c('cyan', '●')} ${c('cyan', value)}${modifiedMarker}`;
}

function parseBooleanValue(value: string): boolean {
  return value === '1' || value.toLowerCase() === 'true';
}

async function showConfigMenu(): Promise<ConfigMenuChoice> {
  const choice = await select<ConfigMenuChoice>({
    message: '',
    choices: [
      {
        name: '- Edit configuration',
        value: 'edit',
        description: 'Configure all @octocodeai/mcp settings for a client',
      },
      {
        name: '- View all configuration options',
        value: 'view',
        description: 'Show available environment variables and their defaults',
      },
      {
        name: '- Show current JSON config',
        value: 'show-json',
        description: 'Display the actual MCP config JSON for a client',
      },
      separatorChoice<{ name: string; value: ConfigMenuChoice }>(),
      {
        name: `${c('dim', '- Back to main menu')}`,
        value: 'back',
      },
    ],
    pageSize: 10,
    loop: false,
    theme: {
      prefix: '  ',
      style: {
        highlight: (text: string) => c('cyan', text),
      },
    },
  });

  return choice;
}

export async function runConfigOptionsFlow(): Promise<void> {
  await loadInquirer();

  const choice = await showConfigMenu();

  switch (choice) {
    case 'view':
      showConfigInfo();

      await pressEnterToContinue();
      break;

    case 'edit':
      await runEditConfigFlow();
      break;

    case 'show-json':
      await showCurrentJsonConfig();
      break;

    case 'back':
    default:
      break;
  }
}

async function pressEnterToContinue(): Promise<void> {
  console.log();
  await input({
    message: dim('Press Enter to continue...'),
    default: '',
  });
}

type OpenChoice = 'cursor' | 'vscode' | 'default' | 'no';

async function promptOpenConfigFile(configPath: string): Promise<void> {
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

async function editBooleanOption(
  option: ConfigOption,
  currentValue: string
): Promise<string | null> {
  const isEnabled = parseBooleanValue(currentValue);
  const currentStatus = isEnabled
    ? c('green', 'enabled')
    : c('dim', 'disabled');

  console.log();
  console.log(`  ${bold(option.name)}`);
  console.log(`  ${dim(option.description)}`);
  console.log(`  ${dim('Current:')} ${currentStatus}`);
  console.log();

  type BoolChoice = 'enable' | 'disable' | 'cancel';
  const choice = await select<BoolChoice>({
    message: `${option.name}:`,
    choices: [
      {
        name: `${c('green', '✅')} Enable`,
        value: 'enable',
      },
      {
        name: `${c('yellow', '○')} Disable`,
        value: 'disable',
      },
      separatorChoice<{ name: string; value: BoolChoice }>(),
      {
        name: `${c('dim', '- Cancel')}`,
        value: 'cancel',
      },
    ],
    loop: false,
  });

  if (choice === 'cancel') return null;
  return choice === 'enable' ? '1' : 'false';
}

async function editStringOption(
  option: ConfigOption,
  currentValue: string
): Promise<string | null> {
  const displayCurrent =
    currentValue && currentValue !== option.defaultValue
      ? c('cyan', currentValue)
      : c('dim', currentValue || option.defaultValue);

  console.log();
  console.log(`  ${bold(option.name)}`);
  console.log(`  ${dim(option.description)}`);
  console.log(`  ${dim('Current:')} ${displayCurrent}`);
  console.log(`  ${dim('Default:')} ${option.defaultValue}`);
  console.log(`  ${dim('(Leave empty and press Enter to cancel)')}`);
  console.log();

  const newValue = await input({
    message: `${option.name}:`,
    default: '',
    validate: (value: string) => {
      if (!value.trim()) {
        return true;
      }
      if (
        option.validation?.pattern &&
        !option.validation.pattern.test(value)
      ) {
        return 'Invalid format';
      }
      return true;
    },
  });

  if (!newValue.trim()) {
    return null;
  }

  return newValue === option.defaultValue ? '' : newValue;
}

async function editNumberOption(
  option: ConfigOption,
  currentValue: string
): Promise<string | null> {
  const displayCurrent =
    currentValue && currentValue !== option.defaultValue
      ? c('cyan', currentValue)
      : c('dim', currentValue || option.defaultValue);

  console.log();
  console.log(`  ${bold(option.name)}`);
  console.log(`  ${dim(option.description)}`);
  console.log(`  ${dim('Current:')} ${displayCurrent}`);
  if (
    option.validation?.min !== undefined ||
    option.validation?.max !== undefined
  ) {
    const min = option.validation?.min ?? 0;
    const max = option.validation?.max ?? Infinity;
    console.log(`  ${dim('Range:')} ${min} - ${max === Infinity ? '∞' : max}`);
  }
  console.log(`  ${dim('Default:')} ${option.defaultValue}`);
  console.log(`  ${dim('(Leave empty and press Enter to cancel)')}`);
  console.log();

  const newValue = await input({
    message: `${option.name}:`,
    default: '',
    validate: (value: string) => {
      if (!value.trim()) {
        return true;
      }
      const num = parseInt(value, 10);
      if (isNaN(num)) {
        return 'Please enter a valid number';
      }
      if (option.validation?.min !== undefined && num < option.validation.min) {
        return `Minimum value is ${option.validation.min}`;
      }
      if (option.validation?.max !== undefined && num > option.validation.max) {
        return `Maximum value is ${option.validation.max}`;
      }
      return true;
    },
  });

  if (!newValue.trim()) {
    return null;
  }

  return newValue === option.defaultValue ? '' : newValue;
}

async function editArrayOption(
  option: ConfigOption,
  currentValue: string
): Promise<string | null> {
  const allTools = getAllTools();
  const currentTools = currentValue
    ? currentValue
        .split(',')
        .map(t => t.trim())
        .filter(Boolean)
    : [];

  const currentDisplay =
    currentTools.length > 0
      ? currentTools.join(', ')
      : option.id === 'toolsToRun'
        ? c('dim', '(all tools)')
        : c('dim', '(none)');

  console.log();
  console.log(`  ${bold(option.name)}`);
  console.log(`  ${dim(option.description)}`);
  console.log(`  ${dim('Current:')} ${currentDisplay}`);
  console.log();

  type ArrayAction = 'select' | 'clear' | 'cancel';
  const action = await select<ArrayAction>({
    message: `${option.name}:`,
    choices: [
      {
        name: 'EDIT Select tools',
        value: 'select',
        description: 'Choose which tools to include',
      },
      {
        name: `${c('yellow', '↺')} Clear all`,
        value: 'clear',
        description:
          option.id === 'toolsToRun'
            ? 'Reset to all tools enabled'
            : 'Remove all tools from this list',
      },
      separatorChoice<{ name: string; value: ArrayAction }>(),
      {
        name: `${c('dim', '- Cancel')}`,
        value: 'cancel',
      },
    ],
    loop: false,
  });

  if (action === 'cancel') {
    return null;
  }

  if (action === 'clear') {
    return '';
  }

  const choices: Array<{
    name: string;
    value: string;
    checked?: boolean;
    description?: string;
  }> = [];

  choices.push({
    name: c('blue', '── GitHub Tools ──'),
    value: '__separator_github__',
    disabled: true,
  } as unknown as (typeof choices)[number]);

  for (const tool of allTools.filter(t => t.category === 'github')) {
    choices.push({
      name: `${tool.name} ${c('dim', `(${tool.id})`)}`,
      value: tool.id,
      checked: currentTools.includes(tool.id),
      description: tool.description,
    });
  }

  choices.push({
    name: c('yellow', '── Local Tools ──'),
    value: '__separator_local__',
    disabled: true,
  } as unknown as (typeof choices)[number]);

  for (const tool of allTools.filter(t => t.category === 'local')) {
    choices.push({
      name: `${tool.name} ${c('dim', `(${tool.id})`)}`,
      value: tool.id,
      checked: currentTools.includes(tool.id),
      description: tool.description,
    });
  }

  choices.push({
    name: c('dim', '── Actions ──'),
    value: '__separator_actions__',
    disabled: true,
  } as unknown as (typeof choices)[number]);

  choices.push({
    name: `${c('dim', '- Cancel (keep current)')}`,
    value: '__cancel__',
    checked: false,
    description: 'Go back without changes',
  });

  console.log();
  console.log(
    `  ${dim('Use Space to select/deselect, Enter to confirm, or select Cancel')}`
  );

  const selected = await checkbox<string>({
    message: `Select tools for ${option.name}:`,
    choices,
    pageSize: 16,
    loop: false,
    theme: {
      prefix: '  ',
      style: {
        highlight: (text: string) => c('cyan', text),
        message: (text: string) => bold(text),
      },
    },
  });

  if (selected.includes('__cancel__')) {
    return null;
  }

  const validTools = selected.filter(
    t => !t.startsWith('__separator') && t !== '__cancel__'
  );
  return validTools.length > 0 ? validTools.join(',') : '';
}

function isValueModified(
  originalEnv: Record<string, string>,
  currentEnv: Record<string, string>,
  option: ConfigOption
): boolean {
  const originalValue = originalEnv[option.envVar] ?? '';
  const currentValue = currentEnv[option.envVar] ?? '';
  return originalValue !== currentValue;
}

function countModifiedOptions(
  originalEnv: Record<string, string>,
  currentEnv: Record<string, string>
): number {
  let count = 0;
  for (const option of ALL_CONFIG_OPTIONS) {
    if (isValueModified(originalEnv, currentEnv, option)) {
      count++;
    }
  }
  return count;
}

async function showEditConfigMenu(
  workingEnv: Record<string, string>,
  originalEnv: Record<string, string>
): Promise<EditConfigChoice> {
  const choices: Array<{
    name: string;
    value: EditConfigChoice;
    description?: string;
  }> = [];

  const booleanOptions = ALL_CONFIG_OPTIONS.filter(o => o.type === 'boolean');
  const stringOptions = ALL_CONFIG_OPTIONS.filter(o => o.type === 'string');
  const numberOptions = ALL_CONFIG_OPTIONS.filter(o => o.type === 'number');
  const arrayOptions = ALL_CONFIG_OPTIONS.filter(o => o.type === 'array');

  const modifiedCount = countModifiedOptions(originalEnv, workingEnv);
  const statusHeader =
    modifiedCount > 0
      ? `${c('yellow', '●')} ${modifiedCount} unsaved ${modifiedCount === 1 ? 'change' : 'changes'}`
      : `${c('green', '●')} No unsaved changes`;

  choices.push({
    name: `${c('dim', '──')} ${statusHeader} ${c('dim', '──')}`,
    value: '__status__',
  });

  if (booleanOptions.length > 0) {
    choices.push({
      name: c('dim', '── Features ──'),
      value: '__sep1__',
    });
    for (const option of booleanOptions) {
      const value = getCurrentValue(workingEnv, option);
      const isModified = isValueModified(originalEnv, workingEnv, option);
      const displayValue = formatDisplayValue(option, value, isModified);
      choices.push({
        name: `${option.name}: ${displayValue}`,
        value: option.id,
        description: option.description,
      });
    }
  }

  if (stringOptions.length > 0) {
    choices.push({
      name: c('dim', '── Endpoints ──'),
      value: '__sep2__',
    });
    for (const option of stringOptions) {
      const value = getCurrentValue(workingEnv, option);
      const isModified = isValueModified(originalEnv, workingEnv, option);
      const displayValue = formatDisplayValue(option, value, isModified);
      choices.push({
        name: `${option.name}: ${displayValue}`,
        value: option.id,
        description: option.description,
      });
    }
  }

  if (numberOptions.length > 0) {
    choices.push({
      name: c('dim', '── Performance ──'),
      value: '__sep3__',
    });
    for (const option of numberOptions) {
      const value = getCurrentValue(workingEnv, option);
      const isModified = isValueModified(originalEnv, workingEnv, option);
      const displayValue = formatDisplayValue(option, value, isModified);
      choices.push({
        name: `${option.name}: ${displayValue}`,
        value: option.id,
        description: option.description,
      });
    }
  }

  if (arrayOptions.length > 0) {
    choices.push({
      name: c('dim', '── Tool Selection ──'),
      value: '__sep4__',
    });
    for (const option of arrayOptions) {
      const value = getCurrentValue(workingEnv, option);
      const isModified = isValueModified(originalEnv, workingEnv, option);
      const displayValue = formatDisplayValue(option, value, isModified);
      choices.push({
        name: `${option.name}: ${displayValue}`,
        value: option.id,
        description: option.description,
      });
    }
  }

  choices.push({
    name: c('dim', '── Actions ──'),
    value: '__sep5__',
  });

  if (modifiedCount > 0) {
    choices.push({
      name: `${c('green', 'Save')} Save changes ${c('yellow', `(${modifiedCount})`)}`,
      value: 'save',
      description: 'Save configuration and exit',
    });
  } else {
    choices.push({
      name: `${c('dim', 'Save')} Save changes`,
      value: 'save',
      description: dim('No changes to save'),
    });
  }

  choices.push({
    name: `${c('yellow', '↺')} Reset to defaults`,
    value: 'reset',
    description: 'Clear all custom configuration',
  });
  choices.push({
    name: `${c('dim', '- Back')}`,
    value: 'back',
  });

  const choice = await select<EditConfigChoice>({
    message: 'Select option to configure:',
    choices,
    pageSize: 20,
    loop: false,
    theme: {
      prefix: '  ',
      style: {
        highlight: (text: string) => c('cyan', text),
        message: (text: string) => bold(text),
      },
    },
  });

  return choice;
}

async function runEditConfigFlow(): Promise<void> {
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

  console.log();
  console.log(`  ${dim('Config file:')} ${c('cyan', configPath)}`);
  console.log(`  ${dim('Client:')} ${clientInfo.name}`);
  console.log();

  const originalEnv = { ...(config.mcpServers?.octocode?.env || {}) };
  const workingEnv = { ...originalEnv };

  let editing = true;
  while (editing) {
    const choice = await showEditConfigMenu(workingEnv, originalEnv);

    if (choice.startsWith('__sep') || choice === '__status__') {
      continue;
    }

    switch (choice) {
      case 'save': {
        const hasChanges =
          JSON.stringify(originalEnv) !== JSON.stringify(workingEnv);
        if (!hasChanges) {
          console.log();
          console.log(`  ${dim('No changes to save.')}`);
          console.log();
          editing = false;
          break;
        }

        const spinner = new Spinner('Saving configuration...').start();

        const cleanEnv: Record<string, string> = {};
        for (const [key, value] of Object.entries(workingEnv)) {
          if (value && value !== '') {
            cleanEnv[key] = value;
          }
        }

        const updatedConfig: MCPConfig = {
          ...config,
          mcpServers: {
            ...config.mcpServers,
            octocode: {
              ...config.mcpServers!.octocode,
              env: Object.keys(cleanEnv).length > 0 ? cleanEnv : undefined,
            },
          },
        };

        if (
          updatedConfig.mcpServers?.octocode?.env &&
          Object.keys(updatedConfig.mcpServers.octocode.env).length === 0
        ) {
          delete updatedConfig.mcpServers.octocode.env;
        }

        const result = writeMCPConfig(configPath, updatedConfig);

        if (result.success) {
          spinner.succeed('Configuration saved!');
          console.log();
          console.log(`  ${c('green', '✅')} Config saved to: ${configPath}`);
          if (result.backupPath) {
            console.log(`  ${dim('Backup:')} ${result.backupPath}`);
          }
          console.log();
          console.log(
            `  ${bold('Note:')} Restart ${clientInfo.name} for changes to take effect.`
          );
          await promptOpenConfigFile(configPath);
        } else {
          spinner.fail('Failed to save configuration');
          console.log();
          console.log(`  ${c('red', 'X')} ${result.error || 'Unknown error'}`);
          console.log();
        }
        editing = false;
        break;
      }

      case 'reset': {
        const confirmReset = await confirm({
          message: 'Reset all configuration to defaults?',
          default: false,
        });
        if (confirmReset) {
          for (const key of Object.keys(workingEnv)) {
            delete workingEnv[key];
          }
          console.log(`  ${c('yellow', '↺')} Configuration reset to defaults`);
        }
        break;
      }

      case 'back': {
        const hasUnsavedChanges =
          JSON.stringify(originalEnv) !== JSON.stringify(workingEnv);
        if (hasUnsavedChanges) {
          const confirmDiscard = await confirm({
            message: 'Discard unsaved changes?',
            default: false,
          });
          if (!confirmDiscard) {
            break;
          }
        }
        editing = false;
        break;
      }

      default: {
        const option = ALL_CONFIG_OPTIONS.find(o => o.id === choice);
        if (!option) break;

        const currentValue = getCurrentValue(workingEnv, option);
        let newValue: string | null = null;

        switch (option.type) {
          case 'boolean':
            newValue = await editBooleanOption(option, currentValue);
            break;
          case 'string':
            newValue = await editStringOption(option, currentValue);
            break;
          case 'number':
            newValue = await editNumberOption(option, currentValue);
            break;
          case 'array':
            newValue = await editArrayOption(option, currentValue);
            break;
          default:
            break;
        }

        if (newValue !== null) {
          if (newValue === '' || newValue === option.defaultValue) {
            delete workingEnv[option.envVar];
          } else {
            workingEnv[option.envVar] = newValue;
          }
        }
        break;
      }
    }
  }
}

async function showCurrentJsonConfig(): Promise<void> {
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

function getExampleValue(option: ConfigOption): string {
  switch (option.id) {
    case 'enableLocal':
      return 'ENABLE_LOCAL=1';
    case 'githubApiUrl':
      return 'GITHUB_API_URL=https://github.mycompany.com/api/v3';
    case 'toolsToRun':
      return 'TOOLS_TO_RUN=ghSearchCode,ghGetFileContent';
    case 'enableTools':
      return 'ENABLE_TOOLS=localSearchCode,localFindFiles';
    case 'disableTools':
      return 'DISABLE_TOOLS=ghHistoryResearch';
    case 'requestTimeout':
      return 'REQUEST_TIMEOUT=60000';
    case 'maxRetries':
      return 'MAX_RETRIES=5';
    default:
      return `${option.envVar}=${option.defaultValue}`;
  }
}

function getDisplayDefault(option: ConfigOption): string {
  if (option.type === 'array') {
    return option.id === 'toolsToRun' ? '(all tools)' : '(none)';
  }
  return option.defaultValue;
}

function showConfigInfo(): void {
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
