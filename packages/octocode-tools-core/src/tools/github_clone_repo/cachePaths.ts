import { join } from 'node:path';
import { createHash } from 'node:crypto';

export function getReposBaseDir(octocodeDir: string): string {
  return getCloneBaseDir(octocodeDir);
}

export function getCloneBaseDir(octocodeDir: string): string {
  return join(octocodeDir, 'tmp', 'clone');
}

export function getTreeBaseDir(octocodeDir: string): string {
  return join(octocodeDir, 'tmp', 'tree');
}

function sparseSuffix(sparsePath?: string): string {
  if (!sparsePath) return '';
  const hash = createHash('sha256')
    .update(sparsePath)
    .digest('hex')
    .substring(0, 6);
  return `__sp_${hash}`;
}

// Branch names may contain `/` (e.g. `release/1.96`, `dependabot/npm/foo`).
// `join()` would otherwise turn each segment into its own directory level,
// but `walkCloneDirs`/eviction always descends exactly owner/repo/<one dir>
// — an extra level has no meta file at the level the walker checks, so GC
// deletes the whole parent as "orphaned", wiping a valid nested clone. Keep
// single-segment branch names (the common case) byte-identical to avoid
// invalidating existing caches; only slash-bearing branches get encoded into
// one segment, with a hash suffix to avoid collisions between branches that
// otherwise map to the same sanitized name.
function safeBranchSegment(branch: string): string {
  if (!/[\\/]/.test(branch)) return branch;
  const hash = createHash('sha256')
    .update(branch)
    .digest('hex')
    .substring(0, 8);
  return `${branch.replace(/[\\/]/g, '_')}__b_${hash}`;
}

export function getCloneDir(
  octocodeDir: string,
  owner: string,
  repo: string,
  branch: string,
  sparsePath?: string
): string {
  const dirName = `${safeBranchSegment(branch)}${sparseSuffix(sparsePath)}`;
  return join(getCloneBaseDir(octocodeDir), owner, repo, dirName);
}

export function getTreeDir(
  octocodeDir: string,
  owner: string,
  repo: string,
  branch: string
): string {
  return join(
    getTreeBaseDir(octocodeDir),
    owner,
    repo,
    safeBranchSegment(branch)
  );
}
