import { createHash } from 'node:crypto';
import { readFile, realpath, stat } from 'node:fs/promises';
import path from 'node:path';
import { LSPClient } from './client.js';
import { resolveServerForFile } from './config.js';
import { nativeBinding } from './native.js';
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

let poolConfiguration: Promise<void> | undefined;

function ensurePoolConfigured(): Promise<void> {
  poolConfiguration ??= nativeBinding.configureLspClientPool(
    POOL_IDLE_TIMEOUT_MS,
    POOL_MAX_ENTRIES
  );
  return poolConfiguration;
}

export interface PoolKey {
  workspaceRoot: string;
  filePath: string;
  languageId: string;
  serverId?: string;
  contextFingerprint?: string;
}

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
    await ensurePoolConfigured();
    const nativeClient = await nativeBinding.acquirePooledLspClient(
      LSPClient.nativeConfig(resolved.serverConfig)
    );
    if (!nativeClient) {
      return {
        ok: false,
        kind: 'startupFailed',
        message: 'Language server failed to start.',
        filePath,
        workspaceRoot,
      };
    }
    const client = LSPClient.fromPooled(resolved.serverConfig, nativeClient);
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
  await ensurePoolConfigured();
  await nativeBinding.clearPooledLspClients();
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
  await ensurePoolConfigured();
  return nativeBinding.releasePooledLspClient(
    LSPClient.nativeConfig(resolved.serverConfig)
  );
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
  await ensurePoolConfigured();
  const configs = (await nativeBinding.pooledLspClientConfigs()) as LanguageServerConfig[];
  const base = {
    enabled: true as const,
    pooledClientCount: nativeBinding.pooledLspClientCount(),
    pooledClients: configs.map(poolStatusKey),
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
  return nativeBinding.pooledLspClientCount();
}

function poolStatusKey(config: LanguageServerConfig): PoolKey {
  return {
    workspaceRoot: config.workspaceRoot,
    // File paths intentionally do not participate in the canonical pool key.
    filePath: config.workspaceRoot,
    languageId: config.languageId ?? 'unknown',
    contextFingerprint: serverConfigurationFingerprint(config),
    serverId: `${config.command} ${(config.args ?? []).join(' ')}`.trim(),
  };
}

type ResolvedPooledServer = {
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
  return { serverConfig, source: resolution.source };
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
