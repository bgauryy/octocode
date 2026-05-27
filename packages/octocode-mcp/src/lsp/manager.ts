/**
 * LSP Client lifecycle management.
 *
 * All LSP tools acquire clients through the shared pool below. The legacy
 * spawn-per-request `createClient` was removed in May-2026 cleanup — every
 * call site now uses `acquirePooledClient` so tsserver stays warm across
 * agent bursts.
 *
 * @module lsp/manager
 */

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

/**
 * Check if a command exists in the system PATH.
 * Works cross-platform (Windows, macOS, Linux).
 *
 * @param command - The command name to check (e.g., 'node', 'python')
 * @returns Promise resolving to true if command exists, false otherwise
 *
 * @example
 * await commandExists('node')    // true (if Node.js installed)
 * await commandExists('nonexistent')  // false
 */
async function commandExists(command: string): Promise<boolean> {
  const isWindows = process.platform === 'win32';
  const checkCmd = isWindows ? 'where' : 'which';

  return new Promise(resolve => {
    const proc = spawn(checkCmd, [command], {
      stdio: 'ignore',
      shell: isWindows, // Required for 'where' on Windows
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

/**
 * Check if a language server is available for the given file type.
 * Checks user config, bundled servers, and PATH.
 *
 * @param filePath - Path to a source file
 * @param workspaceRoot - Optional workspace root for user config lookup
 * @returns Promise resolving to true if an LSP server is available
 *
 * @example
 * if (await isLanguageServerAvailable('/path/to/file.ts')) {
 *   // TypeScript language server is available
 * }
 */
export async function isLanguageServerAvailable(
  filePath: string,
  workspaceRoot?: string
): Promise<boolean> {
  const ext = path.extname(filePath).toLowerCase();

  // 1. Check user config first
  const userConfig = await loadUserConfig(workspaceRoot);
  const userServer = userConfig[ext];

  let command: string;

  if (userServer) {
    command = userServer.command;
  } else {
    // 2. Fall back to built-in defaults
    const serverInfo = LANGUAGE_SERVER_COMMANDS[ext];
    if (!serverInfo) {
      return false;
    }
    command = resolveLanguageServer(serverInfo).command;
  }

  // Bundled server (typescript-language-server via node)
  if (command === process.execPath) {
    return true;
  }

  // Absolute path - check if file exists
  if (path.isAbsolute(command)) {
    try {
      await fs.access(command);
      return true;
    } catch {
      // Absolute server command path missing or unreadable.
      return false;
    }
  }

  // PATH lookup - cross-platform check
  return commandExists(command);
}

/**
 * Shared hint returned by every LSP tool when no language server can be
 * located for the target file.
 *
 * Without this signal, callers (and AI agents) mistake the text-based
 * fallback for real semantic results and report "LSP isn't resolving
 * symbols for this project (likely no TS server indexed)". Emit it from
 * every LSP tool's fallback path so the failure mode is self-describing.
 */
export const LSP_UNAVAILABLE_HINT =
  'LSP unavailable for this file; returned a text-based fallback. ' +
  'For semantic results (cross-file refs, import chasing), install typescript-language-server: ' +
  '`npm i -g typescript-language-server typescript` or set OCTOCODE_TS_SERVER_PATH.';

/**
 * Default idle timeout for the shared LSP client pool (T3.2).
 * Tuned for typical agent burst patterns: several requests in a row,
 * then long pauses. Override via env for benchmarking.
 */
const POOL_IDLE_TIMEOUT_MS = parseInt(
  process.env.OCTOCODE_LSP_POOL_IDLE_MS || '60000',
  10
);

/**
 * Process-wide LSP client pool. Keyed on (workspaceRoot, languageId)
 * so different projects / languages don't share a tsserver. All LSP
 * tools acquire through this pool — there is no spawn-per-request path.
 */
const sharedPool = new LspClientPool<LSPClient>({
  idleTimeoutMs: POOL_IDLE_TIMEOUT_MS,
  factory: async key => {
    const serverConfig = await getLanguageServerForFile(
      // `factory` only knows the key (workspaceRoot + languageId);
      // resolve a synthetic filename so the config layer picks the
      // right server. The languageId is the LSP one (e.g. 'typescript'),
      // not a file extension, so we delegate to the explicit mapping
      // below for an extension hint.
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
        // ignore secondary cleanup errors
      }
      return null;
    }
  },
});

/**
 * Acquire (and pin) a pooled LSP client for the given file. Caller MUST
 * NOT stop the client — the pool owns its lifecycle.
 */
export async function acquirePooledClient(
  workspaceRoot: string,
  filePath: string
): Promise<LSPClient | null> {
  const languageId = languageIdForFile(filePath);
  if (!languageId) return null;
  return sharedPool.acquire({ workspaceRoot, languageId });
}

/**
 * Tear down every pooled client. Used by the upcoming `lspRestart`
 * tool to recover from confused server state.
 */
export async function releaseAllPooledClients(): Promise<void> {
  await sharedPool.clearAll();
}

/** Tear down the pooled client for one workspace/file, if supported. */
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

/** Return lightweight process-local LSP diagnostics. */
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

/** Internal: surface pool size for diagnostics / metrics. */
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
