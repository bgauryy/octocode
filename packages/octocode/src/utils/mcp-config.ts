import type {
  MCPConfig,
  MCPServer,
  InstallMethod,
  MCPClient,
} from '../types/index.js';
import { isWindows } from './platform.js';

export {
  getMCPConfigPath,
  clientConfigExists,
  MCP_CLIENTS,
} from './mcp-paths.js';
export { readMCPConfig, writeMCPConfig } from './mcp-io.js';

export interface MCPRegistryEntry {
  id: string;
  name: string;
  description: string;
  category: string;
  repository: string;
  website?: string;
  stars?: number;
  installationType: 'npm' | 'npx' | 'pip' | 'docker' | 'source';
  npmPackage?: string;
  pipPackage?: string;
  dockerImage?: string;
  installConfig: {
    command: string;
    args: string[];
    env?: Record<string, string>;
  };
  requiredEnvVars?: Array<{
    name: string;
    description: string;
    example?: string;
  }>;
  official?: boolean;
  tags?: string[];
}

export interface OctocodeEnvOptions {
  enableLocal?: boolean;
  githubToken?: string;
}

export function getOctocodeServerConfig(
  method: InstallMethod,
  envOptions?: OctocodeEnvOptions
): MCPServer {
  let config: MCPServer;

  switch (method) {
    case 'npx':
      config = {
        command: 'npx',
        args: ['-y', '@octocodeai/mcp@latest'],
      };
      break;

    default:
      throw new Error(`Unknown install method: ${method}`);
  }

  if (envOptions) {
    const env: Record<string, string> = {};

    if (envOptions.enableLocal !== undefined) {
      env.ENABLE_LOCAL = envOptions.enableLocal ? 'true' : 'false';
    }

    if (envOptions.githubToken) {
      env.GITHUB_TOKEN = envOptions.githubToken;
    }

    if (Object.keys(env).length > 0) {
      config.env = env;
    }
  }

  return config;
}

export function getOctocodeServerConfigWindows(
  method: InstallMethod,
  envOptions?: OctocodeEnvOptions
): MCPServer {
  return getOctocodeServerConfig(method, envOptions);
}

export function mergeOctocodeConfig(
  config: MCPConfig,
  method: InstallMethod,
  envOptions?: OctocodeEnvOptions
): MCPConfig {
  const serverConfig = isWindows
    ? getOctocodeServerConfigWindows(method, envOptions)
    : getOctocodeServerConfig(method, envOptions);

  return {
    ...config,
    mcpServers: {
      ...config.mcpServers,
      octocode: serverConfig,
    },
  };
}

export function isOctocodeConfigured(config: MCPConfig): boolean {
  return Boolean(config.mcpServers?.octocode);
}

export function getConfiguredMethod(config: MCPConfig): InstallMethod | null {
  const octocode = config.mcpServers?.octocode;
  if (!octocode) return null;

  if (octocode.command === 'npx') return 'npx';
  return null;
}

import { getMCPConfigPath, configFileExists } from './mcp-paths.js';
import { readMCPConfig } from './mcp-io.js';

export interface ClientInstallStatus {
  client: MCPClient;
  configExists: boolean;
  octocodeInstalled: boolean;
  method: InstallMethod | null;
  configPath: string;
}

export function getClientInstallStatus(
  client: MCPClient,
  customPath?: string
): ClientInstallStatus {
  const configPath = getMCPConfigPath(client, customPath);
  const configExists = configFileExists(client, customPath);

  let octocodeInstalled = false;
  let method: InstallMethod | null = null;

  if (configExists) {
    const config = readMCPConfig(configPath);
    if (config) {
      octocodeInstalled = isOctocodeConfigured(config);
      method = getConfiguredMethod(config);
    }
  }

  return {
    client,
    configExists,
    octocodeInstalled,
    method,
    configPath,
  };
}

export function getAllClientInstallStatus(): ClientInstallStatus[] {
  const clients: MCPClient[] = [
    'cursor',
    'claude-desktop',
    'claude-code',
    'opencode',
    'vscode-cline',
    'vscode-roo',
    'vscode-continue',
    'windsurf',
    'trae',
    'antigravity',
    'zed',
  ];

  return clients.map(client => getClientInstallStatus(client));
}

export function findInstalledClients(): ClientInstallStatus[] {
  return getAllClientInstallStatus().filter(status => status.octocodeInstalled);
}

export function registryEntryToServerConfig(
  entry: MCPRegistryEntry,
  envValues?: Record<string, string>
): MCPServer {
  const config: MCPServer = {
    command: entry.installConfig.command,
    args: [...entry.installConfig.args],
  };

  if (envValues && config.args) {
    config.args = config.args.map(arg => {
      return arg.replace(/\$\{(\w+)\}/g, (_, varName) => {
        return envValues[varName] || `\${${varName}}`;
      });
    });
  }

  const env: Record<string, string> = {};

  if (entry.installConfig.env) {
    Object.assign(env, entry.installConfig.env);
  }

  if (envValues) {
    for (const [key, value] of Object.entries(envValues)) {
      if (value) {
        env[key] = value;
      }
    }
  }

  if (Object.keys(env).length > 0) {
    config.env = env;
  }

  return config;
}

export function mergeExternalMCPConfig(
  config: MCPConfig,
  entry: MCPRegistryEntry,
  envValues?: Record<string, string>
): MCPConfig {
  const serverConfig = registryEntryToServerConfig(entry, envValues);

  return {
    ...config,
    mcpServers: {
      ...config.mcpServers,
      [entry.id]: serverConfig,
    },
  };
}

export function isExternalMCPConfigured(
  config: MCPConfig,
  entryId: string
): boolean {
  return Boolean(config.mcpServers?.[entryId]);
}

export function removeExternalMCPConfig(
  config: MCPConfig,
  entryId: string
): MCPConfig {
  if (!config.mcpServers?.[entryId]) {
    return config;
  }

  const remainingServers = Object.fromEntries(
    Object.entries(config.mcpServers).filter(([key]) => key !== entryId)
  );

  return {
    ...config,
    mcpServers: remainingServers,
  };
}

export function getInstalledExternalMCPs(
  config: MCPConfig,
  registry: MCPRegistryEntry[]
): MCPRegistryEntry[] {
  if (!config.mcpServers) return [];

  const installedIds = new Set(Object.keys(config.mcpServers));
  return registry.filter(entry => installedIds.has(entry.id));
}

export function validateRequiredEnvVars(
  entry: MCPRegistryEntry,
  envValues: Record<string, string>
): { valid: boolean; missing: string[] } {
  const missing: string[] = [];

  if (entry.requiredEnvVars) {
    for (const envVar of entry.requiredEnvVars) {
      if (!envValues[envVar.name]) {
        missing.push(envVar.name);
      }
    }
  }

  return {
    valid: missing.length === 0,
    missing,
  };
}
