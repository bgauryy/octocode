export const CONFIG_SCHEMA_VERSION = 1;

export const CONFIG_FILE_NAME = '.octocoderc';

export interface GitHubConfigOptions {
  apiUrl?: string;
}

export interface LocalConfigOptions {
  enabled?: boolean;

  enableClone?: boolean;

  /** Opt in to hash-guarded astRewrite file mutation; preview remains available. */
  enableAstRewriteApply?: boolean;

  allowedPaths?: string[];

  workspaceRoot?: string;
}

export interface ToolsConfigOptions {
  enabled?: string[] | null;
  disabled?: string[] | null;
}

export interface NetworkConfigOptions {
  timeout?: number;

  maxRetries?: number;
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

export type StorageMode = 'persistent' | 'memory';

export interface StorageConfigOptions {
  /** `memory` disables persistent caches, materialization, session files, and SQLite-backed extension state. */
  mode?: StorageMode;
}

/**
 * Extension-specific overrides. Keys here take precedence over the global equivalents
 * for the Pi extension runtime only; the Octocode CLI and MCP server continue to use
 * the top-level settings.
 */
export interface ExtensionConfigOptions {
  /**
   * Override `storage.mode` for the Pi extension (Awareness, SQLite state) without
   * changing the global CLI / MCP setting.
   *
   * Example: keep `storage.mode=memory` for the researcher and set
   * `extension.storage.mode=persistent` to enable Awareness in Pi.
   */
  storage?: StorageConfigOptions;
}

export interface OctocodeConfig {
  $schema?: string;

  version?: number;

  github?: GitHubConfigOptions;

  local?: LocalConfigOptions;

  tools?: ToolsConfigOptions;

  network?: NetworkConfigOptions;

  lsp?: LspConfigOptions;

  output?: OutputConfigOptions;

  storage?: StorageConfigOptions;

  /** Per-consumer overrides for the Pi extension runtime. */
  extension?: ExtensionConfigOptions;
}

export interface RequiredGitHubConfig {
  apiUrl: string;
}

export interface RequiredLocalConfig {
  enabled: boolean;
  enableClone: boolean;
  enableAstRewriteApply: boolean;
  allowedPaths: string[];
  workspaceRoot: string | undefined;
}

export interface RequiredToolsConfig {
  enabled: string[] | null;
  disabled: string[] | null;
}

export interface RequiredNetworkConfig {
  timeout: number;
  maxRetries: number;
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

export interface RequiredStorageConfig {
  mode: StorageMode;
}

export interface RequiredExtensionConfig {
  /** Resolved storage settings for the Pi extension (may differ from global storage). */
  storage: RequiredStorageConfig;
}

export interface ResolvedConfig {
  version: number;

  github: RequiredGitHubConfig;

  local: RequiredLocalConfig;

  tools: RequiredToolsConfig;

  network: RequiredNetworkConfig;

  lsp: RequiredLspConfig;

  output: RequiredOutputConfig;

  session: RequiredSessionConfig;

  storage: RequiredStorageConfig;

  /** Resolved Pi-extension-specific overrides. */
  extension: RequiredExtensionConfig;

  source: 'file' | 'defaults' | 'mixed' | 'env' | 'invalid';

  configPath?: string;
}

/**
 * Session / stats persistence options (env-var only — no .octocoderc equivalent).
 * Resolved entirely from OCTOCODE_ENABLE_STATS; default is off to avoid
 * unnecessary SSD writes on long-running agent sessions.
 */
export interface RequiredSessionConfig {
  /** Write stats.json on every flush. Stats are always tracked in memory. */
  enableStats: boolean;
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
