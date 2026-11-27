import * as os from 'os';
import * as path from 'path';
import type { ConfigKey } from './mcpConfig';

export type McpClientDef = {
  name: string;
  getConfigPath: () => string;
  configKey: ConfigKey;
};

export type EditorInfo = {
  configKey: ConfigKey;
  name: string;
  mcpConfigPath: string | null;
};

type PlatformOptions = {
  appData?: string;
  homeDir?: string;
  platform?: NodeJS.Platform;
};

function resolvePlatformOptions(
  options: PlatformOptions = {}
): Required<PlatformOptions> {
  return {
    appData: options.appData ?? process.env.APPDATA ?? '',
    homeDir: options.homeDir ?? os.homedir(),
    platform: options.platform ?? process.platform,
  };
}

export function getPlatformConfigBase(options: PlatformOptions = {}): string {
  const { appData, homeDir, platform } = resolvePlatformOptions(options);

  if (platform === 'darwin') {
    return path.join(homeDir, 'Library', 'Application Support');
  }

  if (platform === 'win32') {
    return appData || path.join(homeDir, 'AppData', 'Roaming');
  }

  return path.join(homeDir, '.config');
}

export function createMcpClients(
  options: PlatformOptions = {}
): Record<string, McpClientDef> {
  return {
    cline: {
      name: 'Cline',
      getConfigPath: () =>
        path.join(
          getPlatformConfigBase(options),
          'Code',
          'User',
          'globalStorage',
          'saoudrizwan.claude-dev',
          'settings',
          'cline_mcp_settings.json'
        ),
      configKey: 'mcpServers',
    },
    rooCode: {
      name: 'Roo Code',
      getConfigPath: () =>
        path.join(
          getPlatformConfigBase(options),
          'Code',
          'User',
          'globalStorage',
          'rooveterinaryinc.roo-cline',
          'settings',
          'mcp_settings.json'
        ),
      configKey: 'mcpServers',
    },
    trae: {
      name: 'Trae',
      getConfigPath: () =>
        path.join(getPlatformConfigBase(options), 'Trae', 'mcp.json'),
      configKey: 'mcpServers',
    },
  };
}

export function detectEditorInfo(
  appName: string,
  options: PlatformOptions = {}
): EditorInfo {
  try {
    const normalizedAppName = appName.toLowerCase();
    const { homeDir, platform } = resolvePlatformOptions(options);

    if (normalizedAppName.includes('cursor')) {
      const cursorConfigPath =
        platform === 'win32'
          ? path.join(getPlatformConfigBase(options), 'Cursor', 'mcp.json')
          : path.join(homeDir, '.cursor', 'mcp.json');
      return {
        name: 'Cursor',
        configKey: 'mcpServers',
        mcpConfigPath: cursorConfigPath,
      };
    }

    if (normalizedAppName.includes('windsurf')) {
      return {
        name: 'Windsurf',
        configKey: 'mcpServers',
        mcpConfigPath: path.join(
          homeDir,
          '.codeium',
          'windsurf',
          'mcp_config.json'
        ),
      };
    }

    if (normalizedAppName.includes('antigravity')) {
      return {
        name: 'Antigravity',
        configKey: 'mcpServers',
        mcpConfigPath: path.join(
          homeDir,
          '.gemini',
          'antigravity',
          'mcp_config.json'
        ),
      };
    }

    if (normalizedAppName.includes('trae')) {
      return {
        name: 'Trae',
        configKey: 'mcpServers',
        mcpConfigPath: path.join(
          getPlatformConfigBase(options),
          'Trae',
          'mcp.json'
        ),
      };
    }

    return {
      name: 'VS Code',
      configKey: 'servers',
      mcpConfigPath: path.join(
        getPlatformConfigBase(options),
        normalizedAppName.includes('insiders') ? 'Code - Insiders' : 'Code',
        'User',
        'mcp.json'
      ),
    };
  } catch {
    return {
      name: 'VS Code',
      configKey: 'servers',
      mcpConfigPath: null,
    };
  }
}
