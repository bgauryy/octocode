import { LSPClient } from './client.js';
import { getLanguageServerForFile } from './config.js';
import { LspClientPool, type PoolKey } from './lspClientPool.js';
import { nativeBinding } from './native.js';
import { resolveWorkspaceRootForFile } from './workspaceRoot.js';

export async function isLanguageServerAvailable(
  filePath: string,
  workspaceRoot?: string
): Promise<boolean> {
  const serverConfig = await getLanguageServerForFile(
    filePath,
    workspaceRoot ?? process.cwd()
  );
  if (!serverConfig) return false;
  return nativeBinding.isCommandAvailable(serverConfig.command);
}

export const LSP_UNAVAILABLE_HINT =
  'No language server is available for this file, so no semantic results were returned. Install a matching language server or set the relevant OCTOCODE_*_SERVER_PATH environment variable. For a text-based search meanwhile, use localSearchCode.';

const POOL_IDLE_TIMEOUT_MS = parseInt(
  process.env.OCTOCODE_LSP_POOL_IDLE_MS || '60000',
  10
);

// Languages whose servers emit $/progress notifications and need waitForReady.
// TypeScript, Python, C/C++, and data-format servers (JSON/YAML/TOML/HTML/CSS)
// answer queries immediately after the LSP handshake — skipping waitForReady
// avoids burning the 2-second SETTLE_MS window for them.
const PROGRESS_LANGUAGES: ReadonlySet<string> = new Set([
  'go',
  'rust',
  'java',
  'kotlin',
  'swift',
  'csharp',
  'elixir',
  'erlang',
]);

// Per-language upper bound for $/progress drain (ms).
// These are ceilings — waitForReady returns as soon as the server goes idle.
const SERVER_READY_TIMEOUT_MS: Partial<Record<string, number>> = {
  go:      15_000,
  rust:    60_000,
  java:   120_000,
  kotlin:  60_000,
  swift:   30_000,
  csharp:  30_000,
  elixir:  30_000,
  erlang:  30_000,
};
const DEFAULT_READY_TIMEOUT_MS = 30_000;

function readyTimeoutForLanguage(languageId: string): number {
  return SERVER_READY_TIMEOUT_MS[languageId] ?? DEFAULT_READY_TIMEOUT_MS;
}

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
      // Wait for servers that do workspace-wide indexing before answering
      // semantic queries. Servers that don't emit $/progress (TypeScript,
      // Python, clangd) answer immediately after the handshake — we skip
      // waitForReady for them to avoid the 2-second SETTLE_MS penalty.
      if (PROGRESS_LANGUAGES.has(key.languageId)) {
        await client.waitForReady(readyTimeoutForLanguage(key.languageId));
      }
      return client;
    } catch {
      await client.stop().catch(() => undefined);
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
  const serverConfig = await getLanguageServerForFile(
    input.filePath,
    workspaceRoot
  );
  const languageId = serverConfig?.languageId;
  const serverAvailable = serverConfig
    ? nativeBinding.isCommandAvailable(serverConfig.command)
    : false;

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

function synthesizeFilePathForKey(key: PoolKey): string {
  return key.filePath;
}

async function poolKeyForFile(
  workspaceRoot: string,
  filePath: string
): Promise<PoolKey | null> {
  const serverConfig = await getLanguageServerForFile(filePath, workspaceRoot);
  if (!serverConfig) return null;
  return {
    workspaceRoot,
    filePath,
    languageId: serverConfig.languageId ?? 'unknown',
    serverId:
      `${serverConfig.command} ${(serverConfig.args ?? []).join(' ')}`.trim(),
  };
}
