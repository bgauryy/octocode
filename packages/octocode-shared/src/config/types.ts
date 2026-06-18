export const CONFIG_SCHEMA_VERSION = 1;

export const CONFIG_FILE_NAME = '.octocoderc';

export interface GitHubConfigOptions {
  apiUrl?: string;
}

export interface LocalConfigOptions {
  enabled?: boolean;

  enableClone?: boolean;

  allowedPaths?: string[];

  workspaceRoot?: string;
}

export interface ToolsConfigOptions {
  enabled?: string[] | null;

  enableAdditional?: string[] | null;

  disabled?: string[] | null;
}

export interface NetworkConfigOptions {
  timeout?: number;

  maxRetries?: number;
}

export interface TelemetryConfigOptions {
  logging?: boolean;
}

export interface LspConfigOptions {
  configPath?: string;
}

export interface OutputPaginationConfigOptions {
  defaultCharLength?: number;
}

export type MinifyMode = 'none' | 'standard' | 'symbols';

export interface OutputConfigOptions {
  format?: 'yaml' | 'json';

  pagination?: OutputPaginationConfigOptions;
}

export interface OctocodeConfig {
  $schema?: string;

  version?: number;

  github?: GitHubConfigOptions;

  local?: LocalConfigOptions;

  tools?: ToolsConfigOptions;

  network?: NetworkConfigOptions;

  telemetry?: TelemetryConfigOptions;

  lsp?: LspConfigOptions;

  output?: OutputConfigOptions;
}

export interface RequiredGitHubConfig {
  apiUrl: string;
}

export interface RequiredLocalConfig {
  enabled: boolean;
  enableClone: boolean;
  allowedPaths: string[];
  workspaceRoot: string | undefined;
}

export interface RequiredToolsConfig {
  enabled: string[] | null;
  enableAdditional: string[] | null;
  disabled: string[] | null;
}

export interface RequiredNetworkConfig {
  timeout: number;
  maxRetries: number;
}

export interface RequiredTelemetryConfig {
  logging: boolean;
}

export interface RequiredLspConfig {
  configPath: string | undefined;
}

export interface RequiredOutputPaginationConfig {
  defaultCharLength: number;
}

export interface RequiredOutputConfig {
  format: 'yaml' | 'json';
  pagination: RequiredOutputPaginationConfig;
}

export interface ResolvedConfig {
  version: number;

  github: RequiredGitHubConfig;

  local: RequiredLocalConfig;

  tools: RequiredToolsConfig;

  network: RequiredNetworkConfig;

  telemetry: RequiredTelemetryConfig;

  lsp: RequiredLspConfig;

  output: RequiredOutputConfig;

  source: 'file' | 'defaults' | 'mixed';

  configPath?: string;
}

export interface ValidationResult {
  valid: boolean;

  errors: string[];

  warnings: string[];

  config?: OctocodeConfig;
}

export interface LoadConfigResult {
  success: boolean;

  config?: OctocodeConfig;

  error?: string;

  path: string;
}
