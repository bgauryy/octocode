import { execFile } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { promisify } from 'node:util';
import { atomicWriteFileSync, sha256 } from '../shared/atomic.js';
import { findRepoRoot, HOME_NAMES, octocodeHome } from '../shared/home.js';
import { processAlive } from '../shared/process.js';
import { errorMessage } from '../shared/util.js';

/**
 * Git worktree isolation for implementer subagents, adapted from the native host's worker worktrees: each isolated
 * subagent works in a detached worktree under `<Octocode home>/pi-worktrees/<repo-hash>/<agent-id>`; on finish its
 * changes are committed to `refs/octocode/pi/<agent-id>` and the worktree is removed; when saving fails the worktree is
 * kept so nothing is lost. Merging stays manual (`/agents merge`).
 * Ignored files (`node_modules`, builds) are not in the worktree.
 */

const REF_PREFIX = 'refs/octocode/pi/';
/** Commits and merges made here never run the repository's hooks. */
const NO_HOOKS = ['-c', 'core.hooksPath=/dev/null'];
const ID_PATTERN = /^[A-Za-z0-9._-]{1,80}$/;

const execFileAsync = promisify(execFile);

async function git(cwd: string, args: string[]): Promise<string> {
  try {
    const { stdout } = await execFileAsync('git', args, { cwd, maxBuffer: 16 * 1024 * 1024, env: { ...process.env, GIT_TERMINAL_PROMPT: '0' } });
    return stdout.trimEnd();
  } catch (error) {
    const stderr = (error as { stderr?: string }).stderr?.trim();
    throw new Error(stderr || errorMessage(error));
  }
}

const quiet = (work: Promise<string>): Promise<string | undefined> => work.catch(() => undefined);

export interface Worktree {
  agentId: string;
  /** Main repository root. */
  repo: string;
  /** The worktree directory. */
  path: string;
  /** The commit the worktree started from. */
  base: string;
  /** Where the subagent runs: the worktree, at the same subdirectory as the parent's cwd. */
  cwd: string;
  /** Set when the main tree had uncommitted changes, which the worktree does not see. */
  warning?: string;
}

interface WorktreeResult {
  /** The private ref holding the subagent's changes; absent when it changed nothing. */
  ref?: string;
  /** `git diff --stat` of those changes. */
  stat?: string;
}

/** The team workspace for `cwd`, as the team store computes it: the nearest directory holding `.git`, else cwd. */
export const teamWorkspace = findRepoRoot;

export function worktreesDir(repo: string, home = octocodeHome()): string {
  return path.join(home, HOME_NAMES.worktrees, sha256(repo).slice(0, 16));
}

/** The repository root, found the same way as the team workspace (`findRepoRoot`), so both always agree. */
function repoRoot(cwd: string): string {
  const root = findRepoRoot(cwd);
  if (!fs.existsSync(path.join(root, '.git'))) throw new Error('isolate needs a git repository; run without isolate here.');
  return root;
}

/** Creates a detached worktree at HEAD for `agentId`. Refuses outside a git repository or before the first commit. */
export async function createWorktree(cwd: string, agentId: string, home = octocodeHome()): Promise<Worktree> {
  if (!ID_PATTERN.test(agentId)) throw new Error(`Invalid agent id "${agentId}"`);
  const repo = repoRoot(cwd);
  const base = await quiet(git(repo, ['rev-parse', '--verify', 'HEAD']));
  if (!base) throw new Error('isolate needs at least one commit to start the worktree from.');
  const dirty = await git(repo, ['status', '--porcelain']);
  const dir = worktreesDir(repo, home);
  const worktree = path.join(dir, agentId);
  fs.mkdirSync(dir, { recursive: true });
  // Never reuse a kept worktree's id: overwriting (then removing) its record would hide it from prune and reports.
  if (fs.existsSync(worktree) || fs.existsSync(`${worktree}.json`)) throw new Error(`A worktree for ${agentId} already exists at ${worktree}; start the agent again for a new id.`);
  // The owner record lets a later session prune worktrees whose parent process died.
  atomicWriteFileSync(`${worktree}.json`, JSON.stringify({ pids: [process.pid], repo, base, createdAt: Date.now() }));
  try {
    await git(repo, ['worktree', 'add', '--detach', worktree, base]);
  } catch (error) {
    fs.rmSync(`${worktree}.json`, { force: true });
    throw error;
  }
  const sub = path.relative(repo, path.resolve(cwd));
  const childCwd = sub && !sub.startsWith('..') && fs.existsSync(path.join(worktree, sub)) ? path.join(worktree, sub) : worktree;
  return {
    agentId,
    repo,
    path: worktree,
    base,
    cwd: childCwd,
    ...(dirty ? { warning: 'The main tree has uncommitted changes; the isolated subagent starts from HEAD without them.' } : {}),
  };
}

/** Adds the subagent's own pid to the owner record, so a parent crash does not let a prune remove the worktree under a live child. */
export function recordWorktreePid(worktree: Pick<Worktree, 'path'>, pid: number): void {
  const meta = `${worktree.path}.json`;
  try {
    const owner = JSON.parse(fs.readFileSync(meta, 'utf8')) as Record<string, unknown>;
    owner.pids = [...new Set([...ownerPids(owner), pid])];
    atomicWriteFileSync(meta, JSON.stringify(owner));
  } catch {
    // The record is gone: the worktree was already finished.
  }
}

function ownerPids(owner: { pids?: unknown }): unknown[] {
  return Array.isArray(owner.pids) ? owner.pids : [];
}

/** An owner record this young that cannot be read may be mid-write by another session: leave its worktree alone. */
const UNREADABLE_GRACE_MS = 60_000;

/**
 * Commits whatever changed in the worktree to the agent's private ref, then removes the worktree. The worktree is
 * removed only once its changes are safe on the ref (or it changed nothing); when saving fails it is kept, with its
 * owner record, and the error names where it is.
 */
export async function finishWorktree(worktree: Pick<Worktree, 'agentId' | 'repo' | 'path' | 'base'>): Promise<WorktreeResult> {
  if (!fs.existsSync(worktree.path)) {
    await removeWorktree(worktree);
    return {};
  }
  let result: WorktreeResult;
  try {
    result = await saveChanges(worktree);
  } catch (error) {
    throw new Error(`${errorMessage(error)}. The worktree is kept at ${worktree.path}; commit or copy its changes, then remove it with \`git worktree remove --force ${worktree.path}\`.`);
  }
  await removeWorktree(worktree);
  return result;
}

async function saveChanges(worktree: Pick<Worktree, 'agentId' | 'repo' | 'path' | 'base'>): Promise<WorktreeResult> {
  if (await git(worktree.path, ['status', '--porcelain'])) {
    await git(worktree.path, ['add', '-A']);
    const identity = (await quiet(git(worktree.path, ['config', 'user.email']))) ? [] : ['-c', 'user.name=Octocode', '-c', 'user.email=octocode@localhost'];
    await git(worktree.path, [...NO_HOOKS, ...identity, '-c', 'commit.gpgsign=false', 'commit', '--no-verify', '-q', '-m', `octocode subagent ${worktree.agentId}`]);
    // Anything still uncommitted (a failed partial commit) would be lost with the worktree.
    if (await git(worktree.path, ['status', '--porcelain'])) throw new Error('Uncommitted changes remain after the commit');
  }
  const head = await git(worktree.path, ['rev-parse', 'HEAD']);
  const base = worktree.base || (await recoverBase(worktree.repo, head));
  if (head === base) return {};
  const ref = await saveRef(worktree.repo, `${REF_PREFIX}${worktree.agentId}`, head);
  return { ref, stat: await git(worktree.repo, ['diff', '--stat', base, head]) };
}

/**
 * The start commit of a worktree whose owner record was unreadable: where its head meets the main tree's HEAD. Without
 * one there is no safe diff or ref, so the worktree is kept (and nothing is written) rather than saved under a guess.
 */
async function recoverBase(repo: string, head: string): Promise<string> {
  const main = await quiet(git(repo, ['rev-parse', 'HEAD']));
  const base = main && (await quiet(git(repo, ['merge-base', head, main])));
  if (!base) throw new Error('its owner record is unreadable and its start commit could not be found');
  return base;
}

/** Creates `ref` (or `ref-2`, `ref-3`, … when taken) at `head`; never overwrites an unmerged ref of an earlier agent with the same id. */
async function saveRef(repo: string, ref: string, head: string): Promise<string> {
  for (let n = 1; n <= 100; n++) {
    const candidate = n === 1 ? ref : `${ref}-${n}`;
    // An empty old value makes update-ref fail when the ref already exists.
    if ((await quiet(git(repo, ['update-ref', candidate, head, '']))) !== undefined) return candidate;
  }
  throw new Error(`Could not save the subagent's changes: ${ref} and its fallbacks are taken.`);
}

async function removeWorktree(worktree: Pick<Worktree, 'repo' | 'path'>): Promise<void> {
  if ((await quiet(git(worktree.repo, ['worktree', 'remove', '--force', worktree.path]))) === undefined) {
    fs.rmSync(worktree.path, { recursive: true, force: true });
    await quiet(git(worktree.repo, ['worktree', 'prune']));
  }
  fs.rmSync(`${worktree.path}.json`, { force: true });
}

/** The lines appended to an isolated subagent's report. */
export function isolationReport(agentId: string, result: WorktreeResult | { error: string }): string {
  if ('error' in result) return `\n\nIsolated worktree for ${agentId} could not be saved: ${result.error}`;
  if (!result.ref) return `\n\nIsolated run ${agentId} changed no files.`;
  return `\n\nIsolated changes are on ${result.ref}, not in this tree:\n${result.stat}\nMerge with \`/agents merge ${result.ref.slice(REF_PREFIX.length)}\` or \`git merge --no-ff ${result.ref}\`.`;
}

/** Merges an isolated subagent's ref into the main tree with hooks disabled; on conflict aborts and lists the paths. */
export async function mergeAgent(cwd: string, agentId: string): Promise<{ ok: boolean; text: string }> {
  if (!ID_PATTERN.test(agentId)) return { ok: false, text: `Invalid agent id "${agentId}"` };
  const repo = repoRoot(cwd);
  const ref = `${REF_PREFIX}${agentId}`;
  if (!(await quiet(git(repo, ['rev-parse', '--verify', '--quiet', ref])))) return { ok: false, text: `No isolated changes for ${agentId} (${ref} does not exist).` };
  try {
    await git(repo, [...NO_HOOKS, 'merge', '--no-ff', '--no-edit', ref]);
  } catch (error) {
    const conflicts = (await quiet(git(repo, ['diff', '--name-only', '--diff-filter=U']))) ?? '';
    if (await quiet(git(repo, ['rev-parse', '--verify', '--quiet', 'MERGE_HEAD']))) await quiet(git(repo, ['merge', '--abort']));
    if (conflicts) return { ok: false, text: `Merging ${ref} conflicts in:\n${conflicts}\nThe merge was aborted and the tree is unchanged; ${ref} is kept.` };
    return { ok: false, text: `Merging ${ref} failed: ${errorMessage(error)}` };
  }
  await quiet(git(repo, ['update-ref', '-d', ref]));
  return { ok: true, text: `Merged ${ref} into ${(await quiet(git(repo, ['rev-parse', '--abbrev-ref', 'HEAD']))) ?? 'HEAD'}.` };
}

/** Refs of isolated results not merged yet. */
export async function pendingRefs(cwd: string): Promise<string[]> {
  const out = await quiet(git(cwd, ['for-each-ref', '--format=%(refname)', REF_PREFIX]));
  return out ? out.split('\n').filter(Boolean).map((ref) => ref.slice(REF_PREFIX.length)) : [];
}

interface PruneReport {
  /** Orphaned worktrees removed after their changes were saved; `ref` is absent when one changed nothing. */
  saved: Array<{ agentId: string; ref?: string }>;
  /** Orphaned worktrees kept because their changes could not be saved. */
  kept: Array<{ agentId: string; error: string }>;
}

/**
 * Removes worktrees of this repository whose owning Pi processes (parent and subagent) are all gone. Their changes are
 * first committed to the agent's ref; a worktree whose changes cannot be saved is kept and reported, so a crash never
 * loses an isolated subagent's work.
 */
export async function pruneWorktrees(cwd: string, home = octocodeHome()): Promise<PruneReport> {
  const report: PruneReport = { saved: [], kept: [] };
  let repo: string;
  try {
    repo = repoRoot(cwd);
  } catch {
    return report;
  }
  const dir = worktreesDir(repo, home);
  if (!fs.existsSync(dir)) return report;
  for (const entry of fs.readdirSync(dir)) {
    if (!entry.endsWith('.json')) continue;
    const agentId = entry.slice(0, -'.json'.length);
    let owner: { pids?: unknown; base?: unknown } = {};
    try {
      owner = JSON.parse(fs.readFileSync(path.join(dir, entry), 'utf8')) as typeof owner;
    } catch {
      // Unreadable: orphaned, unless it is fresh enough to be another session's record still being created.
      if (Date.now() - (fs.statSync(path.join(dir, entry), { throwIfNoEntry: false })?.mtimeMs ?? 0) < UNREADABLE_GRACE_MS) continue;
    }
    if (ownerPids(owner).some(processAlive) || !ID_PATTERN.test(agentId)) continue;
    const worktree = { agentId, repo, path: path.join(dir, agentId), base: typeof owner.base === 'string' ? owner.base : '' };
    try {
      const { ref } = await finishWorktree(worktree);
      report.saved.push({ agentId, ...(ref ? { ref } : {}) });
    } catch (error) {
      report.kept.push({ agentId, error: errorMessage(error) });
    }
  }
  await quiet(git(repo, ['worktree', 'prune']));
  return report;
}

/** What a prune did, for the user: saved refs as info, kept worktrees as a warning; undefined when nothing was orphaned. */
export function describePrune(report: PruneReport): { text: string; level: 'info' | 'warning' } | undefined {
  const lines = [
    ...(report.saved.length ? [`Removed ${report.saved.length} orphaned subagent worktree(s): ${report.saved.map(({ agentId, ref }) => (ref ? `${agentId} (changes on ${ref})` : `${agentId} (no changes)`)).join(', ')}.`] : []),
    ...report.kept.map(({ agentId, error }) => `Kept orphaned subagent worktree ${agentId}: ${error}`),
  ];
  return lines.length ? { text: lines.join('\n'), level: report.kept.length ? 'warning' : 'info' } : undefined;
}
