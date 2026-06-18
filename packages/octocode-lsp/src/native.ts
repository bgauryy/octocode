import { existsSync, readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { execSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const packageName = 'octocode-lsp';
const binaryName = 'octocode-lsp';
const { platform, arch } = process;
const require = createRequire(import.meta.url);
const __dirname = dirname(fileURLToPath(import.meta.url));

type NativeBinding = {
  NativeLspClient: new (config: unknown) => NativeLspClientBinding;
  resolvePosition(filePath: string, fuzzy: unknown): unknown;
  resolvePositionFromContent(content: string, fuzzy: unknown): unknown;
  toUri(path: string): string;
  fromUri(uri: string): string;
  resolveWorkspaceRootForFile(filePath: string): string;
  detectLanguageId(filePath: string): string | undefined;
  getLanguageServerForFile(
    filePath: string,
    workspaceRoot: string
  ): unknown | undefined;
  isCommandAvailable(command: string): boolean;
  safeReadFile(filePath: string): string;
  validateLspServerPath(command: string): string;
  convertSymbolKind(kind?: number): string;
  toLspSymbolKind(kind: string): number;
};

export type NativeLspClientBinding = {
  start(): Promise<void>;
  stop(): Promise<void>;
  waitForReady(timeoutMs?: number): Promise<void>;
  hasCapability?(capability: string): boolean;
  getRecentStderr?(): string[];
  openDocument(filePath: string, content: string): Promise<void>;
  getDefinition(
    filePath: string,
    line: number,
    character: number
  ): Promise<unknown[]>;
  getReferences(
    filePath: string,
    line: number,
    character: number,
    includeDeclaration?: boolean
  ): Promise<unknown[]>;
  getHover(filePath: string, line: number, character: number): Promise<unknown>;
  getTypeDefinition(
    filePath: string,
    line: number,
    character: number
  ): Promise<unknown[]>;
  getImplementation(
    filePath: string,
    line: number,
    character: number
  ): Promise<unknown[]>;
  getDocumentSymbols(filePath: string): Promise<unknown>;
  prepareCallHierarchy(
    filePath: string,
    line: number,
    character: number
  ): Promise<unknown>;
  incomingCalls(item: unknown): Promise<unknown>;
  outgoingCalls(item: unknown): Promise<unknown>;
};

const isFileMusl = (f: string): boolean =>
  f.includes('libc.musl-') || f.includes('ld-musl-');

function isMuslFromFilesystem(): boolean | null {
  try {
    return readFileSync('/usr/bin/ldd', 'utf-8').includes('musl');
  } catch {
    return null;
  }
}

function isMuslFromReport(): boolean | null {
  let report: unknown = null;
  if (typeof process.report?.getReport === 'function') {
    (process.report as { excludeNetwork?: boolean }).excludeNetwork = true;
    report = process.report.getReport();
  }
  if (!report) return null;
  const r = report as Record<string, unknown>;
  if (
    r.header &&
    typeof r.header === 'object' &&
    'glibcVersionRuntime' in r.header
  )
    return false;
  if (Array.isArray(r.sharedObjects) && r.sharedObjects.some(isFileMusl))
    return true;
  return false;
}

function isMuslFromChildProcess(): boolean {
  try {
    return execSync('ldd --version', { encoding: 'utf8' }).includes('musl');
  } catch {
    return false;
  }
}

function isMusl(): boolean {
  if (process.platform !== 'linux') return false;
  let result: boolean | null = isMuslFromFilesystem();
  if (result === null) result = isMuslFromReport();
  if (result === null) result = isMuslFromChildProcess();
  return !!result;
}

function getPlatformKey(): string {
  if (platform === 'darwin') {
    if (arch === 'arm64') return 'darwin-arm64';
    if (arch === 'x64') return 'darwin-x64';
  }

  if (platform === 'linux') {
    const libc = isMusl() ? 'musl' : 'gnu';
    if (arch === 'x64') return `linux-x64-${libc}`;
    if (arch === 'arm64' && libc === 'gnu') return 'linux-arm64-gnu';
  }

  if (platform === 'win32' && arch === 'x64') return 'win32-x64-msvc';

  throw new Error(
    `${packageName} does not ship a native binary for ${platform}-${arch}`
  );
}

function loadNativeBinding(): NativeBinding {
  const key = getPlatformKey();
  const fileName = `${binaryName}.${key}.node`;

  // 1. Explicit override (tests / custom layouts).
  const envOverride = process.env.OCTOCODE_LSP_NATIVE_PATH;
  if (envOverride && existsSync(envOverride)) {
    return require(envOverride) as NativeBinding;
  }

  // 2. Per-platform optional dependency (standard npm install path).
  try {
    return require(`${packageName}-${key}`) as NativeBinding;
  } catch {
    /* try next candidate */
  }

  // 3. Local/dev (.node next to or above the loader) and standalone-binary
  //    layout (runtime/lsp/, copied next to the executable by bundle-lsp.mjs).
  const candidates = [
    join(__dirname, fileName),
    join(__dirname, '..', fileName),
    join(__dirname, 'runtime', 'lsp', fileName),
    join(__dirname, '..', 'runtime', 'lsp', fileName),
    join(__dirname, '..', '..', 'runtime', 'lsp', fileName),
  ];

  for (const candidate of candidates) {
    if (existsSync(candidate)) {
      return require(candidate) as NativeBinding;
    }
  }

  throw new Error(
    `${packageName} native binary not found for ${platform}-${arch}. ` +
      `Install the optional dependency '${packageName}-${key}' or build locally with 'yarn build:dev'. ` +
      `Tried local paths: ${candidates.join(', ')}`
  );
}

export const nativeBinding = loadNativeBinding();
