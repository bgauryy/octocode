// Config types, defaults, validation, loading, and resolution.
// Single source of truth for all .octocoderc + env-override logic.

export type {
  OctocodeConfig,
  ResolvedConfig,
  ValidationResult,
  LoadConfigResult,
  GitHubConfigOptions,
  LocalConfigOptions,
  ToolsConfigOptions,
  NetworkConfigOptions,
  LspConfigOptions,
  OutputConfigOptions,
  OutputPaginationConfigOptions,
  RequiredGitHubConfig,
  RequiredLocalConfig,
  RequiredToolsConfig,
  RequiredNetworkConfig,
  RequiredLspConfig,
  RequiredOutputConfig,
  RequiredOutputPaginationConfig,
  RequiredSessionConfig,
  MinifyMode,
} from './types.js';

export { CONFIG_SCHEMA_VERSION, CONFIG_FILE_NAME } from './types.js';

export {
  DEFAULT_CONFIG,
  DEFAULT_GITHUB_CONFIG,
  DEFAULT_LOCAL_CONFIG,
  DEFAULT_TOOLS_CONFIG,
  DEFAULT_NETWORK_CONFIG,
  DEFAULT_LSP_CONFIG,
  DEFAULT_OUTPUT_CONFIG,
  MIN_TIMEOUT,
  MAX_TIMEOUT,
  MIN_RETRIES,
  MAX_RETRIES,
  MIN_OUTPUT_DEFAULT_CHAR_LENGTH,
  MAX_OUTPUT_DEFAULT_CHAR_LENGTH,
  DEFAULT_SESSION_CONFIG,
} from './defaults.js';

export {
  type RuntimeSurface,
  setRuntimeSurface,
  getRuntimeSurface,
  _resetRuntimeSurface,
} from './runtimeSurface.js';

export { validateConfig } from './validator.js';

export {
  getConfigFilePath,
  configExists,
  loadConfigSync,
  loadConfig,
} from './loader.js';

export {
  parseBooleanEnv,
  parseIntEnv,
  parseStringArrayEnv,
  resolveGitHub,
  resolveLocal,
  resolveTools,
  resolveNetwork,
  resolveLsp,
  resolveOutput,
  resolveSession,
} from './resolverSections.js';

export {
  resolveConfigSync,
  resolveConfig,
  getConfig,
  getConfigSync,
  reloadConfig,
  invalidateConfigCache,
  _resetConfigCache,
  _getCacheState,
  getConfigValue,
} from './resolver.js';
