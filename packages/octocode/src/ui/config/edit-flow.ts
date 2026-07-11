import { c, bold, dim } from '../../utils/colors.js';
import { confirm, select } from '../../utils/prompts.js';
import { Spinner } from '../../utils/spinner.js';
import {
  readMCPConfig,
  writeMCPConfig,
  getMCPConfigPath,
  isOctocodeConfigured,
  MCP_CLIENTS,
} from '../../utils/mcp-config.js';
import { selectMCPClient } from '../install/prompts.js';
import type { MCPConfig } from '../../types/index.js';
import {
  ALL_CONFIG_OPTIONS,
  countModifiedOptions,
  formatDisplayValue,
  getCurrentValue,
  isValueModified,
} from './config-data.js';
import {
  editArrayOption,
  editBooleanOption,
  editNumberOption,
  editStringOption,
} from './edit-prompts.js';
import { promptOpenConfigFile } from './view.js';

type EditConfigChoice = string | 'save' | 'reset' | 'back';

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

export async function runEditConfigFlow(): Promise<void> {
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
