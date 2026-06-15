/**
 * native.ts — ESM bridge to the Rust .node binary.
 * All other source files import Rust functions from here.
 */
import { createRequire } from 'module';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { readFileSync } from 'fs';
import { execSync } from 'child_process';

const _require = createRequire(import.meta.url);
const _dir = dirname(fileURLToPath(import.meta.url));

export interface NativeSanitizationResult {
  content: string;
  hasSecrets: boolean;
  secretsDetected: string[];
  warnings: string[];
}

interface NativeModule {
  sanitizeContent(
    content: string,
    filePath: string | null
  ): NativeSanitizationResult;
  maskSensitiveData(text: string): string;
  patternCount(): number;
}

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
  if (typeof process.report?.getReport !== 'function') return null;
  (process.report as { excludeNetwork?: boolean }).excludeNetwork = true;
  const report = process.report.getReport() as Record<string, unknown>;
  if (!report) return null;
  if (
    report.header &&
    typeof report.header === 'object' &&
    'glibcVersionRuntime' in report.header
  )
    return false;
  if (
    Array.isArray(report.sharedObjects) &&
    (report.sharedObjects as string[]).some(isFileMusl)
  )
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

function loadNative(): NativeModule {
  const platform = process.platform;
  const arch = process.arch;
  const linuxLibc = platform === 'linux' ? (isMusl() ? 'musl' : 'gnu') : '';
  const tripleMap: Record<string, Record<string, string>> = {
    darwin: { arm64: 'darwin-arm64', x64: 'darwin-x64' },
    linux: {
      arm64: `linux-arm64-${linuxLibc}`,
      x64: `linux-x64-${linuxLibc}`,
    },
    win32: { x64: 'win32-x64-msvc' },
  };
  const triple = tripleMap[platform]?.[arch];
  const binaryNames = [
    ...(triple ? [`octocode-security.${triple}.node`] : []),
    'octocode-security.node',
  ];
  const candidates: string[] = [];

  if (process.env.OCTOCODE_SECURITY_NATIVE_PATH) {
    candidates.push(process.env.OCTOCODE_SECURITY_NATIVE_PATH);
  }

  // Preferred: load from the per-platform npm optionalDependency package.
  if (triple) {
    try {
      return _require(`octocode-security-${triple}`) as NativeModule;
    } catch {
      // not installed — fall through to file-based candidates
    }
  }

  // When octocode-security is bundled into octocode-mcp or octocode-cli, the
  // native asset is copied into the owning package's runtime directory.
  for (const binaryName of binaryNames) {
    candidates.push(join(_dir, 'runtime', 'security', binaryName));
    candidates.push(join(_dir, '..', 'runtime', 'security', binaryName));
    candidates.push(join(_dir, '..', '..', 'runtime', 'security', binaryName));
  }

  // Bun-compiled binaries keep native assets beside the executable/bundle.
  for (const binaryName of binaryNames) {
    candidates.push(join(_dir, binaryName));
  }

  // Package-local fallback: dist/ is one level below package root.
  const pkgRoot = join(_dir, '..');
  for (const binaryName of binaryNames) {
    candidates.push(join(pkgRoot, binaryName));
  }

  const errors: string[] = [];

  for (const candidate of candidates) {
    try {
      return _require(candidate) as NativeModule;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      errors.push(`${candidate}: ${message}`);
    }
  }

  throw new Error(
    `octocode-security: no prebuilt binary for ${platform}-${arch}. ` +
      `Run: cargo build --release && node scripts/copy-node.mjs. ` +
      `Tried:\n${errors.join('\n')}`
  );
}

const _native = loadNative();

export const nativeSanitizeContent = (
  content: string,
  filePath: string | null
): NativeSanitizationResult => _native.sanitizeContent(content, filePath);

export const nativeMaskSensitiveData = (text: string): string =>
  _native.maskSensitiveData(text);

export const nativePatternCount = (): number => _native.patternCount();
