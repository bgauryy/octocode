import { IGNORED_PATH_PATTERNS } from './pathPatterns.js';
import { IGNORED_FILE_PATTERNS } from './filePatterns.js';
import { securityRegistry } from './registry.js';

let _compiledPathRegex: RegExp | null = null;
let _compiledFileRegex: RegExp | null = null;
let _cachedVersion = -1;

function stripNamedGroups(source: string): string {
  return source.replace(/\(\?<[^>]+>/g, '(?:');
}

function invalidateIfNeeded(): void {
  const ver = securityRegistry.version;
  if (ver !== _cachedVersion) {
    _compiledPathRegex = null;
    _compiledFileRegex = null;
    _cachedVersion = ver;
  }
}

function getCompiledPathRegex(): RegExp {
  invalidateIfNeeded();
  if (!_compiledPathRegex) {
    const extra = securityRegistry.extraIgnoredPathPatterns;
    const all =
      extra.length > 0
        ? [...IGNORED_PATH_PATTERNS, ...extra]
        : IGNORED_PATH_PATTERNS;
    _compiledPathRegex = new RegExp(
      all.map(r => stripNamedGroups(r.source)).join('|')
    );
  }
  return _compiledPathRegex;
}

function getCompiledFileRegex(): RegExp {
  invalidateIfNeeded();
  if (!_compiledFileRegex) {
    const extra = securityRegistry.extraIgnoredFilePatterns;
    const all =
      extra.length > 0
        ? [...IGNORED_FILE_PATTERNS, ...extra]
        : IGNORED_FILE_PATTERNS;
    _compiledFileRegex = new RegExp(
      all.map(r => stripNamedGroups(r.source)).join('|')
    );
  }
  return _compiledFileRegex;
}

export function shouldIgnorePath(pathToCheck: string): boolean {
  if (!pathToCheck || pathToCheck.trim() === '') {
    return true;
  }

  const normalizedPath = normalizePathForIgnoreMatching(
    pathToCheck.replace(/\\/g, '/')
  );
  const regex = getCompiledPathRegex();

  const pathParts = normalizedPath.split('/');
  for (const part of pathParts) {
    if (regex.test(part)) return true;
  }

  return regex.test(normalizedPath);
}

function normalizePathForIgnoreMatching(normalizedPath: string): string {
  if (normalizedPath === '/private/var') return '/var';
  if (normalizedPath.startsWith('/private/var/')) {
    return normalizedPath.slice('/private'.length);
  }
  return normalizedPath;
}

export function shouldIgnoreFile(fileName: string): boolean {
  if (!fileName || fileName.trim() === '') {
    return true;
  }

  const normalizedPath = fileName.replace(/\\/g, '/');
  const fileNameOnly = normalizedPath.split('/').pop() || '';
  const regex = getCompiledFileRegex();

  return regex.test(fileNameOnly) || regex.test(normalizedPath);
}

export function shouldIgnore(fullPath: string): boolean {
  return shouldIgnorePath(fullPath) || shouldIgnoreFile(fullPath);
}
