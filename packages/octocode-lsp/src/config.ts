import { promises as fs } from 'fs';
import * as path from 'path';
import { createRequire } from 'module';
import { getConfigSync, getOctocodeDir } from 'octocode-shared';
import type {
  LanguageServerConfig,
  UserLanguageServerConfig,
} from './types.js';
import { validateLSPServerPath } from './validation.js';
import { LSPConfigFileSchema } from './schemas.js';

export { LANGUAGE_SERVER_COMMANDS } from './lspRegistry.js';
import { LANGUAGE_SERVER_COMMANDS } from './lspRegistry.js';

export const TYPESCRIPT_LSP_PROVIDERS = [
  'typescript-language-server',
  'vtsls',
  'tsgo',
] as const;

export type TypeScriptLspProvider = (typeof TYPESCRIPT_LSP_PROVIDERS)[number];

type PackagedLanguageServer = {
  command: string;
  args: string[];
  packageName?: string;
  binName?: string;
};

const TYPESCRIPT_LSP_PROVIDER_ENV = 'OCTOCODE_TS_LSP_PROVIDER';

const TYPESCRIPT_LSP_PROVIDER_CONFIG: Record<
  TypeScriptLspProvider,
  PackagedLanguageServer
> = {
  'typescript-language-server': {
    command: 'typescript-language-server',
    args: ['--stdio'],
    packageName: 'typescript-language-server',
    binName: 'typescript-language-server',
  },
  vtsls: {
    command: 'vtsls',
    args: ['--stdio'],
    packageName: '@vtsls/language-server',
    binName: 'vtsls',
  },
  tsgo: {
    command: 'tsgo',
    args: ['lsp', '--stdio'],
    packageName: '@typescript/native-preview',
    binName: 'tsgo',
  },
};

const require = createRequire(import.meta.url);
const DANGEROUS_SHELL_COMMANDS = new Set([
  'sh',
  'bash',
  'zsh',
  'fish',
  'dash',
  'ksh',
  'cmd',
  'cmd.exe',
  'powershell',
  'powershell.exe',
  'pwsh',
  'pwsh.exe',
]);

function isSafeUserLspCommand(command: string): boolean {
  const normalized = path.basename(command).toLowerCase();
  return !DANGEROUS_SHELL_COMMANDS.has(normalized);
}

function sanitizeUserLanguageServers(
  config: Record<string, UserLanguageServerConfig>
): Record<string, UserLanguageServerConfig> {
  const sanitized: Record<string, UserLanguageServerConfig> = {};

  for (const [extension, server] of Object.entries(config)) {
    if (!isSafeUserLspCommand(server.command)) {
      continue;
    }
    sanitized[extension] = server;
  }

  return sanitized;
}

export async function loadUserConfig(
  workspaceRoot?: string
): Promise<Record<string, UserLanguageServerConfig>> {
  const configPaths: string[] = [];

  const lspConfigPath =
    process.env.OCTOCODE_LSP_CONFIG ||
    (() => {
      try {
        return getConfigSync().lsp.configPath;
      } catch {
        return undefined;
      }
    })();
  if (lspConfigPath) {
    configPaths.push(lspConfigPath);
  }

  if (workspaceRoot) {
    configPaths.push(path.join(workspaceRoot, '.octocode', 'lsp-servers.json'));
  }

  configPaths.push(path.join(getOctocodeDir(), 'lsp-servers.json'));

  for (const configPath of configPaths) {
    try {
      const content = await fs.readFile(configPath, 'utf-8');
      const raw = JSON.parse(content);
      const validation = LSPConfigFileSchema.safeParse(raw);
      if (!validation.success) continue;
      const config = validation.data;
      if (config.languageServers) {
        return sanitizeUserLanguageServers(config.languageServers);
      }
    } catch {
      void 0;
    }
  }

  return {};
}

export function resolveLanguageServer(config: {
  command: string;
  args: string[];
  envVar: string;
  packageName?: string;
  binName?: string;
}): { command: string; args: string[] } {
  if (process.env[config.envVar]) {
    return { command: process.env[config.envVar]!, args: config.args };
  }

  const effectiveConfig = isTypeScriptServerConfig(config)
    ? TYPESCRIPT_LSP_PROVIDER_CONFIG[resolveTypeScriptLspProvider()]
    : config;

  const packagedServer = resolvePackagedLanguageServer(effectiveConfig);
  if (packagedServer) {
    return packagedServer;
  }

  return { command: effectiveConfig.command, args: effectiveConfig.args };
}

export function resolveTypeScriptLspProvider(): TypeScriptLspProvider {
  const requested = process.env[TYPESCRIPT_LSP_PROVIDER_ENV];
  if (isTypeScriptLspProvider(requested)) {
    return requested;
  }
  return 'typescript-language-server';
}

function isTypeScriptLspProvider(
  value: string | undefined
): value is TypeScriptLspProvider {
  return (
    value !== undefined &&
    TYPESCRIPT_LSP_PROVIDERS.includes(value as TypeScriptLspProvider)
  );
}

function isTypeScriptServerConfig(config: {
  command: string;
  envVar: string;
}): boolean {
  return (
    config.command === 'typescript-language-server' &&
    config.envVar === 'OCTOCODE_TS_SERVER_PATH'
  );
}

function resolvePackagedLanguageServer(
  config: PackagedLanguageServer
): { command: string; args: string[] } | null {
  if (!config.packageName || !config.binName) {
    return null;
  }

  try {
    const pkgPath = require.resolve(`${config.packageName}/package.json`);
    const pkg = require(pkgPath);
    const pkgDir = path.dirname(pkgPath);

    const binRelativePath = pkg.bin?.[config.binName];
    if (!binRelativePath || typeof binRelativePath !== 'string') {
      return null;
    }

    const binPath = path.join(pkgDir, binRelativePath);
    const validation = validateLSPServerPath(binPath, pkgDir);
    if (!validation.isValid) {
      return null;
    }

    return {
      command: process.execPath,
      args: [validation.resolvedPath!, ...config.args],
    };
  } catch {
    return null;
  }
}

export function detectLanguageId(filePath: string): string {
  const ext = path.extname(filePath).toLowerCase();
  return LANGUAGE_SERVER_COMMANDS[ext]?.languageId ?? 'plaintext';
}

export async function getLanguageServerForFile(
  filePath: string,
  workspaceRoot: string
): Promise<LanguageServerConfig | null> {
  const ext = path.extname(filePath).toLowerCase();

  const userConfig = await loadUserConfig(workspaceRoot);
  const userServer = userConfig[ext];
  if (userServer) {
    return {
      command: userServer.command,
      args: userServer.args ?? [],
      workspaceRoot,
      languageId: userServer.languageId,
    };
  }

  const serverInfo = LANGUAGE_SERVER_COMMANDS[ext];
  if (!serverInfo) return null;

  const { command, args } = resolveLanguageServer(serverInfo);

  return {
    command,
    args,
    workspaceRoot,
    languageId: serverInfo.languageId,
  };
}
