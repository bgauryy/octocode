export type {
  OctocodeConfig,
  ResolvedConfig,
  ValidationResult,
  LoadConfigResult,
  GitHubConfigOptions,
  LocalConfigOptions,
  ToolsConfigOptions,
  NetworkConfigOptions,
  TelemetryConfigOptions,
  LspConfigOptions,
  OutputConfigOptions,
  OutputPaginationConfigOptions,
  RequiredGitHubConfig,
  RequiredLocalConfig,
  RequiredToolsConfig,
  RequiredNetworkConfig,
  RequiredTelemetryConfig,
  RequiredLspConfig,
  RequiredOutputConfig,
  RequiredOutputPaginationConfig,
} from './types.js';

export { CONFIG_SCHEMA_VERSION, CONFIG_FILE_NAME } from './types.js';

export {
  DEFAULT_CONFIG,
  DEFAULT_GITHUB_CONFIG,
  DEFAULT_LOCAL_CONFIG,
  DEFAULT_TOOLS_CONFIG,
  DEFAULT_NETWORK_CONFIG,
  DEFAULT_TELEMETRY_CONFIG,
  DEFAULT_LSP_CONFIG,
  DEFAULT_OUTPUT_CONFIG,
  MIN_TIMEOUT,
  MAX_TIMEOUT,
  MIN_RETRIES,
  MAX_RETRIES,
  MIN_OUTPUT_DEFAULT_CHAR_LENGTH,
  MAX_OUTPUT_DEFAULT_CHAR_LENGTH,
} from './defaults.js';

export { CONFIG_FILE_PATH } from './loader.js';

export {
  loadConfig,
  loadConfigSync,
  configExists,
  getConfigPath,
  getOctocodeDir,
} from './loader.js';

export { validateConfig } from './validator.js';

export {
  getConfig,
  getConfigSync,
  reloadConfig,
  resolveConfig,
  resolveConfigSync,
  invalidateConfigCache,
  getConfigValue,
  _resetConfigCache,
  _getCacheState,
} from './resolver.js';

export { parseLoggingEnv } from './resolverSections.js';

export { OctocodeConfigSchema } from './schemas.js';
