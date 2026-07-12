import path from 'node:path';
import type { ParsedArgs } from '../../types.js';
import { getBool, getString } from '../../options.js';
import { resolveRef, isGithubRef } from '../../routing.js';
import type { CliShorthandCorpus } from './types.js';

/** Resolve a target string to a corpus (local path vs GitHub ref). FS-aware. */
export function resolveCorpus(
  target: string | undefined,
  oqlTarget?: string,
  repoOption?: string,
  ownerOption?: string,
  sourceOverride?: string,
  pathOverride?: string
): CliShorthandCorpus {
  if (sourceOverride === 'npm') return { kind: 'npm' };
  if (!target && oqlTarget === 'packages') return { kind: 'npm' };
  if (target === 'npm') return { kind: 'npm' };
  if (sourceOverride === 'local') {
    return {
      kind: 'local',
      path: pathOverride
        ? path.resolve(pathOverride)
        : target
          ? path.resolve(target)
          : '.',
    };
  }
  if (ownerOption && repoOption && !repoOption.includes('/')) {
    const repoPath = normalizeRepoPath(pathOverride ?? target);
    return {
      kind: 'github',
      repo: `${ownerOption}/${repoOption}`,
      ...(repoPath ? { path: repoPath } : {}),
    };
  }
  if (repoOption) {
    const repo = resolveRef(repoOption);
    if (isGithubRef(repo)) {
      const repoPath = normalizeRepoPath(repo.subpath, pathOverride ?? target);
      return {
        kind: 'github',
        repo: `${repo.owner}/${repo.repo}`,
        ...(repoPath ? { path: repoPath } : {}),
        ...(repo.branch ? { ref: repo.branch } : {}),
      };
    }
  }
  if (sourceOverride === 'github' && target) {
    const ref = resolveRef(target);
    if (isGithubRef(ref)) {
      const repoPath = normalizeRepoPath(pathOverride ?? ref.subpath);
      return {
        kind: 'github',
        repo: `${ref.owner}/${ref.repo}`,
        ...(repoPath ? { path: repoPath } : {}),
        ...(ref.branch ? { ref: ref.branch } : {}),
      };
    }
  }
  if (!target) {
    return {
      kind: 'local',
      path: pathOverride ? path.resolve(pathOverride) : '.',
    };
  }
  const ref = resolveRef(target);
  if (isGithubRef(ref)) {
    const repoPath = normalizeRepoPath(pathOverride ?? ref.subpath);
    return {
      kind: 'github',
      repo: `${ref.owner}/${ref.repo}`,
      ...(repoPath ? { path: repoPath } : {}),
      ...(ref.branch ? { ref: ref.branch } : {}),
    };
  }
  return {
    kind: 'local',
    path: pathOverride ? path.resolve(pathOverride) : ref.path,
  };
}

export function normalizeRepoPath(
  ...parts: readonly (string | undefined)[]
): string {
  const joined = parts
    .map(part => part?.trim().replace(/^\/+|\/+$/g, ''))
    .filter(Boolean)
    .join('/');
  if (!joined) return '';
  const normalized = path.posix.normalize(joined);
  if (
    normalized === '..' ||
    normalized.startsWith('../') ||
    normalized.includes('/../')
  ) {
    throw new Error('Remote path cannot contain path traversal segments.');
  }
  return normalized === '.' ? '' : normalized;
}

export function resolveSemanticsOp(
  options: ParsedArgs['options']
): string | undefined {
  return (
    getString(options, 'op') ||
    (getBool(options, 'symbols') ? 'documentSymbols' : undefined)
  );
}

export function normalizeEntryType(
  value: string | undefined
): 'file' | 'directory' | undefined {
  if (!value) return undefined;
  if (value === 'file' || value === 'f') return 'file';
  if (value === 'directory' || value === 'd') return 'directory';
  return undefined;
}
