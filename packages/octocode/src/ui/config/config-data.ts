import { c } from '../../utils/colors.js';
import { DEFAULT_CONFIG } from '@octocodeai/config';
import {
  DIRECT_TOOL_DISCOVERY_DEFINITIONS,
  getDirectToolCategory,
} from '@octocodeai/octocode-tools-core/schema';

type ToolCategory = 'github' | 'package' | 'local';

interface AvailableTool {
  id: string;
  name: string;
  description: string;
  category: ToolCategory;
}

function categoryFor(toolName: string): ToolCategory {
  const category = getDirectToolCategory(toolName);
  if (category === 'GitHub') return 'github';
  if (category === 'Package') return 'package';
  return 'local';
}

const PUBLIC_TOOLS: AvailableTool[] = DIRECT_TOOL_DISCOVERY_DEFINITIONS.map(
  tool => {
    const id = tool.name;
    return {
      id,
      name: tool.title,
      description: tool.description,
      category: categoryFor(id),
    };
  }
);

export const ALL_AVAILABLE_TOOLS = {
  github: PUBLIC_TOOLS.filter(tool => tool.category === 'github'),
  package: PUBLIC_TOOLS.filter(tool => tool.category === 'package'),
  local: PUBLIC_TOOLS.filter(tool => tool.category === 'local'),
};

export interface ConfigOption {
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

export const ALL_CONFIG_OPTIONS: ConfigOption[] = [
  {
    id: 'enableLocal',
    envVar: 'ENABLE_LOCAL',
    name: 'Local File Tools',
    description:
      'Enable local file exploration tools for searching and browsing local files',
    type: 'boolean',
    defaultValue: String(DEFAULT_CONFIG.local.enabled),
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

export function getAllTools(): Array<{
  id: string;
  name: string;
  description: string;
  category: ToolCategory;
}> {
  return PUBLIC_TOOLS;
}

export function getCurrentValue(
  env: Record<string, string>,
  option: ConfigOption
): string {
  const value = env[option.envVar];
  if (value === undefined || value === null || value === '') {
    return option.defaultValue;
  }
  return value;
}

export function formatDisplayValue(
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

export function parseBooleanValue(value: string): boolean {
  return value === '1' || value.toLowerCase() === 'true';
}

export function isValueModified(
  originalEnv: Record<string, string>,
  currentEnv: Record<string, string>,
  option: ConfigOption
): boolean {
  const originalValue = originalEnv[option.envVar] ?? '';
  const currentValue = currentEnv[option.envVar] ?? '';
  return originalValue !== currentValue;
}

export function countModifiedOptions(
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

export function getExampleValue(option: ConfigOption): string {
  switch (option.id) {
    case 'enableLocal':
      return 'ENABLE_LOCAL=1';
    case 'githubApiUrl':
      return 'GITHUB_API_URL=https://github.mycompany.com/api/v3';
    case 'toolsToRun':
      return 'TOOLS_TO_RUN=ghSearch,ghGetFileContent';
    case 'disableTools':
      return 'DISABLE_TOOLS=ghSearchHistory';
    case 'requestTimeout':
      return 'REQUEST_TIMEOUT=60000';
    case 'maxRetries':
      return 'MAX_RETRIES=5';
    default:
      return `${option.envVar}=${option.defaultValue}`;
  }
}

export function getDisplayDefault(option: ConfigOption): string {
  if (option.type === 'array') {
    return option.id === 'toolsToRun' ? '(all tools)' : '(none)';
  }
  return option.defaultValue;
}
