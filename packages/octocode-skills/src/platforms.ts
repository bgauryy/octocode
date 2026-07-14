/**
 * Platform → skills directory mapping.
 *
 * Each platform has a dedicated directory where agents look for skills.
 * Installation creates a symlink from that directory to the canonical
 * home at ~/.octocode/skills/<name>.
 */

import os from 'node:os';
import path from 'node:path';

const HOME = os.homedir();
const isWindows = os.platform() === 'win32';

export type Platform =
  | 'pi'
  | 'cursor'
  | 'claude'
  | 'claude-desktop'
  | 'codex'
  | 'opencode'
  | 'copilot'
  | 'gemini'
  | 'common'
  | 'agents';

export const VALID_PLATFORMS: readonly string[] = [
  'pi',
  'cursor',
  'claude',
  'claude-desktop',
  'claude-code',
  'codex',
  'opencode',
  'copilot',
  'gemini',
  'common',
  'agents',
  'all',
];

function getAppData(): string {
  return process.env['APPDATA'] ?? path.join(HOME, 'AppData', 'Roaming');
}

/**
 * Resolve the skills directory for a given platform.
 * This is WHERE the symlink is placed (not the skill source).
 */
export function getPlatformSkillsDir(platform: Platform): string {
  if (isWindows) {
    const appData = getAppData();
    switch (platform) {
      case 'claude':
      case 'claude-desktop':
        return path.join(appData, 'Claude Desktop', 'skills');
      case 'cursor':
        return path.join(HOME, '.cursor', 'skills');
      case 'codex':
        return path.join(HOME, '.agents', 'skills');
      case 'opencode':
        return path.join(appData, 'opencode', 'skills');
      case 'pi':
        return path.join(HOME, '.pi', 'agent', 'skills');
      case 'copilot':
        return path.join(HOME, '.copilot', 'skills');
      case 'gemini':
        return path.join(HOME, '.gemini', 'skills');
      case 'common':
      case 'agents':
        return path.join(HOME, '.agents', 'skills');
    }
  }

  switch (platform) {
    case 'claude':
    case 'claude-desktop':
      return path.join(HOME, '.claude-desktop', 'skills');
    case 'cursor':
      return path.join(HOME, '.cursor', 'skills');
    case 'codex':
      return path.join(HOME, '.agents', 'skills');
    case 'opencode':
      return path.join(HOME, '.config', 'opencode', 'skills');
    case 'pi':
      return path.join(HOME, '.pi', 'agent', 'skills');
    case 'copilot':
      return path.join(HOME, '.copilot', 'skills');
    case 'gemini':
      return path.join(HOME, '.gemini', 'skills');
    case 'common':
    case 'agents':
      return path.join(HOME, '.agents', 'skills');
  }
}

const ALL_PLATFORMS: Platform[] = [
  'pi', 'cursor', 'claude', 'codex', 'opencode', 'copilot', 'gemini', 'common',
];

const PLATFORM_ALIASES: Record<string, Platform | 'all'> = {
  pi: 'pi',
  'pi-agent': 'pi',
  cursor: 'cursor',
  claude: 'claude',
  'claude-code': 'claude',
  'claude-desktop': 'claude-desktop',
  codex: 'codex',
  opencode: 'opencode',
  'open-code': 'opencode',
  copilot: 'copilot',
  'github-copilot': 'copilot',
  'vscode-copilot': 'copilot',
  gemini: 'gemini',
  'gemini-cli': 'gemini',
  common: 'common',
  agents: 'agents',
  agent: 'agents',
  all: 'all',
};

export function parsePlatforms(raw: string): { platforms: Platform[]; error?: string } {
  const parts = raw
    .split(',')
    .map((p) => p.trim().toLowerCase())
    .filter(Boolean);

  const platforms: Platform[] = [];

  for (const part of parts) {
    const resolved = PLATFORM_ALIASES[part];
    if (!resolved) {
      return {
        platforms: [],
        error: `Unknown platform: "${part}". Valid: ${VALID_PLATFORMS.join(', ')}`,
      };
    }
    if (resolved === 'all') {
      return { platforms: ALL_PLATFORMS };
    }
    if (!platforms.includes(resolved)) {
      platforms.push(resolved);
    }
  }

  return { platforms };
}
