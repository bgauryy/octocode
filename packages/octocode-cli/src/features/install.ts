import type {
  IDE,
  InstallMethod,
  MCPConfig,
  MCPServer,
  MCPClient,
} from '../types/index.js';
import {
  getMCPConfigPath,
  readMCPConfig,
  writeMCPConfig,
  mergeOctocodeConfig,
  isOctocodeConfigured,
  clientConfigExists,
  getOctocodeServerConfig,
  getOctocodeServerConfigWindows,
  getConfiguredMethod,
} from '../utils/mcp-config.js';
import { DETECTABLE_MCP_CLIENTS } from '../utils/mcp-paths.js';
import { fileExists } from '../utils/fs.js';
import { isWindows } from '../utils/platform.js';

function ideToMCPClient(ide: string): MCPClient {
  if (ide === 'claude') {
    return 'claude-desktop';
  }

  return ide as MCPClient;
}

interface InstallOptions {
  ide: IDE;
  method: InstallMethod;
  force?: boolean;
}

export interface InstallResult {
  success: boolean;
  configPath: string;
  backupPath?: string;
  alreadyInstalled?: boolean;
  error?: string;
}

interface InstallPreview {
  ide: IDE;
  method: InstallMethod;
  configPath: string;
  serverConfig: MCPServer;
  action: 'create' | 'add' | 'override';
  existingMethod?: InstallMethod | null;
}

export function detectAvailableIDEs(): IDE[] {
  const available: IDE[] = [];

  if (clientConfigExists('cursor')) {
    available.push('cursor');
  }
  if (clientConfigExists('claude-desktop')) {
    available.push('claude');
  }

  return available;
}

export function checkExistingInstallation(ide: IDE): {
  installed: boolean;
  configPath: string;
  configExists: boolean;
} {
  const configPath = getMCPConfigPath(ideToMCPClient(ide));
  const configExists = fileExists(configPath);

  if (!configExists) {
    return { installed: false, configPath, configExists: false };
  }

  const config = readMCPConfig(configPath);
  if (!config) {
    return { installed: false, configPath, configExists: true };
  }

  return {
    installed: isOctocodeConfigured(config),
    configPath,
    configExists: true,
  };
}

export function installOctocode(options: InstallOptions): InstallResult {
  const { ide, method, force = false } = options;
  const configPath = getMCPConfigPath(ideToMCPClient(ide));

  let config: MCPConfig = readMCPConfig(configPath) || { mcpServers: {} };

  if (isOctocodeConfigured(config) && !force) {
    return {
      success: false,
      configPath,
      alreadyInstalled: true,
      error: 'Octocode is already configured. Use --force to overwrite.',
    };
  }

  config = mergeOctocodeConfig(config, method);

  const writeResult = writeMCPConfig(configPath, config);

  if (!writeResult.success) {
    return {
      success: false,
      configPath,
      error: writeResult.error || 'Failed to write config',
    };
  }

  return {
    success: true,
    configPath,
    backupPath: writeResult.backupPath,
  };
}

export function installOctocodeMultiple(
  ides: IDE[],
  method: InstallMethod,
  force: boolean = false
): Map<IDE, InstallResult> {
  const results = new Map<IDE, InstallResult>();

  for (const ide of ides) {
    results.set(ide, installOctocode({ ide, method, force }));
  }

  return results;
}

export function getInstallPreview(
  ide: IDE,
  method: InstallMethod
): InstallPreview {
  const configPath = getMCPConfigPath(ideToMCPClient(ide));
  const existing = checkExistingInstallation(ide);
  const existingConfig = readMCPConfig(configPath);
  const serverConfig = isWindows
    ? getOctocodeServerConfigWindows(method)
    : getOctocodeServerConfig(method);

  let action: InstallPreview['action'] = 'create';
  if (existing.installed) {
    action = 'override';
  } else if (existing.configExists) {
    action = 'add';
  }

  return {
    ide,
    method,
    configPath,
    serverConfig,
    action,
    existingMethod: existingConfig ? getConfiguredMethod(existingConfig) : null,
  };
}

import type { OctocodeEnvOptions } from '../utils/mcp-config.js';

interface ClientInstallOptions {
  client: MCPClient;
  method: InstallMethod;
  customPath?: string;
  force?: boolean;
  envOptions?: OctocodeEnvOptions;
}

interface ClientInstallPreview {
  client: MCPClient;
  method: InstallMethod;
  configPath: string;
  serverConfig: MCPServer;
  action: 'create' | 'add' | 'override';
  existingMethod?: InstallMethod | null;
}

export function checkExistingClientInstallation(
  client: MCPClient,
  customPath?: string
): {
  installed: boolean;
  configPath: string;
  configExists: boolean;
} {
  const configPath =
    client === 'custom' && customPath
      ? customPath
      : getMCPConfigPath(client, customPath);
  const configExists = fileExists(configPath);

  if (!configExists) {
    return { installed: false, configPath, configExists: false };
  }

  const config = readMCPConfig(configPath);
  if (!config) {
    return { installed: false, configPath, configExists: true };
  }

  return {
    installed: isOctocodeConfigured(config),
    configPath,
    configExists: true,
  };
}

export function installOctocodeForClient(
  options: ClientInstallOptions
): InstallResult {
  const { client, method, customPath, force = false, envOptions } = options;
  const configPath =
    client === 'custom' && customPath
      ? customPath
      : getMCPConfigPath(client, customPath);

  let config: MCPConfig = readMCPConfig(configPath) || { mcpServers: {} };

  if (isOctocodeConfigured(config) && !force) {
    return {
      success: false,
      configPath,
      alreadyInstalled: true,
      error: 'Octocode is already configured. Use --force to overwrite.',
    };
  }

  config = mergeOctocodeConfig(config, method, envOptions);

  const writeResult = writeMCPConfig(configPath, config);

  if (!writeResult.success) {
    return {
      success: false,
      configPath,
      error: writeResult.error || 'Failed to write config',
    };
  }

  return {
    success: true,
    configPath,
    backupPath: writeResult.backupPath,
  };
}

export function getInstallPreviewForClient(
  client: MCPClient,
  method: InstallMethod,
  customPath?: string,
  envOptions?: OctocodeEnvOptions
): ClientInstallPreview {
  const configPath =
    client === 'custom' && customPath
      ? customPath
      : getMCPConfigPath(client, customPath);
  const existing = checkExistingClientInstallation(client, customPath);
  const existingConfig = readMCPConfig(configPath);
  const serverConfig = isWindows
    ? getOctocodeServerConfigWindows(method, envOptions)
    : getOctocodeServerConfig(method, envOptions);

  let action: ClientInstallPreview['action'] = 'create';
  if (existing.installed) {
    action = 'override';
  } else if (existing.configExists) {
    action = 'add';
  }

  return {
    client,
    method,
    configPath,
    serverConfig,
    action,
    existingMethod: existingConfig ? getConfiguredMethod(existingConfig) : null,
  };
}

export function detectAvailableClients(): MCPClient[] {
  const available: MCPClient[] = [];

  for (const client of DETECTABLE_MCP_CLIENTS) {
    if (clientConfigExists(client)) {
      available.push(client);
    }
  }

  return available;
}
