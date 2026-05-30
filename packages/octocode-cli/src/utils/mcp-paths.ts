import path from 'node:path';
import type {
  MCPClient,
  MCPClientInfo,
  MCPClientCategory,
} from '../types/index.js';
import { isWindows, isMac, HOME, getAppDataPath } from './platform.js';
import { dirExists, fileExists } from './fs.js';

function getAppSupportDir(): string {
  if (isWindows) {
    return getAppDataPath();
  }
  if (isMac) {
    return path.join(HOME, 'Library', 'Application Support');
  }

  return process.env.XDG_CONFIG_HOME || path.join(HOME, '.config');
}

function getVSCodeGlobalStoragePath(): string {
  const appSupport = getAppSupportDir();
  if (isWindows) {
    return path.join(appSupport, 'Code', 'User', 'globalStorage');
  }
  if (isMac) {
    return path.join(appSupport, 'Code', 'User', 'globalStorage');
  }

  return path.join(appSupport, 'Code', 'User', 'globalStorage');
}

export const MCP_CLIENTS: Record<MCPClient, MCPClientInfo> = {
  cursor: {
    id: 'cursor',
    name: 'Cursor',
    description: 'AI-first code editor',
    category: 'ide',
    url: 'https://cursor.sh',
    envVars: ['CURSOR_AGENT', 'CURSOR_TRACE_ID', 'CURSOR_SESSION_ID', 'CURSOR'],
  },
  'claude-desktop': {
    id: 'claude-desktop',
    name: 'Claude Desktop',
    description: "Anthropic's desktop app",
    category: 'desktop',
    url: 'https://claude.ai/download',
  },
  'claude-code': {
    id: 'claude-code',
    name: 'Claude Code',
    description: 'Claude CLI for terminal',
    category: 'cli',
    url: 'https://docs.anthropic.com/claude-code',
    envVars: ['CLAUDE_CODE'],
  },
  'vscode-cline': {
    id: 'vscode-cline',
    name: 'Cline (VS Code)',
    description: 'AI coding assistant extension',
    category: 'extension',
    url: 'https://marketplace.visualstudio.com/items?itemName=saoudrizwan.claude-dev',
    envVars: ['VSCODE_PID', 'TERM_PROGRAM'],
  },
  'vscode-roo': {
    id: 'vscode-roo',
    name: 'Roo-Cline (VS Code)',
    description: 'Roo AI coding extension',
    category: 'extension',
    envVars: ['VSCODE_PID'],
  },
  windsurf: {
    id: 'windsurf',
    name: 'Windsurf',
    description: 'Codeium AI IDE',
    category: 'ide',
    url: 'https://codeium.com/windsurf',
    envVars: ['WINDSURF_SESSION'],
  },
  trae: {
    id: 'trae',
    name: 'Trae',
    description: 'Adaptive AI IDE',
    category: 'ide',
    url: 'https://trae.ai',
  },
  antigravity: {
    id: 'antigravity',
    name: 'Antigravity',
    description: 'Gemini-powered AI IDE',
    category: 'ide',
  },
  'vscode-continue': {
    id: 'vscode-continue',
    name: 'Continue (VS Code)',
    description: 'Open-source AI assistant',
    category: 'extension',
    url: 'https://continue.dev',
    envVars: ['VSCODE_PID'],
  },
  zed: {
    id: 'zed',
    name: 'Zed',
    description: 'High-performance code editor',
    category: 'ide',
    url: 'https://zed.dev',
    envVars: ['ZED_TERM'],
  },
  opencode: {
    id: 'opencode',
    name: 'Opencode',
    description: 'AI coding agent CLI',
    category: 'cli',
    url: 'https://opencode.ai',
    envVars: ['OPENCODE'],
  },
  codex: {
    id: 'codex',
    name: 'Codex',
    description: 'OpenAI Codex CLI agent',
    category: 'cli',
    url: 'https://github.com/openai/codex',
    envVars: ['CODEX_HOME', 'CODEX_SANDBOX_TYPE'],
  },
  'gemini-cli': {
    id: 'gemini-cli',
    name: 'Gemini CLI',
    description: 'Google Gemini CLI',
    category: 'cli',
    url: 'https://github.com/google-gemini/gemini-cli',
    envVars: ['GEMINI_API_KEY'],
  },
  goose: {
    id: 'goose',
    name: 'Goose',
    description: 'Block AI coding agent',
    category: 'desktop',
    url: 'https://block.github.io/goose',
    envVars: ['GOOSE_MODE'],
  },
  kiro: {
    id: 'kiro',
    name: 'Kiro',
    description: 'AWS AI IDE',
    category: 'ide',
    url: 'https://kiro.dev',
  },
  custom: {
    id: 'custom',
    name: 'Custom Path',
    description: 'Specify your own MCP config path',
    category: 'cli',
  },
};

export type DetectableMCPClient = Exclude<MCPClient, 'custom'>;

export const DETECTABLE_MCP_CLIENTS = Object.keys(MCP_CLIENTS).filter(
  (client): client is DetectableMCPClient => client !== 'custom'
);

export function getMCPConfigPath(
  client: MCPClient,
  customPath?: string
): string {
  if (client === 'custom' && customPath) {
    return customPath;
  }

  const appSupport = getAppSupportDir();
  const vsCodeStorage = getVSCodeGlobalStoragePath();

  switch (client) {
    case 'cursor':
      if (isWindows) {
        return path.join(getAppDataPath(), 'Cursor', 'mcp.json');
      }
      return path.join(HOME, '.cursor', 'mcp.json');

    case 'claude-desktop':
      if (isWindows) {
        return path.join(appSupport, 'Claude', 'claude_desktop_config.json');
      }
      if (isMac) {
        return path.join(appSupport, 'Claude', 'claude_desktop_config.json');
      }

      return path.join(appSupport, 'claude', 'claude_desktop_config.json');

    case 'claude-code':
      return path.join(HOME, '.claude.json');

    case 'vscode-cline':
      return path.join(
        vsCodeStorage,
        'saoudrizwan.claude-dev',
        'settings',
        'cline_mcp_settings.json'
      );

    case 'vscode-roo':
      return path.join(
        vsCodeStorage,
        'rooveterinaryinc.roo-cline',
        'settings',
        'mcp_settings.json'
      );

    case 'windsurf':
      return path.join(HOME, '.codeium', 'windsurf', 'mcp_config.json');

    case 'trae':
      if (isWindows) {
        return path.join(getAppDataPath(), 'Trae', 'mcp.json');
      }
      if (isMac) {
        return path.join(appSupport, 'Trae', 'mcp.json');
      }

      return path.join(appSupport, 'Trae', 'mcp.json');

    case 'antigravity':
      return path.join(HOME, '.gemini', 'antigravity', 'mcp_config.json');

    case 'vscode-continue':
      return path.join(HOME, '.continue', 'config.json');

    case 'zed':
      if (isWindows) {
        return path.join(getAppDataPath(), 'Zed', 'settings.json');
      }
      if (isMac) {
        return path.join(HOME, '.config', 'zed', 'settings.json');
      }

      return path.join(appSupport, 'zed', 'settings.json');

    case 'opencode':
      if (isWindows) {
        return path.join(getAppDataPath(), 'opencode', 'config.json');
      }

      return path.join(appSupport, 'opencode', 'config.json');

    case 'codex':
      return path.join(HOME, '.codex', 'config.toml');

    case 'gemini-cli':
      return path.join(HOME, '.gemini', 'settings.json');

    case 'goose':
      if (isWindows) {
        return path.join(getAppDataPath(), 'goose', 'config.yaml');
      }
      if (isMac) {
        return path.join(appSupport, 'goose', 'config.yaml');
      }

      return path.join(appSupport, 'goose', 'config.yaml');

    case 'kiro':
      if (isWindows) {
        return path.join(getAppDataPath(), 'Kiro', 'mcp.json');
      }
      return path.join(HOME, '.kiro', 'mcp.json');

    case 'custom':
      throw new Error('Custom path requires customPath parameter');

    default:
      throw new Error(`Unknown MCP client: ${client}`);
  }
}

export function clientConfigExists(
  client: MCPClient,
  customPath?: string
): boolean {
  try {
    const configPath = getMCPConfigPath(client, customPath);
    const configDir = path.dirname(configPath);
    return dirExists(configDir);
  } catch {
    return false;
  }
}

export function configFileExists(
  client: MCPClient,
  customPath?: string
): boolean {
  try {
    const configPath = getMCPConfigPath(client, customPath);
    return fileExists(configPath);
  } catch {
    return false;
  }
}

export function detectCurrentClient(): MCPClient | null {
  const env = process.env;

  if (
    env.CURSOR_AGENT ||
    env.CURSOR_TRACE_ID ||
    env.CURSOR_SESSION_ID ||
    env.CURSOR
  ) {
    return 'cursor';
  }

  if (env.WINDSURF_SESSION) {
    return 'windsurf';
  }

  if (env.CLAUDE_CODE) {
    return 'claude-code';
  }

  if (env.ZED_TERM || env.ZED) {
    return 'zed';
  }

  if (env.OPENCODE) {
    return 'opencode';
  }

  if (env.CODEX_HOME || env.CODEX_SANDBOX_TYPE) {
    return 'codex';
  }

  if (env.GEMINI_API_KEY) {
    return 'gemini-cli';
  }

  if (env.GOOSE_MODE) {
    return 'goose';
  }

  if (env.VSCODE_PID || env.TERM_PROGRAM === 'vscode') {
    if (env.ROO_CLINE || env.ROO) {
      return 'vscode-roo';
    }
    if (env.CONTINUE_GLOBAL_DIR) {
      return 'vscode-continue';
    }
    return 'vscode-cline';
  }

  return null;
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

export function getClientsByCategory(): Record<
  MCPClientCategory,
  MCPClientInfo[]
> {
  const grouped: Record<MCPClientCategory, MCPClientInfo[]> = {
    ide: [],
    desktop: [],
    extension: [],
    cli: [],
  };

  for (const client of Object.values(MCP_CLIENTS)) {
    if (client.id !== 'custom') {
      grouped[client.category].push(client);
    }
  }

  return grouped;
}
