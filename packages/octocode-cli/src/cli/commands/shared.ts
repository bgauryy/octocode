import type { MCPClient } from '../../types/index.js';
import type { TokenSource } from '../../types/index.js';
import { c, bold, dim } from '../../utils/colors.js';
import { IDE_INFO, CLIENT_INFO } from '../../ui/constants.js';
import { getAuthStatus, getStoragePath } from '../../features/github-oauth.js';
import { DETECTABLE_MCP_CLIENTS } from '../../utils/mcp-paths.js';
export {
  formatSkillInstallTargets,
  normalizeSkillTarget,
  type SkillInstallMode,
  type SkillInstallStrategy,
  type SkillInstallTarget,
} from '../../utils/skills.js';

export type GetTokenSource = 'octocode' | 'gh' | 'auto';

export const MCP_CLIENT_IDS: Record<string, MCPClient> = {
  'claude-desktop': 'claude-desktop',
  'claude-code': 'claude-code',
  cursor: 'cursor',
  windsurf: 'windsurf',
  trae: 'trae',
  antigravity: 'antigravity',
  zed: 'zed',
  'vscode-cline': 'vscode-cline',
  'vscode-roo': 'vscode-roo',
  'vscode-continue': 'vscode-continue',
  opencode: 'opencode',
  codex: 'codex',
  'gemini-cli': 'gemini-cli',
  goose: 'goose',
  kiro: 'kiro',
  custom: 'custom',
};

export function normalizeMCPClient(value: string): MCPClient | null {
  return MCP_CLIENT_IDS[value.trim().toLowerCase()] ?? null;
}

export function formatSupportedMCPClients(): string {
  return DETECTABLE_MCP_CLIENTS.join(', ');
}

export function getIDEDisplayName(ide: string): string {
  if (ide in CLIENT_INFO) {
    return CLIENT_INFO[ide as keyof typeof CLIENT_INFO].name;
  }
  if (ide in IDE_INFO) {
    return IDE_INFO[ide as keyof typeof IDE_INFO].name;
  }
  return ide.charAt(0).toUpperCase() + ide.slice(1);
}

export function maskToken(token: string): string {
  if (token.length <= 8) return '****';
  return token.slice(0, 4) + '****' + token.slice(-4);
}

export function safeTokenOutput(token: string): string {
  if (!process.stdout.isTTY) return token;
  return maskToken(token);
}

export function printLoginHint(): void {
  console.log(`  ${dim('To login:')}`);
  console.log(`    ${c('cyan', '→')} ${c('yellow', 'octocode login')}`);
  console.log(`    ${dim('or')}`);
  console.log(`    ${c('cyan', '→')} ${c('yellow', 'gh auth login')}`);
}

export function formatAuthStatusAsJson(
  hostname: string
): Record<string, unknown> {
  const status = getAuthStatus(hostname);
  return {
    authenticated: status.authenticated,
    username: status.username || null,
    hostname: status.hostname,
    tokenSource: status.tokenSource || null,
    tokenExpired: Boolean(status.tokenExpired),
  };
}

export function printAuthStatus(hostname: string = 'github.com'): void {
  console.log();
  console.log(`  ${bold('🔐 GitHub Authentication')}`);
  console.log();

  const status = getAuthStatus(hostname);

  if (status.authenticated) {
    console.log(
      `  ${c('green', '✓')} Authenticated as ${c('cyan', status.username || 'unknown')}`
    );
    console.log(`  ${dim('Logged in:')} yes`);
    if (status.tokenExpired) {
      console.log(
        `  ${c('yellow', '⚠')} Token has expired - please login again`
      );
    }
    console.log(`  ${dim('Host:')} ${status.hostname}`);
    console.log(
      `  ${dim('Source:')} ${formatTokenSource(status.tokenSource || 'none', status.envTokenSource)}`
    );
  } else {
    console.log(
      `  ${c('yellow', '⚠')} ${c('yellow', 'Not authenticated')} ${dim('(Not logged in)')}`
    );
    console.log();
    console.log(`  ${bold('To authenticate:')}`);
    printLoginHint();
  }

  console.log();
  console.log(`  ${dim('Credentials stored in:')} ${getStoragePath()}`);
  console.log();
}

export function printNodeDoctorHintCLI(): void {
  console.log(
    `  ${dim('For deeper diagnostics:')} ${c('cyan', 'npx node-doctor')}`
  );
  console.log();
}

export function formatTokenSource(
  source: TokenSource,
  envSource?: string
): string {
  switch (source) {
    case 'octocode':
      return c('cyan', 'octocode');
    case 'gh-cli':
      return c('magenta', 'gh cli');
    case 'env':
      if (envSource) {
        const varName = envSource.replace('env:', '');
        return c('green', varName);
      }
      return c('green', 'environment variable');
    default:
      return dim('none');
  }
}

export function parseMCPEnv(envArg?: string): {
  values: Record<string, string>;
  error?: string;
} {
  if (!envArg || envArg.trim().length === 0) {
    return { values: {} };
  }
  const entries = envArg
    .split(',')
    .map(part => part.trim())
    .filter(Boolean);
  const values: Record<string, string> = {};
  const namePattern = /^[A-Za-z_][A-Za-z0-9_]*$/;
  for (const entry of entries) {
    const eqIdx = entry.indexOf('=');
    if (eqIdx <= 0) {
      return {
        values: {},
        error: `Invalid --env pair: "${entry}" (expected KEY=VALUE)`,
      };
    }
    const key = entry.slice(0, eqIdx).trim();
    const val = entry.slice(eqIdx + 1);
    if (!namePattern.test(key)) {
      return {
        values: {},
        error: `Invalid env var name: "${key}"`,
      };
    }
    values[key] = val;
  }
  return { values };
}
