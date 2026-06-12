/**
 * native.ts — ESM bridge to the Rust .node binary.
 * All other source files import Rust functions from here.
 */
import { createRequire } from 'module';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';

const _require = createRequire(import.meta.url);
const _dir = dirname(fileURLToPath(import.meta.url));

export interface NativeSanitizationResult {
  content: string;
  hasSecrets: boolean;
  secretsDetected: string[];
  warnings: string[];
}

interface NativeModule {
  sanitizeContent(content: string, filePath: string | null): NativeSanitizationResult;
  maskSensitiveData(text: string): string;
  patternCount(): number;
}

function loadNative(): NativeModule {
  const platform = process.platform;
  const arch = process.arch;
  const tripleMap: Record<string, Record<string, string>> = {
    darwin: { arm64: 'darwin-arm64', x64:   'darwin-x64' },
    linux:  { arm64: 'linux-arm64-gnu', x64: 'linux-x64-gnu' },
    win32:  { x64: 'win32-x64-msvc' },
  };
  const triple = tripleMap[platform]?.[arch];
  const candidates: string[] = [];
  // dist/ is one level below package root
  const pkgRoot = join(_dir, '..');
  if (triple) candidates.push(join(pkgRoot, `octocode-security.${triple}.node`));
  candidates.push(join(pkgRoot, 'octocode-security.node'));

  for (const candidate of candidates) {
    try {
      return _require(candidate) as NativeModule;
    } catch {
      // try next
    }
  }

  throw new Error(
    `octocode-security: no prebuilt binary for ${platform}-${arch}. ` +
    `Run: cargo build --release && node scripts/copy-node.mjs`
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
