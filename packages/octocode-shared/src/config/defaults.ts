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

/**
 * Default GitHub configuration
 */
export const DEFAULT_GITHUB_CONFIG: RequiredGitHubConfig = {
  apiUrl: 'https://api.github.com',
};

/**
 * Default local tools configuration
 */
export const DEFAULT_LOCAL_CONFIG: RequiredLocalConfig = {
  enabled: true,
  enableClone: false,
  allowedPaths: [],
};

/**
 * Default tools configuration
 */
export const DEFAULT_TOOLS_CONFIG: RequiredToolsConfig = {
  enabled: null,
  enableAdditional: null,
  disabled: null,
};

/**
 * Default network configuration
 */
export const DEFAULT_NETWORK_CONFIG: RequiredNetworkConfig = {
  timeout: 30000,
  maxRetries: 3,
};

/**
 * Default telemetry configuration
 */
export const DEFAULT_TELEMETRY_CONFIG: RequiredTelemetryConfig = {
  logging: true,
};

/**
 * Default LSP configuration
 */
export const DEFAULT_LSP_CONFIG: RequiredLspConfig = {
  configPath: undefined,
};

/**
 * Default output configuration
 */
export const DEFAULT_OUTPUT_CONFIG: RequiredOutputConfig = {
  format: 'yaml',
  pagination: {
    // THE single pagination char limit for every tool result (~2000 tokens).
    // Larger result sets are reached by paginating, never one big payload.
    defaultCharLength: 8000,
  },
};

/**
 * Complete default configuration
 * Used as fallback when .octocoderc is missing or invalid
 */
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

/** Minimum timeout - 5 seconds (prevents accidental misconfiguration) */
export const MIN_TIMEOUT = 5000;

/** Maximum timeout - 5 minutes */
export const MAX_TIMEOUT = 300000;

/** Minimum retries */
export const MIN_RETRIES = 0;

/** Maximum retries */
export const MAX_RETRIES = 10;

/** Minimum output char-length pagination budget */
export const MIN_OUTPUT_DEFAULT_CHAR_LENGTH = 1000;

/** Maximum output char-length pagination budget */
export const MAX_OUTPUT_DEFAULT_CHAR_LENGTH = 50000;
