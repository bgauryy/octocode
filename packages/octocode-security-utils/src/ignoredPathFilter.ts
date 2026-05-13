import { IGNORED_PATH_PATTERNS } from './pathPatterns.js';
import { IGNORED_FILE_PATTERNS } from './filePatterns.js';
import { securityRegistry } from './registry.js';

let _compiledPathRegex: RegExp | null = null;
let _compiledFileRegex: RegExp | null = null;
let _cachedVersion = -1;

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
    _compiledPathRegex = new RegExp(all.map(r => r.source).join('|'));
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
    _compiledFileRegex = new RegExp(all.map(r => r.source).join('|'));
  }
  return _compiledFileRegex;
}

/**
 * Checks if a path should be ignored.
 * @param pathToCheck - The path to check (relative or absolute)
 * @returns true if the path should be ignored
 *
 * @example
 * ```ts
 * shouldIgnorePath('.aws/credentials'); // true
 * shouldIgnorePath('src/index.ts');     // false
 * ```
 */
export function shouldIgnorePath(pathToCheck: string): boolean {
  if (!pathToCheck || pathToCheck.trim() === '') {
    return true;
  }

  const normalizedPath = pathToCheck.replace(/\\/g, '/');
  const regex = getCompiledPathRegex();

  const pathParts = normalizedPath.split('/');
  for (const part of pathParts) {
    if (regex.test(part)) return true;
  }

  return regex.test(normalizedPath);
}

/**
 * Checks if a file should be ignored.
 * @param fileName - The file name or full path to check
 * @returns true if the file should be ignored
 *
 * @example
 * ```ts
 * shouldIgnoreFile('.env.local');   // true
 * shouldIgnoreFile('package.json'); // false
 * ```
 */
export function shouldIgnoreFile(fileName: string): boolean {
  if (!fileName || fileName.trim() === '') {
    return true;
  }

  const normalizedPath = fileName.replace(/\\/g, '/');
  const fileNameOnly = normalizedPath.split('/').pop() || '';
  const regex = getCompiledFileRegex();

  return regex.test(fileNameOnly) || regex.test(normalizedPath);
}

/**
 * Combined check for both path and file filtering.
 * @param fullPath - The full path to check
 * @returns true if the path or file should be ignored
 *
 * @example
 * ```ts
 * shouldIgnore('/app/.git/config'); // true
 * shouldIgnore('/app/.env');        // true
 * shouldIgnore('/app/src/index.ts');// false
 * ```
 */
export function shouldIgnore(fullPath: string): boolean {
  return shouldIgnorePath(fullPath) || shouldIgnoreFile(fullPath);
}
