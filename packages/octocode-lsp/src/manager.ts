import { spawn } from 'child_process';
import { promises as fs } from 'fs';
import * as path from 'path';
import { LSPClient } from './client.js';
import {
  getLanguageServerForFile,
  loadUserConfig,
  resolveLanguageServer,
  LANGUAGE_SERVER_COMMANDS,
} from './config.js';
import { LspClientPool, type PoolKey } from './lspClientPool.js';
import { resolveWorkspaceRootForFile } from './workspaceRoot.js';

async function commandExists(command: string): Promise<boolean> {
  const isWindows = process.platform === 'win32';
  const checkCmd = isWindows ? 'where' : 'which';

  return new Promise(resolve => {
    const proc = spawn(checkCmd, [command], {
      stdio: 'ignore',
      shell: isWindows,
    });

    const timeout = setTimeout(() => {
      proc.kill();
      resolve(false);
    }, 5000);

    proc.on('close', code => {
      clearTimeout(timeout);
      resolve(code === 0);
    });

    proc.on('error', () => {
      clearTimeout(timeout);
      resolve(false);
    });
  });
}

async function commandExitsZero(
  command: string,
  args: string[],
  timeoutMs: number
): Promise<boolean> {
  return new Promise(resolve => {
    const proc = spawn(command, args, {
      stdio: 'ignore',
      shell: false,
    });

    const timeout = setTimeout(() => {
      proc.kill();
      resolve(false);
    }, timeoutMs);

    proc.on('close', code => {
      clearTimeout(timeout);
      resolve(code === 0);
    });

    proc.on('error', () => {
      clearTimeout(timeout);
      resolve(false);
    });
  });
}

async function commandPassesHealthCheck(command: string): Promise<boolean> {
  const executable = path.basename(command).toLowerCase();
  if (executable !== 'rust-analyzer') {
    return true;
  }

  // rustup may install a rust-analyzer shim even when the component is missing.
  // `which rust-analyzer` succeeds in that state, but starting the LSP exits
  // immediately. A version probe reliably separates a usable server from a shim.
  return commandExitsZero(command, ['--version'], 5_000);
}

export async function isLanguageServerAvailable(
  filePath: string,
  workspaceRoot?: string
): Promise<boolean> {
  const ext = path.extname(filePath).toLowerCase();

  const userConfig = await loadUserConfig(workspaceRoot);
  const userServer = userConfig[ext];

  let command: string;

  if (userServer) {
    command = userServer.command;
  } else {
    const serverInfo = LANGUAGE_SERVER_COMMANDS[ext];
    if (!serverInfo) {
      return false;
    }
    command = resolveLanguageServer(serverInfo).command;
  }

  if (command === process.execPath) {
    return true;
  }

  if (path.isAbsolute(command)) {
    try {
      await fs.access(command);
      return commandPassesHealthCheck(command);
    } catch {
      return false;
    }
  }

  if (!(await commandExists(command))) {
    return false;
  }

  return commandPassesHealthCheck(command);
}

export const LSP_UNAVAILABLE_HINT =
  'No language server is available for this file, so no semantic results were returned. ' +
  'Install @typescript/native-preview for the default tsgo provider, ' +
  'or set OCTOCODE_TS_LSP_PROVIDER=typescript-language-server|vtsls with the matching server installed. ' +
  'For a custom binary, set OCTOCODE_TS_SERVER_PATH. For a text-based search meanwhile, use localSearchCode.';

const POOL_IDLE_TIMEOUT_MS = parseInt(
  process.env.OCTOCODE_LSP_POOL_IDLE_MS || '60000',
  10
);

const sharedPool = new LspClientPool<LSPClient>({
  idleTimeoutMs: POOL_IDLE_TIMEOUT_MS,
  factory: async key => {
    const serverConfig = await getLanguageServerForFile(
      synthesizeFilePathForKey(key),
      key.workspaceRoot
    );
    if (!serverConfig) return null;
    const client = new LSPClient(serverConfig);
    try {
      await client.start();
      return client;
    } catch {
      try {
        await client.stop();
      } catch {
        void 0;
      }
      return null;
    }
  },
});

export async function acquirePooledClient(
  workspaceRoot: string,
  filePath: string
): Promise<LSPClient | null> {
  const key = await poolKeyForFile(workspaceRoot, filePath);
  if (!key) return null;
  return sharedPool.acquire(key);
}

export async function releaseAllPooledClients(): Promise<void> {
  await sharedPool.clearAll();
}

export async function releasePooledClientForFile(
  workspaceRoot: string,
  filePath: string
): Promise<boolean> {
  const key = await poolKeyForFile(workspaceRoot, filePath);
  if (!key) return false;
  await sharedPool.clear(key);
  return true;
}

export type LspStatusInput = {
  filePath?: string;
  workspaceRoot?: string;
};

export type LspStatusResult = {
  enabled: true;
  pooledClientCount: number;
  pooledClients: PoolKey[];
  filePath?: string;
  workspaceRoot?: string;
  languageId?: string;
  serverAvailable?: boolean;
  hints: string[];
};

export async function getLspStatus(
  input: LspStatusInput = {}
): Promise<LspStatusResult> {
  const base = {
    enabled: true as const,
    pooledClientCount: sharedPool.size(),
    pooledClients: sharedPool.keys(),
  };

  if (!input.filePath) {
    return {
      ...base,
      hints: [
        'Provide filePath to check language server availability for a specific file.',
      ],
    };
  }

  const workspaceRoot =
    input.workspaceRoot ?? (await resolveWorkspaceRootForFile(input.filePath));
  const languageId = languageIdForFile(input.filePath) ?? undefined;
  const serverAvailable = await isLanguageServerAvailable(
    input.filePath,
    workspaceRoot
  );

  return {
    ...base,
    filePath: input.filePath,
    workspaceRoot,
    languageId,
    serverAvailable,
    hints: serverAvailable
      ? ['Language server appears available for this file.']
      : [LSP_UNAVAILABLE_HINT],
  };
}

export function pooledClientCount(): number {
  return sharedPool.size();
}

// Derived from LANGUAGE_SERVER_COMMANDS — single source of truth so this map
// never drifts out of sync with the server registry.
const LANGUAGE_ID_FOR_EXT: Record<string, string> = Object.fromEntries(
  Object.entries(LANGUAGE_SERVER_COMMANDS).map(([ext, cfg]) => [
    ext,
    cfg.languageId,
  ])
);

function languageIdForFile(filePath: string): string | null {
  return LANGUAGE_ID_FOR_EXT[path.extname(filePath).toLowerCase()] ?? null;
}

function synthesizeFilePathForKey(key: PoolKey): string {
  const ext =
    key.extension ??
    Object.entries(LANGUAGE_SERVER_COMMANDS).find(
      ([, cfg]) => serverIdentityForRegistryCommand(cfg) === key.serverId
    )?.[0] ??
    Object.entries(LANGUAGE_ID_FOR_EXT).find(
      ([, id]) => id === key.languageId
    )?.[0] ??
    '.ts';
  return path.join(key.workspaceRoot, `__octocode_pool_probe${ext}`);
}

async function poolKeyForFile(
  workspaceRoot: string,
  filePath: string
): Promise<PoolKey | null> {
  const ext = path.extname(filePath).toLowerCase();
  const userServer = (await loadUserConfig(workspaceRoot))[ext];
  if (userServer) {
    return {
      workspaceRoot,
      languageId: userServer.languageId,
      serverId: serverIdentityForCommand(
        userServer.command,
        userServer.args ?? []
      ),
      extension: ext,
    };
  }

  const serverInfo = LANGUAGE_SERVER_COMMANDS[ext];
  if (!serverInfo) return null;

  return {
    workspaceRoot,
    languageId: serverInfo.languageId,
    serverId: serverIdentityForRegistryCommand(serverInfo),
  };
}

function serverIdentityForRegistryCommand(config: {
  command: string;
  args: string[];
  envVar: string;
}): string {
  return serverIdentityForCommand(config.command, config.args, config.envVar);
}

function serverIdentityForCommand(
  command: string,
  args: string[],
  envVar?: string
): string {
  return [envVar ?? command, command, ...args].join('\u0000');
}
