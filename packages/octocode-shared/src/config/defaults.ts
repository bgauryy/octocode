import type {
  RequiredGitHubConfig,
  RequiredLocalConfig,
  RequiredToolsConfig,
  RequiredNetworkConfig,
  RequiredTelemetryConfig,
  RequiredLspConfig,
  RequiredOutputConfig,
  ResolvedConfig,
} from './types.js';

export const DEFAULT_GITHUB_CONFIG: RequiredGitHubConfig = {
  apiUrl: 'https://api.github.com',
};

export const DEFAULT_LOCAL_CONFIG: RequiredLocalConfig = {
  enabled: true,
  enableClone: false,
  allowedPaths: [],
  workspaceRoot: undefined,
};

export const DEFAULT_TOOLS_CONFIG: RequiredToolsConfig = {
  enabled: null,
  enableAdditional: null,
  disabled: null,
};

export const DEFAULT_NETWORK_CONFIG: RequiredNetworkConfig = {
  timeout: 30000,
  maxRetries: 3,
};

export const DEFAULT_TELEMETRY_CONFIG: RequiredTelemetryConfig = {
  logging: true,
};

export const DEFAULT_LSP_CONFIG: RequiredLspConfig = {
  configPath: undefined,
};

export const DEFAULT_OUTPUT_CONFIG: RequiredOutputConfig = {
  format: 'yaml',
  pagination: {
    defaultCharLength: 20000,
  },
};

export const DEFAULT_CONFIG: Omit<ResolvedConfig, 'source' | 'configPath'> = {
  version: 1,
  github: DEFAULT_GITHUB_CONFIG,
  local: DEFAULT_LOCAL_CONFIG,
  tools: DEFAULT_TOOLS_CONFIG,
  network: DEFAULT_NETWORK_CONFIG,
  telemetry: DEFAULT_TELEMETRY_CONFIG,
  lsp: DEFAULT_LSP_CONFIG,
  output: DEFAULT_OUTPUT_CONFIG,
};

export const MIN_TIMEOUT = 5000;

export const MAX_TIMEOUT = 300000;

export const MIN_RETRIES = 0;

export const MAX_RETRIES = 10;

export const MIN_OUTPUT_DEFAULT_CHAR_LENGTH = 1000;

export const MAX_OUTPUT_DEFAULT_CHAR_LENGTH = 50000;
