import { LSPClient } from './client.js';
import {
  getLanguageServerForFile,
  resolveServerForFile,
} from './config.js';
import { LspClientPool, type PoolKey } from './lspClientPool.js';
import { manifestInstallHint } from './serverManifest.js';
import { resolveWorkspaceRootForFile } from './workspaceRoot.js';
import type { LspServerSource } from './types.js';

export async function isLanguageServerAvailable(
  filePath: string,
  workspaceRoot?: string
): Promise<boolean> {
  const resolution = await resolveServerForFile(
    filePath,
    workspaceRoot ?? process.cwd()
  );
  return resolution != null && resolution.source !== 'unavailable';
}

export const LSP_UNAVAILABLE_HINT =
  'No language server is available for this file, so no semantic results were returned. Install a matching language server or set the relevant OCTOCODE_*_SERVER_PATH environment variable. For a text-based search meanwhile, use localSearchCode.';

// Single source of truth for toolchain-coupled servers — ones that can't be a
// portable download because they need a host toolchain/runtime to function.
// Both `unavailableHintFor` (by languageId) and the CLI's `lsp-server`
// (by server name) derive from this one list, so they can't drift.
export interface ToolchainServer {
  server: string;
  languageId: string;
  hint: string;
}

export const TOOLCHAIN_SERVERS: readonly ToolchainServer[] = [
  {
    server: 'gopls',
    languageId: 'go',
    hint: 'Install Go, then `go install golang.org/x/tools/gopls@latest` (gopls needs the Go toolchain at runtime).',
  },
  {
    server: 'jdtls',
    languageId: 'java',
    hint: 'Install a JDK/JRE 21+ and Eclipse JDT LS (https://download.eclipse.org/jdtls/).',
  },
  {
    server: 'sourcekit-lsp',
    languageId: 'swift',
    hint: 'Install Xcode or Xcode Command Line Tools (`xcode-select --install`); sourcekit-lsp ships at /usr/bin/sourcekit-lsp on macOS.',
  },
  {
    server: 'csharp-ls',
    languageId: 'csharp',
    hint: 'Install .NET SDK, then `dotnet tool install -g csharp-ls` (adds csharp-ls to ~/.dotnet/tools).',
  },
];

const TOOLCHAIN_INSTALL_HINTS: Record<string, string> = Object.fromEntries(
  TOOLCHAIN_SERVERS.map(t => [t.languageId, t.hint])
);

/** Honest, actionable guidance for a file whose server did not resolve. */
export function unavailableHintFor(languageId?: string, command?: string): string {
  const toolchain = languageId ? TOOLCHAIN_INSTALL_HINTS[languageId] : undefined;
  if (toolchain) return toolchain;
  const manifest = command ? manifestInstallHint(command) : null;
  if (manifest) return manifest;
  return LSP_UNAVAILABLE_HINT;
}

const POOL_IDLE_TIMEOUT_MS = parseInt(
  process.env.OCTOCODE_LSP_POOL_IDLE_MS || '60000',
  10
);

// Languages whose servers emit $/progress notifications and need waitForReady.
// TypeScript, Python, C/C++, and data-format servers (JSON/YAML/HTML/CSS)
// answer queries immediately after the LSP handshake — skipping waitForReady
// avoids burning the 2-second SETTLE_MS window for them.
const PROGRESS_LANGUAGES: ReadonlySet<string> = new Set([
  'go',
  'rust',
  'java',
  'csharp',
  'swift',
]);

// Per-language upper bound for $/progress drain (ms).
// These are ceilings — waitForReady returns as soon as the server goes idle.
const SERVER_READY_TIMEOUT_MS: Partial<Record<string, number>> = {
  go:      15_000,
  rust:    60_000,
  java:   120_000,
  csharp:  30_000,
  swift:   30_000,
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
  /** Which layer of the resolution ladder provided the server (or `unavailable`). */
  serverSource?: LspServerSource;
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
  const resolution = await resolveServerForFile(input.filePath, workspaceRoot);
  const languageId = resolution?.config.languageId;
  const serverSource: LspServerSource = resolution?.source ?? 'unavailable';
  const serverAvailable = serverSource !== 'unavailable';

  return {
    ...base,
    filePath: input.filePath,
    workspaceRoot,
    languageId,
    serverAvailable,
    serverSource,
    hints: serverAvailable
      ? [`Language server resolved for this file (source: ${serverSource}).`]
      : [unavailableHintFor(languageId, resolution?.config.command)],
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
