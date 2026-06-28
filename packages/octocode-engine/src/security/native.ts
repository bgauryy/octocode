import { createRequire } from 'module';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { deduplicateSpans, applyMaskToSpans } from './maskUtils.js';
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

function envFlag(name: string): boolean {
  const value = process.env[name];
  return value === '1' || value?.toLowerCase() === 'true';
}

// Secret detection is part of the single octocode-engine native binary. The
// engine root loader (index.cjs) already resolves the correct platform binary
// (incl. musl detection), so we simply require it — both dist/security/native.js
// and src/security/native.ts sit two levels below the package root.
function loadNative(): NativeModule {
  const candidates: string[] = [];
  if (process.env.OCTOCODE_SECURITY_NATIVE_PATH) {
    candidates.push(process.env.OCTOCODE_SECURITY_NATIVE_PATH);
  }
  candidates.push(join(_dir, '..', '..', 'index.cjs'));

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
    `octocode-engine: secret-detection native binding unavailable for ` +
      `${process.platform}-${process.arch}. Tried:\n${errors.join('\n')}`
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
  // Byte length (UTF-8) to match the native path's `content.len()` — otherwise
  // large multibyte content is redacted by native but not by this fallback.
  if (Buffer.byteLength(content, 'utf8') > MAX_CONTENT_SIZE) {
    return {
      content: CONTENT_SIZE_PLACEHOLDER,
      hasSecrets: true,
      secretsDetected: [CONTENT_SIZE_EXCEEDED],
      warnings: [
        `Content exceeds ${MAX_CONTENT_SIZE} byte limit — redacted for safety`,
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

  const spans: Array<{ start: number; end: number }> = [];

  for (const pattern of allRegexPatterns) {
    if (pattern.fileContext) continue;

    const regex = cloneGlobalRegex(pattern.regex);
    let match: RegExpExecArray | null;
    while ((match = regex.exec(text)) !== null) {
      spans.push({ start: match.index, end: match.index + match[0].length });
      if (match[0].length === 0) regex.lastIndex++;
    }
  }

  if (spans.length === 0) return text;
  return applyMaskToSpans(text, deduplicateSpans(spans));
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
