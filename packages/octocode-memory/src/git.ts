/**
 * git.ts — Git-based workspace/repo detection.
 * Pure functions: detectGit returns data; fillScope returns a NEW scope object.
 */

import { spawnSync } from 'node:child_process';
import { basename } from 'node:path';
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
 * Return a new scope object with workspace_path, repo, ref filled from git
 * when not already present in `partial`. NEVER mutates the input.
 */
export function fillScope(partial: ScopePartial, cwd?: string): Scope {
  const scope: Scope = {
    workspace_path: partial.workspace_path ?? null,
    repo: partial.repo ?? null,
    ref: partial.ref ?? null,
  };

  if (scope.workspace_path && scope.repo) return scope;

  const git = detectGit(cwd ?? process.cwd());
  if (!git.is_repo) return scope;

  if (!scope.workspace_path && git.root) scope.workspace_path = git.root;
  if (!scope.repo && git.repo) scope.repo = git.repo;
  if (!scope.ref && git.branch) scope.ref = git.branch;

  return scope;
}
