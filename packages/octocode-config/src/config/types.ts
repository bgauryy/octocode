import type {
  OctocodeConfig,
  ResolvedConfigData,
} from './contract.generated.js';

export type {
  ClassificationConfigOptions,
  ClassificationVendor,
  GitHubConfigOptions,
  LocalConfigOptions,
  LspConfigOptions,
  NetworkConfigOptions,
  OctocodeConfig,
  OutputConfigOptions,
  OutputFormat,
  OutputPaginationConfigOptions,
  RequiredClassificationConfig,
  RequiredGitHubConfig,
  RequiredLocalConfig,
  RequiredLspConfig,
  RequiredNetworkConfig,
  RequiredOutputConfig,
  RequiredOutputPaginationConfig,
  RequiredStorageConfig,
  RequiredToolsConfig,
  StorageConfigOptions,
  StorageMode,
  ToolsConfigOptions,
} from './contract.generated.js';
export {
  CONFIG_FILE_NAME,
  CONFIG_SCHEMA_VERSION,
} from './contract.generated.js';

export interface ResolvedConfig extends ResolvedConfigData {
  source: 'defaults' | 'file' | 'env' | 'mixed' | 'invalid';
  configPath?: string;
}

export interface ValidationResult {
  valid: boolean;
  errors: string[];
  warnings: string[];
}

export interface LoadConfigResult {
  config?: OctocodeConfig;
  path: string;
  success: boolean;
  error?: string;
  validation?: ValidationResult;
}

export type MinifyMode = 'none' | 'lines' | 'ast' | 'summary' | 'auto';
