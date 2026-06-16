import { createRequire } from 'module';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { readFileSync } from 'fs';
import { execSync } from 'child_process';
import { maskEveryOtherChar } from './maskUtils.js';
import { allRegexPatterns } from './regexes/index.js';
import type { SensitiveDataPattern } from './types.js';

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

type NativeLoadState =
  | { kind: 'native'; module: NativeModule }
  | { kind: 'fallback'; error?: unknown };

const MAX_CONTENT_SIZE = 10_000_000;
const CONTENT_SIZE_EXCEEDED = 'content-size-exceeded';
const CONTENT_SIZE_PLACEHOLDER = '[CONTENT-REDACTED-SIZE-LIMIT]';

let nativeLoadState: NativeLoadState | undefined;

const isFileMusl = (f: string): boolean =>
  f.includes('libc.musl-') || f.includes('ld-musl-');

function envFlag(name: string): boolean {
  const value = process.env[name];
  return value === '1' || value?.toLowerCase() === 'true';
}

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
  const errors: string[] = [];

  if (process.env.OCTOCODE_SECURITY_NATIVE_PATH) {
    candidates.push(process.env.OCTOCODE_SECURITY_NATIVE_PATH);
  }

  if (triple) {
    try {
      return _require(`octocode-security-${triple}`) as NativeModule;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      errors.push(`octocode-security-${triple}: ${message}`);
    }
  }

  for (const binaryName of binaryNames) {
    candidates.push(join(_dir, 'runtime', 'security', binaryName));
    candidates.push(join(_dir, '..', 'runtime', 'security', binaryName));
    candidates.push(join(_dir, '..', '..', 'runtime', 'security', binaryName));
  }

  for (const binaryName of binaryNames) {
    candidates.push(join(_dir, binaryName));
  }

  const pkgRoot = join(_dir, '..');
  for (const binaryName of binaryNames) {
    candidates.push(join(pkgRoot, binaryName));
  }

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

function getNativeModule(): NativeModule | null {
  if (envFlag('OCTOCODE_SECURITY_FORCE_JS')) {
    if (envFlag('OCTOCODE_SECURITY_REQUIRE_NATIVE')) {
      throw new Error(
        'OCTOCODE_SECURITY_REQUIRE_NATIVE=1 conflicts with OCTOCODE_SECURITY_FORCE_JS=1'
      );
    }
    nativeLoadState = { kind: 'fallback' };
    return null;
  }

  if (nativeLoadState) {
    if (nativeLoadState.kind === 'native') return nativeLoadState.module;
    if (envFlag('OCTOCODE_SECURITY_REQUIRE_NATIVE')) {
      throw nativeLoadState.error instanceof Error
        ? nativeLoadState.error
        : new Error(String(nativeLoadState.error));
    }
    return null;
  }

  try {
    const module = loadNative();
    nativeLoadState = { kind: 'native', module };
    return module;
  } catch (error) {
    if (envFlag('OCTOCODE_SECURITY_REQUIRE_NATIVE')) {
      throw error;
    }
    nativeLoadState = { kind: 'fallback', error };
    return null;
  }
}

function cloneGlobalRegex(regex: RegExp): RegExp {
  const flags = regex.flags.includes('g') ? regex.flags : `${regex.flags}g`;
  return new RegExp(regex.source, flags);
}

function shouldApplyPattern(
  pattern: SensitiveDataPattern,
  filePath: string | null
): boolean {
  if (!pattern.fileContext) return true;
  if (!filePath) return false;

  pattern.fileContext.lastIndex = 0;
  const applies = pattern.fileContext.test(filePath);
  pattern.fileContext.lastIndex = 0;
  return applies;
}

function sanitizeWithJsFallback(
  content: string,
  filePath: string | null
): NativeSanitizationResult {
  if (content.length > MAX_CONTENT_SIZE) {
    return {
      content: CONTENT_SIZE_PLACEHOLDER,
      hasSecrets: true,
      secretsDetected: [CONTENT_SIZE_EXCEEDED],
      warnings: [
        `Content exceeds ${MAX_CONTENT_SIZE} character limit — redacted for safety`,
      ],
    };
  }

  let sanitized = content;
  const secretsDetected: string[] = [];

  for (const pattern of allRegexPatterns) {
    if (!shouldApplyPattern(pattern, filePath)) continue;

    const regex = cloneGlobalRegex(pattern.regex);
    regex.lastIndex = 0;

    if (!regex.test(sanitized)) continue;
    regex.lastIndex = 0;

    const next = sanitized.replace(
      regex,
      `[REDACTED-${pattern.name.toUpperCase()}]`
    );

    if (next !== sanitized) {
      sanitized = next;
      secretsDetected.push(pattern.name);
    }
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
}

function maskWithJsFallback(text: string): string {
  if (!text) return text;

  const matches: Array<{ start: number; end: number }> = [];

  for (const pattern of allRegexPatterns) {
    if (pattern.fileContext) continue;

    const regex = cloneGlobalRegex(pattern.regex);
    let match: RegExpExecArray | null;
    while ((match = regex.exec(text)) !== null) {
      matches.push({ start: match.index, end: match.index + match[0].length });
      if (match[0].length === 0) regex.lastIndex++;
    }
  }

  if (matches.length === 0) return text;

  matches.sort((left, right) => left.start - right.start);

  const nonOverlapping: Array<{ start: number; end: number }> = [];
  let lastEnd = 0;
  for (const match of matches) {
    if (match.start >= lastEnd) {
      nonOverlapping.push(match);
      lastEnd = match.end;
    }
  }

  let result = '';
  let position = 0;
  for (const match of nonOverlapping) {
    result += text.slice(position, match.start);
    result += maskEveryOtherChar(text.slice(match.start, match.end));
    position = match.end;
  }
  result += text.slice(position);
  return result;
}

export const nativeSanitizeContent = (
  content: string,
  filePath: string | null
): NativeSanitizationResult =>
  getNativeModule()?.sanitizeContent(content, filePath) ??
  sanitizeWithJsFallback(content, filePath);

export const nativeMaskSensitiveData = (text: string): string =>
  getNativeModule()?.maskSensitiveData(text) ?? maskWithJsFallback(text);

export const nativePatternCount = (): number =>
  getNativeModule()?.patternCount() ?? allRegexPatterns.length;
