import { existsSync } from 'node:fs';
import { join } from 'node:path';

const packageName = 'octocode-lsp';
const binaryName = 'octocode-lsp';
const { platform, arch } = process;

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

function isMusl(): boolean {
  const report = process.report?.getReport?.();
  const header =
    report && typeof report === 'object' && 'header' in report
      ? report.header
      : undefined;
  return !(
    header &&
    typeof header === 'object' &&
    'glibcVersionRuntime' in header
  );
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
  const candidates = [
    join(__dirname, `${binaryName}.${key}.node`),
    join(__dirname, '..', `${binaryName}.${key}.node`),
  ];

  for (const candidate of candidates) {
    if (existsSync(candidate)) {
      return require(candidate) as NativeBinding;
    }
  }

  throw new Error(
    `${packageName} native binary not found for ${platform}-${arch}. Tried: ${candidates.join(', ')}`
  );
}

export const nativeBinding = loadNativeBinding();
