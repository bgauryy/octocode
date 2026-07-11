import { c } from '../../utils/colors.js';

export const ALL_AVAILABLE_TOOLS = {
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

export function getAllTools(): Array<{
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

export function getDisplayDefault(option: ConfigOption): string {
  if (option.type === 'array') {
    return option.id === 'toolsToRun' ? '(all tools)' : '(none)';
  }
  return option.defaultValue;
}
