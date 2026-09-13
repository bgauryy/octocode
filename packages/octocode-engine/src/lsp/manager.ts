import { createHash } from 'node:crypto';
import { readFile, realpath, stat } from 'node:fs/promises';
import path from 'node:path';
import { LSPClient } from './client.js';
import { getLanguageServerForFile, resolveServerForFile } from './config.js';
import { LspClientPool, type PoolKey, serializeKey } from './lspClientPool.js';
import { manifestInstallHint } from './serverManifest.js';
import { resolveWorkspaceRootForFile } from './workspaceRoot.js';
import type {
  LanguageServerConfig,
  LspServerSource,
  RustBuildContext,
} from './types.js';
import {
  applyRustBuildContext,
  serverConfigurationFingerprint,
} from './rustContext.js';

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
  'No language server is available for this file, so no semantic results were returned. Install a matching language server or set the relevant OCTOCODE_*_SERVER_PATH environment variable. For a text-based search meanwhile, use localSearch.';

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
    server: 'intelephense',
    languageId: 'php',
    hint: 'Install `intelephense` in the project or on PATH, or set `OCTOCODE_PHP_SERVER_PATH` to its executable.',
  },
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
  {
    server: 'ruby-lsp',
    languageId: 'ruby',
    hint: 'Install a supported Ruby and `gem install ruby-lsp`, or set `OCTOCODE_RUBY_SERVER_PATH` to the ruby-lsp executable.',
  },
  {
    server: 'kotlin-language-server',
    languageId: 'kotlin',
    hint: 'Install the Kotlin language server and its required JDK; put `kotlin-language-server` on PATH or set `OCTOCODE_KOTLIN_SERVER_PATH`.',
  },
  {
    server: 'elixir-ls',
    languageId: 'elixir',
    hint: 'Install Elixir/Erlang and ElixirLS; set `OCTOCODE_ELIXIR_SERVER_PATH` to its language_server.sh launcher (or an elixir-ls executable on PATH).',
  },
  {
    server: 'sqls',
    languageId: 'sql',
    hint: 'Install `sqls` from https://github.com/sqls-server/sqls and put it on PATH, or set `OCTOCODE_SQL_SERVER_PATH`.',
  },
  {
    server: 'metals',
    languageId: 'scala',
    hint: 'Install Metals with a supported JDK and Coursier (https://scalameta.org/metals/docs/); put `metals` on PATH or set `OCTOCODE_SCALA_SERVER_PATH`.',
  },
];

const TOOLCHAIN_INSTALL_HINTS: Record<string, string> = Object.fromEntries(
  TOOLCHAIN_SERVERS.map(t => [t.languageId, t.hint])
);

/** Honest, actionable guidance for a file whose server did not resolve. */
export function unavailableHintFor(
  languageId?: string,
  command?: string
): string {
  const toolchain = languageId
    ? TOOLCHAIN_INSTALL_HINTS[languageId]
    : undefined;
  if (toolchain) return toolchain;
  const manifest = command ? manifestInstallHint(command) : null;
  if (manifest) return manifest;
  return LSP_UNAVAILABLE_HINT;
}

export function parsePoolIdleTimeoutMs(
  raw: string | undefined = process.env.OCTOCODE_LSP_POOL_IDLE_MS
): number {
  const parsed = Number.parseInt(raw ?? '', 10);
  return Number.isFinite(parsed) && parsed >= 1_000 ? parsed : 60_000;
}

const POOL_IDLE_TIMEOUT_MS = parsePoolIdleTimeoutMs();

export function parsePoolMaxEntries(
  raw: string | undefined = process.env.OCTOCODE_LSP_POOL_MAX_CLIENTS
): number {
  const parsed = Number.parseInt(raw ?? '', 10);
  if (!Number.isFinite(parsed) || parsed < 1) return 4;
  return Math.min(parsed, 32);
}

const POOL_MAX_ENTRIES = parsePoolMaxEntries();

// Servers that need post-initialize readiness before semantic requests.
// Bash awaits workspace configuration before enabling document analysis without
// emitting progress; its bounded settle remains explicitly unconfirmed.
// TypeScript, Python, C/C++, and data-format servers (JSON/YAML/HTML/CSS)
// answer queries immediately after the LSP handshake — skipping waitForReady
// avoids burning the 2-second SETTLE_MS window for them.
const STARTUP_WAIT_LANGUAGES: ReadonlySet<string> = new Set([
  'go',
  'rust',
  'java',
  'csharp',
  'swift',
  'shellscript',
]);

// Per-language upper bound for $/progress drain (ms).
// These are ceilings — waitForReady returns as soon as the server goes idle.
const SERVER_READY_TIMEOUT_MS: Partial<Record<string, number>> = {
  go: 15_000,
  rust: 60_000,
  java: 120_000,
  csharp: 30_000,
  swift: 30_000,
  shellscript: 2_000,
};
const DEFAULT_READY_TIMEOUT_MS = 30_000;

function readyTimeoutForLanguage(languageId: string): number {
  return SERVER_READY_TIMEOUT_MS[languageId] ?? DEFAULT_READY_TIMEOUT_MS;
}

// Eliminates the double getLanguageServerForFile call that would otherwise
// happen once in poolKeyForFile (to build the key) and again inside the factory
// (to create the client). poolKeyForFile deposits the already-resolved config
// here before calling sharedPool.acquire; the factory reads and clears it.
//
// The deposit is conditional: when sharedPool already has an entry or an
// inflight promise for this key, acquire() returns the cached client or the
// existing promise WITHOUT invoking the factory — so depositing again here
// would never be read or cleared and would leak the entry forever. Key format
// is the shared serializeKey from lspClientPool.ts.
const _pendingConfigs = new Map<string, LanguageServerConfig>();

const sharedPool = new LspClientPool<LSPClient>({
  idleTimeoutMs: POOL_IDLE_TIMEOUT_MS,
  maxEntries: POOL_MAX_ENTRIES,
  factory: async key => {
    const cacheKey = serializeKey(key);
    const serverConfig =
      _pendingConfigs.get(cacheKey) ??
      (await getLanguageServerForFile(
        synthesizeFilePathForKey(key),
        key.workspaceRoot
      ));
    _pendingConfigs.delete(cacheKey);
    if (!serverConfig) return null;
    const client = new LSPClient(serverConfig);
    try {
      await client.start();
      // Wait for servers that do workspace-wide indexing before answering
      // semantic queries, plus Bash's asynchronous configuration handshake.
      // Other servers skip the bounded settle interval.
      if (STARTUP_WAIT_LANGUAGES.has(key.languageId)) {
        await client.waitForReady(readyTimeoutForLanguage(key.languageId));
      }
      return client;
    } catch (error) {
      await client.stop().catch(() => undefined);
      throw error;
    }
  },
});

export type LspClientAcquireFailureKind = 'unavailable' | 'startupFailed';

export type ResolvedLanguageServerArtifact = {
  role: 'command' | 'argument';
  path: string;
  size: number;
  sha256?: string;
};

export type ResolvedLanguageServerPackage = {
  name?: string;
  version?: string;
  manifestPath: string;
  manifestSha256: string;
};

/**
 * The effective server invocation and observable semantic state for an LSP
 * response. It records the post-resolution config, not an input preference.
 */
export type ResolvedLanguageServerReceipt = {
  command: string;
  /** Arguments passed to `command`, in their exact execution order. */
  argv: string[];
  source: Exclude<LspServerSource, 'unavailable'>;
  workspaceRoot: string;
  workspaceFingerprint: string;
  configurationFingerprint: string;
  capabilities: Record<string, boolean>;
  readiness?: ReturnType<LSPClient['getReadiness']>;
  identity: {
    artifacts: ResolvedLanguageServerArtifact[];
    packages: ResolvedLanguageServerPackage[];
  };
};

export type LspClientAcquireResult =
  | { ok: true; client: LSPClient; receipt: ResolvedLanguageServerReceipt }
  | {
      ok: false;
      kind: LspClientAcquireFailureKind;
      message: string;
      filePath: string;
      workspaceRoot: string;
    };

export async function acquirePooledClientDetailed(
  workspaceRoot: string,
  filePath: string,
  rustContext?: RustBuildContext
): Promise<LspClientAcquireResult> {
  const resolved = await resolvePooledServerForFile(
    workspaceRoot,
    filePath,
    rustContext
  );
  if (!resolved) {
    return {
      ok: false,
      kind: 'unavailable',
      message: 'No language server is available for this file.',
      filePath,
      workspaceRoot,
    };
  }
  try {
    const client = await sharedPool.acquire(resolved.key);
    if (!client) {
      return {
        ok: false,
        kind: 'startupFailed',
        message: 'Language server failed to start.',
        filePath,
        workspaceRoot,
      };
    }
    return {
      ok: true,
      client,
      receipt: await resolvedLanguageServerReceipt(
        resolved.serverConfig,
        resolved.source,
        client
      ),
    };
  } catch (error) {
    return {
      ok: false,
      kind: 'startupFailed',
      message: error instanceof Error ? error.message : String(error),
      filePath,
      workspaceRoot,
    };
  }
}

export async function acquirePooledClient(
  workspaceRoot: string,
  filePath: string,
  rustContext?: RustBuildContext
): Promise<LSPClient | null> {
  const result = await acquirePooledClientDetailed(
    workspaceRoot,
    filePath,
    rustContext
  );
  return result.ok ? result.client : null;
}

export async function releaseAllPooledClients(): Promise<void> {
  await sharedPool.clearAll();
}

export async function releasePooledClientForFile(
  workspaceRoot: string,
  filePath: string,
  rustContext?: RustBuildContext
): Promise<boolean> {
  const resolved = await resolvePooledServerForFile(
    workspaceRoot,
    filePath,
    rustContext
  );
  if (!resolved) return false;
  await sharedPool.clear(resolved.key);
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

type ResolvedPooledServer = {
  key: PoolKey;
  serverConfig: LanguageServerConfig;
  source: Exclude<LspServerSource, 'unavailable'>;
};

async function resolvePooledServerForFile(
  workspaceRoot: string,
  filePath: string,
  rustContext?: RustBuildContext
): Promise<ResolvedPooledServer | null> {
  const resolution = await resolveServerForFile(filePath, workspaceRoot);
  if (!resolution || resolution.source === 'unavailable') return null;
  const serverConfig = applyRustBuildContext(resolution.config, rustContext);
  const key: PoolKey = {
    workspaceRoot,
    filePath,
    languageId: serverConfig.languageId ?? 'unknown',
    contextFingerprint: serverConfigurationFingerprint(serverConfig),
    serverId:
      `${serverConfig.command} ${(serverConfig.args ?? []).join(' ')}`.trim(),
  };
  const serialized = serializeKey(key);
  // Only deposit when the pool will actually call the factory for this key.
  // If there's already an entry or inflight, acquire() won't start a new factory
  // run — so depositing here would permanently leak the entry.
  if (!sharedPool.has(key)) {
    _pendingConfigs.set(serialized, serverConfig);
  }
  return { key, serverConfig, source: resolution.source };
}

const RECEIPT_CAPABILITIES = [
  'definitionProvider',
  'typeDefinitionProvider',
  'implementationProvider',
  'referencesProvider',
  'hoverProvider',
  'callHierarchyProvider',
  'typeHierarchyProvider',
  'documentSymbolProvider',
  'workspaceSymbolProvider',
  'diagnosticProvider',
] as const;
const MAX_ARTIFACT_HASH_BYTES = 16 * 1024 * 1024;

async function resolvedLanguageServerReceipt(
  serverConfig: LanguageServerConfig,
  source: Exclude<LspServerSource, 'unavailable'>,
  client: LSPClient
): Promise<ResolvedLanguageServerReceipt> {
  const invocation = [serverConfig.command, ...(serverConfig.args ?? [])];
  const artifacts = (
    await Promise.all(
      invocation.map((candidate, index) =>
        artifactIdentity(candidate, index === 0 ? 'command' : 'argument')
      )
    )
  ).filter(
    (artifact): artifact is ResolvedLanguageServerArtifact => artifact != null
  );
  const packageRoots = new Set(
    artifacts
      .map(artifact => packageRootForArtifact(artifact.path))
      .filter((root): root is string => root != null)
  );
  const packages = (
    await Promise.all([...packageRoots].map(packageIdentity))
  ).filter((pkg): pkg is ResolvedLanguageServerPackage => pkg != null);

  return {
    command: serverConfig.command,
    argv: [...(serverConfig.args ?? [])],
    source,
    workspaceRoot: serverConfig.workspaceRoot,
    workspaceFingerprint: fingerprint(path.resolve(serverConfig.workspaceRoot)),
    configurationFingerprint: serverConfigurationFingerprint(serverConfig),
    capabilities: Object.fromEntries(
      RECEIPT_CAPABILITIES.map(capability => [
        capability,
        client.hasCapability(capability),
      ])
    ),
    readiness: client.getReadiness(),
    identity: { artifacts, packages },
  };
}

function fingerprint(value: string): string {
  return createHash('sha256').update(value).digest('hex');
}

async function artifactIdentity(
  candidate: string,
  role: ResolvedLanguageServerArtifact['role']
): Promise<ResolvedLanguageServerArtifact | null> {
  if (!path.isAbsolute(candidate)) return null;
  try {
    const canonicalPath = await realpath(candidate);
    const metadata = await stat(canonicalPath);
    if (!metadata.isFile()) return null;
    const artifact: ResolvedLanguageServerArtifact = {
      role,
      path: canonicalPath,
      size: metadata.size,
    };
    if (metadata.size <= MAX_ARTIFACT_HASH_BYTES) {
      artifact.sha256 = createHash('sha256')
        .update(await readFile(canonicalPath))
        .digest('hex');
    }
    return artifact;
  } catch {
    return null;
  }
}

function packageRootForArtifact(filePath: string): string | null {
  const marker = `${path.sep}node_modules${path.sep}`;
  const markerIndex = filePath.lastIndexOf(marker);
  if (markerIndex < 0) return null;
  const packageSegments = filePath
    .slice(markerIndex + marker.length)
    .split(path.sep)
    .filter(Boolean);
  if (!packageSegments.length) return null;
  const packageLength = packageSegments[0]?.startsWith('@') ? 2 : 1;
  if (packageSegments.length < packageLength) return null;
  return path.join(
    filePath.slice(0, markerIndex + marker.length),
    ...packageSegments.slice(0, packageLength)
  );
}

async function packageIdentity(
  packageRoot: string
): Promise<ResolvedLanguageServerPackage | null> {
  const manifestPath = path.join(packageRoot, 'package.json');
  try {
    const manifest = await readFile(manifestPath);
    const parsed = JSON.parse(manifest.toString()) as {
      name?: unknown;
      version?: unknown;
    };
    return {
      ...(typeof parsed.name === 'string' ? { name: parsed.name } : {}),
      ...(typeof parsed.version === 'string'
        ? { version: parsed.version }
        : {}),
      manifestPath,
      manifestSha256: createHash('sha256').update(manifest).digest('hex'),
    };
  } catch {
    return null;
  }
}
