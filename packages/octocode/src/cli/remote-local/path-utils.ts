import path from 'node:path';
import { resolveRef, isGithubRef, type GithubRef } from '../routing.js';
import type { RemoteLocationKind, RemoteMaterializationKind } from './types.js';

export function normalizeRepoPath(
  ...parts: readonly (string | undefined)[]
): string {
  const joined = parts
    .map(part => part?.trim())
    .filter((part): part is string => Boolean(part && part !== '.'))
    .join('/');
  if (!joined) return '';
  if (path.posix.isAbsolute(joined)) {
    throw new Error('Remote path must be repository-relative.');
  }
  if (joined.split('/').some(segment => segment === '..')) {
    throw new Error('Remote path cannot contain path traversal segments.');
  }

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

export function cloneSparsePathFor(
  requestedPath: string,
  kind: RemoteMaterializationKind
): string | undefined {
  if (!requestedPath) return undefined;
  if (kind !== 'file') return requestedPath;
  const parent = path.posix.dirname(requestedPath);
  return parent === '.' ? undefined : parent;
}

export function resolveRepoOption(repoRef: string, branch?: string): GithubRef {
  const ref = resolveRef(repoRef, branch || undefined);
  if (!isGithubRef(ref)) {
    throw new Error(`--repo must be a GitHub ref, got "${repoRef}".`);
  }
  return ref;
}

/**
 * Maps a materialization request kind to the structural `location.kind`.
 * A `tree` materialization lands on disk as a directory.
 */
export function locationKindFor(
  kind: RemoteMaterializationKind
): RemoteLocationKind {
  if (kind === 'tree') return 'directory';
  return kind;
}

export function isFullRepoOption(options: {
  readonly repo?: string;
  readonly owner?: string;
}): boolean {
  return Boolean(
    options.repo?.includes('/') || (!options.owner && options.repo)
  );
}
