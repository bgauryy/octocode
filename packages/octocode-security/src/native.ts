/**
 * native.ts — ESM bridge to the Rust .node binary.
 * All other source files import Rust functions from here.
 */
import { createRequire } from 'module';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { allRegexPatterns } from './regexes/index.js';

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

const MAX_CONTENT_SIZE = 10_000_000;

function isForcedJsFallback(): boolean {
  return process.env.OCTOCODE_SECURITY_FORCE_JS === '1';
}

function isNativeRequired(): boolean {
  return process.env.OCTOCODE_SECURITY_REQUIRE_NATIVE === '1';
}

function shouldApplyPattern(
  fileContext: RegExp | undefined,
  filePath: string | null
): boolean {
  if (!fileContext) return true;
  if (!filePath) return false;
  fileContext.lastIndex = 0;
  const applies = fileContext.test(filePath);
  fileContext.lastIndex = 0;
  return applies;
}

function replacementFor(patternName: string): string {
  return `[REDACTED-${patternName.toUpperCase()}]`;
}

function sanitizeContentJsFallback(
  content: string,
  filePath: string | null
): NativeSanitizationResult {
  if (content.length > MAX_CONTENT_SIZE) {
    return {
      content: '[CONTENT-REDACTED-SIZE-LIMIT]',
      hasSecrets: true,
      secretsDetected: ['content-size-exceeded'],
      warnings: [
        `Content exceeds ${MAX_CONTENT_SIZE} character limit — redacted for safety`,
      ],
    };
  }

  try {
    let sanitized = content;
    const secretsDetected: string[] = [];

    for (const pattern of allRegexPatterns) {
      if (!shouldApplyPattern(pattern.fileContext, filePath)) continue;

      pattern.regex.lastIndex = 0;
      if (pattern.regex.test(sanitized)) {
        secretsDetected.push(pattern.name);
        pattern.regex.lastIndex = 0;
        sanitized = sanitized.replace(
          pattern.regex,
          replacementFor(pattern.name)
        );
      }
      pattern.regex.lastIndex = 0;
    }

    return {
      content: sanitized,
      hasSecrets: secretsDetected.length > 0,
      secretsDetected,
      warnings:
        secretsDetected.length > 0
          ? [`${secretsDetected.length} secret(s) redacted`]
          : [],
    };
  } catch {
    return {
      content: '[CONTENT-REDACTED-DETECTION-ERROR]',
      hasSecrets: true,
      secretsDetected: ['detection-error'],
      warnings: ['Secret detection failed — content redacted for safety'],
    };
  }
}

function maskEveryOtherCharacter(text: string): string {
  let result = '';
  for (let i = 0; i < text.length; i++) {
    result += i % 2 === 0 ? '*' : text[i];
  }
  return result;
}

function maskSensitiveDataJsFallback(text: string): string {
  if (!text) return text;

  const matches: Array<{ start: number; end: number }> = [];
  for (const pattern of allRegexPatterns) {
    if (pattern.fileContext) continue;

    const flags = pattern.regex.flags.includes('g')
      ? pattern.regex.flags
      : `${pattern.regex.flags}g`;
    const regex = new RegExp(pattern.regex.source, flags);
    let match: RegExpExecArray | null;
    while ((match = regex.exec(text)) !== null) {
      matches.push({ start: match.index, end: match.index + match[0].length });
      if (match[0].length === 0) regex.lastIndex++;
    }
  }

  if (matches.length === 0) return text;

  matches.sort((a, b) => a.start - b.start);
  const nonOverlapping: Array<{ start: number; end: number }> = [];
  let lastEnd = 0;
  for (const match of matches) {
    if (match.start >= lastEnd) {
      nonOverlapping.push(match);
      lastEnd = match.end;
    }
  }

  let result = text;
  for (let i = nonOverlapping.length - 1; i >= 0; i--) {
    const { start, end } = nonOverlapping[i]!;
    result =
      result.slice(0, start) +
      maskEveryOtherCharacter(text.slice(start, end)) +
      result.slice(end);
  }
  return result;
}

function createJsFallbackNative(): NativeModule {
  return {
    sanitizeContent: sanitizeContentJsFallback,
    maskSensitiveData: maskSensitiveDataJsFallback,
    patternCount: () => allRegexPatterns.length,
  };
}

function isMusl(): boolean {
  try {
    const report = (
      process as NodeJS.Process & {
        report?: {
          getReport(): { header?: { glibcVersionRuntime?: string } };
        };
      }
    ).report?.getReport() as
      | { header?: { glibcVersionRuntime?: string } }
      | undefined;
    return !report?.header?.glibcVersionRuntime;
  } catch {
    return true;
  }
}

function loadNative(): NativeModule {
  if (isForcedJsFallback()) {
    return createJsFallbackNative();
  }

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
      // try next
    }
  }

  if (!isNativeRequired()) {
    return createJsFallbackNative();
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
