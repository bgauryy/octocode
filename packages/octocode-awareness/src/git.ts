/**
 * git.ts — Git-based workspace/repo detection.
 * Pure functions: detectGit returns data; fillScope returns a NEW scope object.
 */

import { spawnSync } from 'node:child_process';
import { realpathSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import type { Scope, ScopePartial } from './types.js';

export interface GitInfo {
  is_repo: false;
}

export interface GitRepo {
  is_repo: true;
  root: string;
  repo: string;
  branch: string | null;
  remote: string | null;
}

export type GitResult = GitInfo | GitRepo;

function runCmd(cmd: string, args: string[], cwd?: string): string | null {
  try {
    const r = spawnSync(cmd, args, { cwd: cwd ?? process.cwd(), encoding: 'utf8', timeout: 5000 });
    return r.status === 0 ? (r.stdout as string).trim() : null;
  } catch {
    return null;
  }
}

/**
 * Detect git repo info for a working directory.
 */
export function detectGit(cwd?: string): GitResult {
  const root = runCmd('git', ['-C', cwd ?? '.', 'rev-parse', '--show-toplevel']);
  if (!root) return { is_repo: false };

  const branch = runCmd('git', ['-C', root, 'rev-parse', '--abbrev-ref', 'HEAD']);
  const remote = runCmd('git', ['-C', root, 'remote', 'get-url', 'origin']);
  const repoName = remote
    ? (remote.match(/github\.com[:/]([^/]+\/[^/]+?)(?:\.git)?$/) ?? [])[1] ?? basename(root)
    : basename(root);

  return { is_repo: true, root, repo: repoName, branch, remote };
}

/**
 * Canonicalize a path for scope-key purposes: resolve symlinks on the longest
 * existing ancestor and rejoin any not-yet-created tail segments.
 *
 * Without this, the SAME workspace can hash to two different scope keys —
 * e.g. macOS `/tmp` is a symlink to `/private/tmp`, and `git rev-parse
 * --show-toplevel` resolves symlinks while a plain `path.resolve()` does not.
 * A memory recorded via `path.resolve()` before a directory is a git repo
 * (or on a symlinked path) would otherwise become invisible once the same
 * directory is queried through git-root resolution (e.g. after `git init`),
 * with no error — just fewer results. Applying this canonicalization up
 * front, independent of git-repo detection, keeps the scope key stable
 * regardless of symlink components or git-init timing.
 */
export function canonicalizePath(input: string): string {
  let dir = resolve(input);
  const tail: string[] = [];
  for (let guard = 0; guard < 4096; guard += 1) {
    try {
      return tail.length ? join(realpathSync(dir), ...tail) : realpathSync(dir);
    } catch {
      const parent = dirname(dir);
      if (parent === dir) return resolve(input); // reached filesystem root
      tail.unshift(basename(dir));
      dir = parent;
    }
  }
  return resolve(input);
}

/**
 * Return a new scope object with workspace_path, repo, ref filled from git
 * when not already present in `partial`. NEVER mutates the input.
 */
export function fillScope(partial: ScopePartial, cwd?: string): Scope {
  const explicitWorkspace = partial.workspace_path ? canonicalizePath(partial.workspace_path) : null;
  const scope: Scope = {
    workspace_path: explicitWorkspace,
    artifact: partial.artifact ?? null,
    repo: partial.repo ?? null,
    ref: partial.ref ?? null,
  };

  // Detect from the explicit workspace when given — falling back to cwd here
  // used to tag a non-git workspace with whatever repo the process ran from.
  const git = detectGit(scope.workspace_path ?? cwd ?? process.cwd());
  if (!git.is_repo) return scope;

  // Workspace scope is repo-root based. If a caller passes a package/subdir as
  // workspace_path, normalize it so sibling package recalls meet the same row.
  // git.root is already symlink-resolved by `git rev-parse`, but canonicalize
  // it too so both code paths agree even if a future git version changes that.
  if (git.root) scope.workspace_path = canonicalizePath(git.root);
  if (!scope.repo && git.repo) scope.repo = git.repo;
  if (!scope.ref && git.branch) scope.ref = git.branch;

  return scope;
}

/**
 * Normalize a workspace filter/storage key the same way memory/refinement scope
 * does: an explicit path inside a git worktree becomes that repo root; a non-git
 * path remains an absolute path. Returns null only when no workspace/cwd exists.
 */
export function normalizeWorkspacePath(workspacePath?: string | null, cwd?: string): string | null {
  const candidate = workspacePath ? resolve(workspacePath) : cwd ? resolve(cwd) : null;
  const scope = fillScope({ workspace_path: candidate }, candidate ?? process.cwd());
  if (scope.workspace_path) return scope.workspace_path;
  return candidate;
}
