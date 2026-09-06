import { spawnSync } from 'node:child_process';
import { shortId } from './ids.js';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { extensionHome } from '../extension-paths.js';
import type { WorkerWorktreeState } from '../types.js';
import { ensurePrivateDirectory, hardenPrivateFile, PRIVATE_FILE_MODE } from '@octocodeai/agent-contracts/permissions';

export type WorktreeIsolation = 'shared' | 'worktree';

export interface InternalWorktreeState extends WorkerWorktreeState {
  parentCwd: string;
  repoKey: string;
  metaPath: string;
}

export interface CreateWorktreeOptions {
  parentCwd: string;
  agentId: string;
  name: string;
  includeUncommitted?: boolean;
  home?: string;
}

export interface GitResult {
  stdout: string;
  stderr: string;
}

export type WorktreeGitRunner = (cwd: string, args: string[]) => GitResult;

const DEFAULT_GIT_TIMEOUT_MS = 15_000;
const META_SUFFIX = '.json';

const defaultGitRunner: WorktreeGitRunner = (cwd, args) => {
  const result = spawnSync('git', args, {
    cwd,
    encoding: 'utf8',
    timeout: DEFAULT_GIT_TIMEOUT_MS,
    maxBuffer: 10 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    const stderr = String(result.stderr ?? '').trim();
    const stdout = String(result.stdout ?? '').trim();
    throw new Error(`git ${args.join(' ')} failed${stderr || stdout ? `: ${stderr || stdout}` : ''}`);
  }
  return { stdout: String(result.stdout ?? ''), stderr: String(result.stderr ?? '') };
};

let gitRunner: WorktreeGitRunner = defaultGitRunner;

export function setWorktreeGitRunnerForTests(runner: WorktreeGitRunner | null): void {
  gitRunner = runner ?? defaultGitRunner;
}

function git(cwd: string, args: string[]): string {
  return gitRunner(cwd, args).stdout.trim();
}

function hashRepoKey(commonDir: string): string {
  return createHash('sha256').update(path.resolve(commonDir)).digest('hex').slice(0, 16);
}

function safeBranchName(name: string): string {
  const cleaned = name.toLowerCase().replace(/[^a-z0-9._-]+/g, '-').replace(/^-+|-+$/g, '');
  return cleaned || 'worker';
}

function countNonEmptyLines(text: string): number {
  return text.split(/\r?\n/).filter((line) => line.trim()).length;
}

function metaPathFor(worktreePath: string): string {
  return `${worktreePath}${META_SUFFIX}`;
}

function writeMeta(state: InternalWorktreeState): void {
  fs.writeFileSync(
    state.metaPath,
    JSON.stringify({
      path: state.path,
      branch: state.branch,
      baseCommit: state.baseCommit,
      parentCwd: state.parentCwd,
      repoKey: state.repoKey,
    }, null, 2),
    { encoding: 'utf8', mode: PRIVATE_FILE_MODE },
  );
  hardenPrivateFile(state.metaPath);
}

function removeMeta(state: InternalWorktreeState): void {
  try { fs.rmSync(state.metaPath, { force: true }); } catch { /* best-effort */ }
}

export function assertWorktreeSpawnAllowed(cwd: string): void {
  const inside = git(cwd, ['rev-parse', '--is-inside-work-tree']);
  if (inside !== 'true') throw new Error('isolation:"worktree" requires cwd to be inside a git work tree.');
  const root = git(cwd, ['rev-parse', '--show-toplevel']);
  if (fs.existsSync(path.join(root, '.gitmodules'))) {
    throw new Error('isolation:"worktree" is not supported for repositories with submodules yet.');
  }
}

export function createAgentWorktree(opts: CreateWorktreeOptions): InternalWorktreeState {
  const parentCwd = path.resolve(opts.parentCwd);
  assertWorktreeSpawnAllowed(parentCwd);
  const baseCommit = git(parentCwd, ['rev-parse', 'HEAD']);
  const commonDir = git(parentCwd, ['rev-parse', '--path-format=absolute', '--git-common-dir']);
  const repoKey = hashRepoKey(commonDir);
  const id8 = shortId(opts.agentId);
  const branch = `octocode/agents/${safeBranchName(opts.name)}-${id8}`;
  const worktreeRoot = path.join(extensionHome(opts.home), 'worktrees', repoKey);
  const worktreePath = path.join(worktreeRoot, id8);
  ensurePrivateDirectory(worktreeRoot);

  let state: InternalWorktreeState | undefined;
  try {
    git(parentCwd, ['worktree', 'add', '--no-track', '-b', branch, worktreePath, baseCommit]);
    state = {
      path: worktreePath,
      branch,
      baseCommit,
      dirtyFiles: 0,
      aheadCommits: 0,
      mergeState: 'clean',
      parentCwd,
      repoKey,
      metaPath: metaPathFor(worktreePath),
    };
    writeMeta(state);

    if (opts.includeUncommitted) {
      const snapshot = git(parentCwd, ['stash', 'create']);
      if (snapshot) git(worktreePath, ['stash', 'apply', snapshot]);
    }
    return refreshWorktreeState(state);
  } catch (error) {
    if (state) removeAgentWorktree(state, { force: true });
    else {
      try { git(parentCwd, ['worktree', 'remove', '--force', worktreePath]); } catch { /* best-effort */ }
      try { git(parentCwd, ['branch', '-D', branch]); } catch { /* best-effort */ }
      try { fs.rmSync(metaPathFor(worktreePath), { force: true }); } catch { /* best-effort */ }
    }
    throw error;
  }
}

export function refreshWorktreeState(state: InternalWorktreeState): InternalWorktreeState {
  const dirtyFiles = countNonEmptyLines(git(state.path, ['status', '--porcelain']));
  const aheadCommits = Number(git(state.path, ['rev-list', '--count', `${state.baseCommit}..HEAD`]) || '0');
  let mergeState: WorkerWorktreeState['mergeState'];
  try {
    git(state.path, ['rev-parse', '-q', '--verify', 'MERGE_HEAD']);
    mergeState = 'conflict';
  } catch {
    mergeState = dirtyFiles === 0 && aheadCommits === 0 ? 'clean' : 'unmerged';
  }
  state.dirtyFiles = dirtyFiles;
  state.aheadCommits = aheadCommits;
  if (state.mergeState !== 'merged' && state.mergeState !== 'discarded') state.mergeState = mergeState;
  return state;
}

export function removeAgentWorktree(state: InternalWorktreeState, opts: { force?: boolean } = {}): void {
  const removeArgs = ['worktree', 'remove', opts.force ? '--force' : undefined, state.path].filter(Boolean) as string[];
  try { git(state.parentCwd, removeArgs); } catch (err) {
    if (!opts.force) {
      const message = err instanceof Error ? err.message : String(err);
      // git refuses to remove a worktree with local changes; only then is
      // "unmerged work" the accurate diagnosis. Any other failure (missing
      // worktree, locked, unexpected git error) should surface as-is.
      if (/contains modified or untracked|unmerged|not empty|use --force/i.test(message)) {
        throw new Error(`worktree ${state.path} has unmerged work; refusing to remove without force.`);
      }
      throw new Error(`failed to remove worktree ${state.path}: ${message}`);
    }
    try { fs.rmSync(state.path, { recursive: true, force: true }); } catch { /* best-effort */ }
  }
  try { git(state.parentCwd, ['branch', '-D', state.branch]); } catch { /* best-effort */ }
  state.mergeState = 'discarded';
  removeMeta(state);
}

export function cleanupWorktreeIfNoWork(state: InternalWorktreeState): 'removed' | 'kept' {
  const refreshed = refreshWorktreeState(state);
  if (refreshed.dirtyFiles === 0 && refreshed.aheadCommits === 0) {
    removeAgentWorktree(refreshed, { force: false });
    return 'removed';
  }
  refreshed.mergeState = refreshed.mergeState === 'conflict' ? 'conflict' : 'unmerged';
  return 'kept';
}

function readMeta(metaPath: string): InternalWorktreeState | undefined {
  try {
    const raw = JSON.parse(fs.readFileSync(metaPath, 'utf8')) as Partial<InternalWorktreeState>;
    if (!raw.path || !raw.branch || !raw.baseCommit || !raw.parentCwd || !raw.repoKey) return undefined;
    return {
      path: raw.path,
      branch: raw.branch,
      baseCommit: raw.baseCommit,
      dirtyFiles: 0,
      aheadCommits: 0,
      mergeState: 'clean',
      parentCwd: raw.parentCwd,
      repoKey: raw.repoKey,
      metaPath,
    };
  } catch {
    return undefined;
  }
}

export function sweepAgentWorktrees(parentCwd: string, liveWorktreePaths: Iterable<string> = [], home?: string): number {
  assertWorktreeSpawnAllowed(parentCwd);
  try { git(parentCwd, ['worktree', 'prune']); } catch { /* best-effort */ }
  const commonDir = git(parentCwd, ['rev-parse', '--path-format=absolute', '--git-common-dir']);
  const root = path.join(extensionHome(home), 'worktrees', hashRepoKey(commonDir));
  if (!fs.existsSync(root)) return 0;
  const live = new Set([...liveWorktreePaths].map((p) => path.resolve(p)));
  let removed = 0;
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    if (!entry.isFile() || !entry.name.endsWith(META_SUFFIX)) continue;
    const metaFile = path.join(root, entry.name);
    const meta = readMeta(metaFile);
    if (!meta || live.has(path.resolve(meta.path))) continue;
    // The worktree directory is gone (pruned/deleted out from under us); the
    // sidecar meta json would otherwise linger forever. Drop it best-effort.
    if (!fs.existsSync(meta.path)) {
      try { fs.rmSync(metaFile, { force: true }); } catch { /* best-effort */ }
      continue;
    }
    try {
      if (cleanupWorktreeIfNoWork(meta) === 'removed') removed += 1;
    } catch {
      // Kept worktrees with work or odd git failures need explicit user review.
    }
  }
  return removed;
}
