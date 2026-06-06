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
      return true;
    } catch {
      return false;
    }
  }

  return commandExists(command);
}

export const LSP_UNAVAILABLE_HINT =
  'No language server is available for this file, so no semantic results were returned. ' +
  'Install typescript-language-server (`npm i -g typescript-language-server typescript`) ' +
  'or set OCTOCODE_TS_SERVER_PATH. For a text-based search meanwhile, use localSearchCode.';

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
  const languageId = languageIdForFile(filePath);
  if (!languageId) return null;
  return sharedPool.acquire({ workspaceRoot, languageId });
}

export async function releaseAllPooledClients(): Promise<void> {
  await sharedPool.clearAll();
}

export async function releasePooledClientForFile(
  workspaceRoot: string,
  filePath: string
): Promise<boolean> {
  const languageId = languageIdForFile(filePath);
  if (!languageId) return false;
  await sharedPool.clear({ workspaceRoot, languageId });
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

const LANGUAGE_ID_FOR_EXT: Record<string, string> = {
  '.ts': 'typescript',
  '.tsx': 'typescriptreact',
  '.js': 'javascript',
  '.jsx': 'javascriptreact',
  '.mjs': 'javascript',
  '.cjs': 'javascript',
  '.py': 'python',
  '.rs': 'rust',
  '.go': 'go',
  '.java': 'java',
};

function languageIdForFile(filePath: string): string | null {
  return LANGUAGE_ID_FOR_EXT[path.extname(filePath).toLowerCase()] ?? null;
}

function synthesizeFilePathForKey(key: PoolKey): string {
  const ext =
    Object.entries(LANGUAGE_ID_FOR_EXT).find(
      ([, id]) => id === key.languageId
    )?.[0] ?? '.ts';
  return path.join(key.workspaceRoot, `__octocode_pool_probe${ext}`);
}
