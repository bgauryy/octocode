/**
 * Global Configuration Types
 *
 * Type definitions for ~/.octocode/.octocoderc configuration file.
 * Provides global control over Octocode execution across all packages.
 */

/**
 * Schema version for forward compatibility
 */
export const CONFIG_SCHEMA_VERSION = 1;

/**
 * Config file name
 */
export const CONFIG_FILE_NAME = '.octocoderc';

/**
 * GitHub-specific configuration
 */
export interface GitHubConfigOptions {
  /** GitHub API URL (default: https://api.github.com) */
  apiUrl?: string;
}

/**
 * Local filesystem tools configuration
 */
export interface LocalConfigOptions {
  /** Enable local filesystem tools (default: false) */
  enabled?: boolean;
  /** Enable clone/fetch repository functionality (default: false, requires enabled=true) */
  enableClone?: boolean;
  /** Restrict to specific paths (empty = all allowed) */
  allowedPaths?: string[];
}

/**
 * Tool enable/disable configuration
 */
export interface ToolsConfigOptions {
  /** Whitelist of tools to enable (null = all) */
  enabled?: string[] | null;
  /** Additional tools to enable on top of defaults */
  enableAdditional?: string[] | null;
  /** Blacklist of tools to disable */
  disabled?: string[] | null;
}

/**
 * Network/performance configuration
 */
export interface NetworkConfigOptions {
  /** Request timeout in milliseconds (min: 5000, default: 30000) */
  timeout?: number;
  /** Max retry attempts (0-10, default: 3) */
  maxRetries?: number;
}

/**
 * Telemetry and logging configuration
 */
export interface TelemetryConfigOptions {
  /** Enable debug logging (default: true) */
  logging?: boolean;
}

/**
 * LSP tools configuration
 */
export interface LspConfigOptions {
  /** Path to custom LSP servers config file (default: ${OCTOCODE_HOME:-~/.octocode}/lsp-servers.json) */
  configPath?: string;
}

/**
 * Output format configuration
 */
export interface OutputPaginationConfigOptions {
  /** Default character budget for automatic payload pagination (default: 8000) */
  defaultCharLength?: number;
}

export interface OutputConfigOptions {
  /** Response serialization format: 'yaml' (token-efficient) or 'json' (raw) */
  format?: 'yaml' | 'json';
  /** Shared pagination defaults for tool outputs */
  pagination?: OutputPaginationConfigOptions;
}

/**
 * Complete .octocoderc configuration schema (raw file structure)
 */
export interface OctocodeConfig {
  /** JSON Schema URL (optional) */
  $schema?: string;
  /** Config schema version */
  version?: number;
  /** GitHub configuration */
  github?: GitHubConfigOptions;
  /** Local tools configuration */
  local?: LocalConfigOptions;
  /** Tool enable/disable */
  tools?: ToolsConfigOptions;
  /** Network settings */
  network?: NetworkConfigOptions;
  /** Telemetry settings */
  telemetry?: TelemetryConfigOptions;
  /** LSP settings */
  lsp?: LspConfigOptions;
  /** Output format settings */
  output?: OutputConfigOptions;
}

/**
 * Required versions of config options (all fields populated)
 */
export interface RequiredGitHubConfig {
  apiUrl: string;
}

export interface RequiredLocalConfig {
  enabled: boolean;
  enableClone: boolean;
  allowedPaths: string[];
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

/**
 * Resolved configuration with all defaults applied
 */
export interface ResolvedConfig {
  /** Config schema version */
  version: number;
  /** GitHub configuration */
  github: RequiredGitHubConfig;
  /** Local tools configuration */
  local: RequiredLocalConfig;
  /** Tool enable/disable */
  tools: RequiredToolsConfig;
  /** Network settings */
  network: RequiredNetworkConfig;
  /** Telemetry settings */
  telemetry: RequiredTelemetryConfig;
  /** LSP settings */
  lsp: RequiredLspConfig;
  /** Output format settings */
  output: RequiredOutputConfig;
  /** Source of this configuration */
  source: 'file' | 'defaults' | 'mixed';
  /** Path to config file (if loaded from file) */
  configPath?: string;
}

/**
 * Result of configuration validation
 */
export interface ValidationResult {
  /** Whether the config is valid */
  valid: boolean;
  /** List of validation errors */
  errors: string[];
  /** List of validation warnings */
  warnings: string[];
  /** Validated config (with invalid fields removed) */
  config?: OctocodeConfig;
}

/**
 * Result of loading config from file
 */
export interface LoadConfigResult {
  /** Whether loading succeeded */
  success: boolean;
  /** Loaded config (if success) */
  config?: OctocodeConfig;
  /** Error message (if failed) */
  error?: string;
  /** Path to config file */
  path: string;
}
