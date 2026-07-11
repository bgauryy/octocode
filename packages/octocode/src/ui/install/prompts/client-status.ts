import type { MCPClient } from '../../../types/index.js';
import { c } from '../../../utils/colors.js';
import { clientConfigExists } from '../../../utils/mcp-paths.js';
import {
  getClientInstallStatus,
  type ClientInstallStatus,
} from '../../../utils/mcp-config.js';
import type { ClientWithStatus } from './types.js';

export function getClientStatusIndicator(status: ClientInstallStatus): string {
  if (status.octocodeInstalled) {
    return c('green', '✅ installed');
  }
  if (status.configExists) {
    return c('blue', '○ Ready');
  }
  if (clientConfigExists(status.client)) {
    return c('dim', '○ Available');
  }
  return c('dim', '○ Not found');
}

export function getAllClientsWithStatus(): ClientWithStatus[] {
  const clientOrder: MCPClient[] = [
    'cursor',
    'claude-desktop',
    'claude-code',
    'opencode',
    'codex',
    'gemini-cli',
    'windsurf',
    'trae',
    'antigravity',
    'goose',
    'kiro',
    'zed',
    'vscode-cline',
    'vscode-roo',
    'vscode-continue',
  ];

  return clientOrder.map(clientId => ({
    clientId,
    status: getClientInstallStatus(clientId),
    isAvailable: clientConfigExists(clientId),
  }));
}
